// Consumed by the auth API in later tasks.
#![allow(dead_code)]

use hbb_common::{
    anyhow::anyhow,
    rand::{rngs::OsRng, Rng},
    ResultType,
};
use sqlx::{sqlite::SqliteRow, Row, SqlitePool};

const BCRYPT_COST: u32 = 12;
const MIN_PASSWORD_LEN: usize = 8;
const GENERATED_PASSWORD_LEN: usize = 16;
// No 0/O/1/l/I to avoid ambiguity when the password is read from a log.
const PASSWORD_ALPHABET: &[u8] = b"abcdefghijkmnopqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789";

#[derive(Debug, Clone)]
pub(crate) struct User {
    pub guid: Vec<u8>,
    pub name: String,
    pub display_name: String,
    pub email: String,
    pub note: String,
    pub is_admin: bool,
    pub status: i64,
    pub must_change_password: bool,
}

const USER_COLUMNS: &str =
    "guid, name, display_name, email, note, is_admin, status, must_change_password";

fn row_to_user(row: &SqliteRow) -> Result<User, sqlx::Error> {
    Ok(User {
        guid: row.try_get("guid")?,
        name: row.try_get("name")?,
        display_name: row.try_get("display_name")?,
        email: row.try_get("email")?,
        note: row.try_get("note")?,
        is_admin: row.try_get::<i64, _>("is_admin")? != 0,
        status: row.try_get("status")?,
        must_change_password: row.try_get::<i64, _>("must_change_password")? != 0,
    })
}

pub(crate) fn hash_password(p: &str) -> ResultType<String> {
    Ok(bcrypt::hash(p, BCRYPT_COST)?)
}

pub(crate) fn verify_password(p: &str, hash: &str) -> bool {
    bcrypt::verify(p, hash).unwrap_or(false)
}

pub(crate) async fn find_by_name(
    pool: &SqlitePool,
    name: &str,
) -> ResultType<Option<(User, String)>> {
    let sql = format!("select {USER_COLUMNS}, password_hash from console_user where name = ?");
    let row = sqlx::query(&sql).bind(name).fetch_optional(pool).await?;
    match row {
        Some(row) => {
            let hash: String = row.try_get("password_hash")?;
            Ok(Some((row_to_user(&row)?, hash)))
        }
        None => Ok(None),
    }
}

pub(crate) async fn find_by_guid(pool: &SqlitePool, guid: &[u8]) -> ResultType<Option<User>> {
    let sql = format!("select {USER_COLUMNS} from console_user where guid = ?");
    let row = sqlx::query(&sql).bind(guid).fetch_optional(pool).await?;
    match row {
        Some(row) => Ok(Some(row_to_user(&row)?)),
        None => Ok(None),
    }
}

/// Returns whether the user was found. Enforces the minimum password length.
pub(crate) async fn set_password(
    pool: &SqlitePool,
    name: &str,
    password: &str,
    must_change: bool,
) -> ResultType<bool> {
    if password.chars().count() < MIN_PASSWORD_LEN {
        return Err(anyhow!(
            "password must be at least {} characters",
            MIN_PASSWORD_LEN
        ));
    }
    let hash = hash_password(password)?;
    let res = sqlx::query(
        "update console_user set password_hash = ?, must_change_password = ? where name = ?",
    )
    .bind(hash)
    .bind(must_change)
    .bind(name)
    .execute(pool)
    .await?;
    Ok(res.rows_affected() > 0)
}

fn generate_password() -> String {
    let mut rng = OsRng;
    (0..GENERATED_PASSWORD_LEN)
        .map(|_| PASSWORD_ALPHABET[rng.gen_range(0..PASSWORD_ALPHABET.len())] as char)
        .collect()
}

/// Creates the `admin` user when no console user exists. Returns the generated password.
pub(crate) async fn ensure_initial_admin(pool: &SqlitePool) -> ResultType<Option<String>> {
    let (count,): (i64,) = sqlx::query_as("select count(*) from console_user")
        .fetch_one(pool)
        .await?;
    if count > 0 {
        return Ok(None);
    }
    let password = generate_password();
    let hash = hash_password(&password)?;
    let guid = uuid::Uuid::new_v4().as_bytes().to_vec();
    sqlx::query(
        "insert into console_user(guid, name, password_hash, is_admin, status, must_change_password)
         values(?, 'admin', ?, 1, 1, 1)",
    )
    .bind(guid)
    .bind(hash)
    .execute(pool)
    .await?;
    Ok(Some(password))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::console::test_util::test_pool;

    async fn migrated_pool() -> sqlx::SqlitePool {
        let pool = test_pool().await;
        crate::console::db::migrations::migrate(&pool, ":memory:")
            .await
            .unwrap();
        pool
    }

    #[tokio::test]
    async fn initial_admin_created_once() {
        let pool = migrated_pool().await;
        let pw = ensure_initial_admin(&pool).await.unwrap().unwrap();
        assert_eq!(pw.len(), 16);
        let (user, hash) = find_by_name(&pool, "admin").await.unwrap().unwrap();
        assert!(user.is_admin);
        assert!(user.must_change_password);
        assert!(verify_password(&pw, &hash));
        assert!(ensure_initial_admin(&pool).await.unwrap().is_none());
    }

    #[test]
    fn verify_password_roundtrip() {
        let h = hash_password("correct horse").unwrap();
        assert!(verify_password("correct horse", &h));
        assert!(!verify_password("wrong", &h));
        assert!(!verify_password("correct horse", "not-a-hash"));
    }

    #[tokio::test]
    async fn set_password_rejects_short() {
        let pool = migrated_pool().await;
        ensure_initial_admin(&pool).await.unwrap();
        assert!(set_password(&pool, "admin", "1234567", false).await.is_err());
        assert!(set_password(&pool, "admin", "12345678", false).await.unwrap());
        assert!(!set_password(&pool, "nobody", "12345678", false).await.unwrap());
    }

    #[tokio::test]
    async fn find_by_name_case_insensitive() {
        let pool = migrated_pool().await;
        ensure_initial_admin(&pool).await.unwrap();
        let (user, _) = find_by_name(&pool, "ADMIN").await.unwrap().unwrap();
        assert_eq!(user.name, "admin");
        assert!(find_by_guid(&pool, &user.guid).await.unwrap().is_some());
    }
}
