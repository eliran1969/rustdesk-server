use crate::peer::PeerMap;
use hbb_common::ResultType;
use sqlx::{Row, SqlitePool};
use std::time::Duration;

pub(crate) const SYSINFO_VER: &str = "1";

/// Same window as `REG_TIMEOUT` (30_000 ms) in rendezvous_server.rs: a peer that has
/// registered within it counts as online.
const ONLINE_TIMEOUT: Duration = Duration::from_secs(30);

/// Returns the peer's guid only when a `peer` row has this id AND its `uuid` equals the
/// base64-decoded `uuid_b64`. Malformed base64 never matches.
pub(crate) async fn verify_peer(
    pool: &SqlitePool,
    id: &str,
    uuid_b64: &str,
) -> ResultType<Option<Vec<u8>>> {
    let uuid = match base64::decode(uuid_b64) {
        Ok(uuid) => uuid,
        Err(_) => return Ok(None),
    };
    let row = sqlx::query("select guid from peer where id = ? and uuid = ?")
        .bind(id)
        .bind(uuid)
        .fetch_optional(pool)
        .await?;
    match row {
        Some(row) => Ok(Some(row.try_get("guid")?)),
        None => Ok(None),
    }
}

pub(crate) async fn upsert_sysinfo(
    pool: &SqlitePool,
    guid: &[u8],
    sysinfo: &str,
) -> ResultType<()> {
    sqlx::query(
        "insert into console_peer_sysinfo(peer_guid, sysinfo) values(?, ?)
         on conflict(peer_guid) do update set sysinfo = excluded.sysinfo,
         updated_at = current_timestamp",
    )
    .bind(guid)
    .bind(sysinfo)
    .execute(pool)
    .await?;
    Ok(())
}

/// Sets `last_online`; returns false when the peer has no sysinfo row yet.
pub(crate) async fn touch_last_online(pool: &SqlitePool, guid: &[u8]) -> ResultType<bool> {
    let r = sqlx::query(
        "update console_peer_sysinfo set last_online = current_timestamp where peer_guid = ?",
    )
    .bind(guid)
    .execute(pool)
    .await?;
    Ok(r.rows_affected() > 0)
}

/// Assigns an unassigned peer to the enabled console user `name`. Writes only `peer.user`.
pub(crate) async fn assign_if_unassigned(
    pool: &SqlitePool,
    guid: &[u8],
    name: &str,
) -> ResultType<()> {
    let user = sqlx::query("select guid from console_user where name = ? and status = 1")
        .bind(name)
        .fetch_optional(pool)
        .await?;
    if let Some(user) = user {
        let user_guid: Vec<u8> = user.try_get("guid")?;
        sqlx::query("update peer set user = ? where guid = ? and user is null")
            .bind(user_guid)
            .bind(guid)
            .execute(pool)
            .await?;
    }
    Ok(())
}

pub(crate) async fn is_online(pm: &PeerMap, id: &str) -> bool {
    match pm.get_in_memory(id).await {
        Some(p) => p.read().await.last_reg_time.elapsed() < ONLINE_TIMEOUT,
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::console::test_util::test_pm;
    use std::time::Instant;

    #[tokio::test]
    async fn is_online_follows_last_reg_time() {
        let pm = test_pm().await;
        assert!(!is_online(&pm, "x1").await);
        let p = pm.get_or("x1").await;
        assert!(!is_online(&pm, "x1").await);
        p.write().await.last_reg_time = Instant::now();
        assert!(is_online(&pm, "x1").await);
    }
}
