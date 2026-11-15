// Consumed by the device management API in later tasks.
#![allow(dead_code)]

use sqlx::{Row, SqlitePool};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct PeerExt {
    pub peer_guid: Vec<u8>,
    pub device_group_guid: Option<Vec<u8>>,
    pub device_name: Option<String>,
    pub device_username: Option<String>,
}

pub(crate) async fn get(
    pool: &SqlitePool,
    peer_guid: &[u8],
) -> Result<Option<PeerExt>, sqlx::Error> {
    let row = sqlx::query(
        "select peer_guid, device_group_guid, device_name, device_username
         from console_peer_ext where peer_guid = ?",
    )
    .bind(peer_guid)
    .fetch_optional(pool)
    .await?;
    row.map(|r| {
        Ok(PeerExt {
            peer_guid: r.try_get("peer_guid")?,
            device_group_guid: r.try_get("device_group_guid")?,
            device_name: r.try_get("device_name")?,
            device_username: r.try_get("device_username")?,
        })
    })
    .transpose()
}

pub(crate) async fn set_device_group(
    pool: &SqlitePool,
    peer_guid: &[u8],
    group_guid: Option<&[u8]>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "insert into console_peer_ext(peer_guid, device_group_guid) values(?1, ?2)
         on conflict(peer_guid) do update set device_group_guid = excluded.device_group_guid",
    )
    .bind(peer_guid)
    .bind(group_guid)
    .execute(pool)
    .await?;
    Ok(())
}

pub(crate) async fn set_device_name(
    pool: &SqlitePool,
    peer_guid: &[u8],
    name: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "insert into console_peer_ext(peer_guid, device_name) values(?1, ?2)
         on conflict(peer_guid) do update set device_name = excluded.device_name",
    )
    .bind(peer_guid)
    .bind(name)
    .execute(pool)
    .await?;
    Ok(())
}

pub(crate) async fn set_device_username(
    pool: &SqlitePool,
    peer_guid: &[u8],
    username: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "insert into console_peer_ext(peer_guid, device_username) values(?1, ?2)
         on conflict(peer_guid) do update set device_username = excluded.device_username",
    )
    .bind(peer_guid)
    .bind(username)
    .execute(pool)
    .await?;
    Ok(())
}

pub(crate) async fn delete(pool: &SqlitePool, peer_guid: &[u8]) -> Result<(), sqlx::Error> {
    sqlx::query("delete from console_peer_ext where peer_guid = ?")
        .bind(peer_guid)
        .execute(pool)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::console::{db::migrations::migrate, test_util::test_pool};

    #[tokio::test]
    async fn overrides_roundtrip() {
        let pool = test_pool().await;
        migrate(&pool, ":memory:").await.unwrap();
        let g = vec![1u8; 16];
        assert!(get(&pool, &g).await.unwrap().is_none());
        set_device_name(&pool, &g, Some("PC")).await.unwrap();
        set_device_group(&pool, &g, Some(&[2u8; 16])).await.unwrap();
        set_device_username(&pool, &g, Some("bob")).await.unwrap();
        let e = get(&pool, &g).await.unwrap().unwrap();
        assert_eq!(e.device_name.as_deref(), Some("PC"));
        assert_eq!(e.device_group_guid, Some(vec![2u8; 16]));
        assert_eq!(e.device_username.as_deref(), Some("bob"));
        delete(&pool, &g).await.unwrap();
        assert!(get(&pool, &g).await.unwrap().is_none());
    }
}
