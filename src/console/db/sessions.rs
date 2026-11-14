use hbb_common::{
    rand::{rngs::OsRng, RngCore},
    sha2::{Digest, Sha256},
    ResultType,
};
use sqlx::SqlitePool;

const TOKEN_BYTES: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SessionKind {
    Web,
    Client,
}

impl SessionKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            SessionKind::Web => "web",
            SessionKind::Client => "client",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s {
            "web" => Some(SessionKind::Web),
            "client" => Some(SessionKind::Client),
            _ => None,
        }
    }
}

/// Session tokens are 32 random bytes, hex-encoded (64 lowercase chars). The database
/// stores only SHA-256 over the token's ASCII hex string (exactly what the client sends
/// in `Authorization: Bearer <token>`), never the token itself.
pub(crate) fn hash_token(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}

fn generate_token() -> String {
    let mut bytes = [0u8; TOKEN_BYTES];
    OsRng.fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Creates a session and returns the plain token (shown to the caller once, never stored).
pub(crate) async fn create_session(
    pool: &SqlitePool,
    user_guid: &[u8],
    kind: SessionKind,
    peer_id: Option<&str>,
    peer_uuid: Option<&str>,
) -> ResultType<String> {
    let token = generate_token();
    sqlx::query(
        "insert into console_session(token_hash, user_guid, kind, peer_id, peer_uuid)
         values(?, ?, ?, ?, ?)",
    )
    .bind(hash_token(&token))
    .bind(user_guid)
    .bind(kind.as_str())
    .bind(peer_id)
    .bind(peer_uuid)
    .execute(pool)
    .await?;
    Ok(token)
}

/// Looks up a session by token hash. Returns the owner's guid and the session kind.
pub(crate) async fn find_session(
    pool: &SqlitePool,
    token_hash: &[u8],
) -> ResultType<Option<(Vec<u8>, SessionKind)>> {
    let row: Option<(Vec<u8>, String)> =
        sqlx::query_as("select user_guid, kind from console_session where token_hash = ?")
            .bind(token_hash)
            .fetch_optional(pool)
            .await?;
    Ok(row.and_then(|(guid, kind)| SessionKind::parse(&kind).map(|k| (guid, k))))
}

pub(crate) async fn touch_session(pool: &SqlitePool, token_hash: &[u8]) -> ResultType<()> {
    sqlx::query("update console_session set last_seen = current_timestamp where token_hash = ?")
        .bind(token_hash)
        .execute(pool)
        .await?;
    Ok(())
}

pub(crate) async fn delete_session(pool: &SqlitePool, token_hash: &[u8]) -> ResultType<()> {
    sqlx::query("delete from console_session where token_hash = ?")
        .bind(token_hash)
        .execute(pool)
        .await?;
    Ok(())
}

/// Deletes every session of `user_guid` except the one identified by `keep_hash`.
pub(crate) async fn delete_other_sessions(
    pool: &SqlitePool,
    user_guid: &[u8],
    keep_hash: &[u8],
) -> ResultType<()> {
    sqlx::query("delete from console_session where user_guid = ? and token_hash != ?")
        .bind(user_guid)
        .bind(keep_hash)
        .execute(pool)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_is_64_hex_and_unique() {
        let a = generate_token();
        let b = generate_token();
        assert_eq!(a.len(), 64);
        assert!(a
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        assert_ne!(a, b);
        assert_eq!(hash_token(&a).len(), 32);
    }

    #[test]
    fn session_kind_roundtrip() {
        for k in [SessionKind::Web, SessionKind::Client] {
            assert_eq!(SessionKind::parse(k.as_str()), Some(k));
        }
        assert_eq!(SessionKind::parse("other"), None);
    }
}
