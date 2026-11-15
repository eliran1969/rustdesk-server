// Consumed by the auth API in later tasks.
#![allow(dead_code)]

use hbb_common::{
    anyhow::anyhow,
    rand::{rngs::OsRng, Rng},
    ResultType,
};
use sqlx::{sqlite::SqliteRow, Row, SqliteConnection, SqlitePool};

use super::sessions;

const BCRYPT_COST: u32 = 12;
pub(crate) const MIN_PASSWORD_LEN: usize = 8;
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

/// Stores an already computed bcrypt hash (lets callers hash off the async runtime).
/// Returns whether the user was found.
pub(crate) async fn update_password_hash(
    pool: &SqlitePool,
    guid: &[u8],
    hash: &str,
    must_change: bool,
) -> ResultType<bool> {
    let res = sqlx::query(
        "update console_user set password_hash = ?, must_change_password = ? where guid = ?",
    )
    .bind(hash)
    .bind(must_change)
    .bind(guid)
    .execute(pool)
    .await?;
    Ok(res.rows_affected() > 0)
}

/// Stores the new hash and revokes every other session of the user in one transaction:
/// either both happen or neither. Returns whether the user was found.
pub(crate) async fn change_password_revoking_sessions(
    pool: &SqlitePool,
    guid: &[u8],
    hash: &str,
    keep_session_hash: &[u8],
) -> ResultType<bool> {
    let mut tx = pool.begin().await?;
    let res = sqlx::query(
        "update console_user set password_hash = ?, must_change_password = 0 where guid = ?",
    )
    .bind(hash)
    .bind(guid)
    .execute(&mut tx)
    .await?;
    if res.rows_affected() == 0 {
        return Ok(false);
    }
    super::sessions::delete_other_sessions(&mut tx, guid, keep_session_hash).await?;
    tx.commit().await?;
    Ok(true)
}

fn generate_password() -> String {
    let mut rng = OsRng;
    (0..GENERATED_PASSWORD_LEN)
        .filter_map(|_| {
            PASSWORD_ALPHABET
                .get(rng.gen_range(0..PASSWORD_ALPHABET.len()))
                .map(|&b| b as char)
        })
        .collect()
}

/// Creates a console user in the given group (the `Default` group when `None`).
/// Returns the new guid.
pub(crate) async fn create_user(
    pool: &SqlitePool,
    name: &str,
    password_hash: &str,
    is_admin: bool,
    group_guid: Option<&[u8]>,
) -> Result<Vec<u8>, sqlx::Error> {
    insert_user(
        pool,
        &NewUser {
            name,
            password_hash,
            is_admin,
            group_guid,
            email: "",
            note: "",
        },
    )
    .await
}

/// A user to create; `group_guid == None` means the `Default` group.
pub(crate) struct NewUser<'a> {
    pub name: &'a str,
    pub password_hash: &'a str,
    pub is_admin: bool,
    pub group_guid: Option<&'a [u8]>,
    pub email: &'a str,
    pub note: &'a str,
}

async fn insert_user(pool: &SqlitePool, u: &NewUser<'_>) -> Result<Vec<u8>, sqlx::Error> {
    let guid = uuid::Uuid::new_v4().as_bytes().to_vec();
    sqlx::query(
        "insert into console_user(guid, name, password_hash, is_admin, group_guid, email, note)
         values(?1, ?2, ?3, ?4,
                coalesce(?5, (select guid from console_user_group where name = 'Default')),
                ?6, ?7)",
    )
    .bind(&guid)
    .bind(u.name)
    .bind(u.password_hash)
    .bind(u.is_admin)
    .bind(u.group_guid)
    .bind(u.email)
    .bind(u.note)
    .execute(pool)
    .await?;
    Ok(guid)
}

/// Like `create_user`, with profile fields; a taken name (case-insensitive) is
/// reported as `UserWriteError::Duplicate`.
pub(crate) async fn create_user_with_profile(
    pool: &SqlitePool,
    u: &NewUser<'_>,
) -> Result<Vec<u8>, UserWriteError> {
    insert_user(pool, u).await.map_err(|e| {
        if is_unique_violation(&e) {
            UserWriteError::Duplicate
        } else {
            UserWriteError::Db(e)
        }
    })
}

#[derive(Debug)]
pub(crate) enum UserWriteError {
    Duplicate,
    NotFound,
    /// The change would leave no enabled administrator.
    LastAdmin,
    /// Only disabled users may be deleted.
    NotDisabled,
    Db(sqlx::Error),
}

impl From<sqlx::Error> for UserWriteError {
    fn from(e: sqlx::Error) -> Self {
        UserWriteError::Db(e)
    }
}

fn is_unique_violation(e: &sqlx::Error) -> bool {
    match e {
        sqlx::Error::Database(d) => d.message().contains("UNIQUE constraint failed"),
        _ => false,
    }
}

/// Row-level guard for updates that take admin power away from `?1`: the update
/// matches only if that row is not an enabled admin, or another enabled admin exists.
/// It is part of the same statement, so it cannot race with a concurrent change.
const KEEPS_AN_ADMIN: &str = "(is_admin = 0 or status != 1 or exists(
    select 1 from console_user o where o.guid != ?1 and o.is_admin = 1 and o.status = 1))";

/// One row of the user list.
#[derive(Debug, Clone)]
pub(crate) struct UserListItem {
    pub user: User,
    /// Empty when the user has no (existing) group.
    pub group_name: String,
    /// `YYYY-MM-DDTHH:MM:SS`.
    pub created_at: String,
}

/// Filters of `list`, combined with AND. `*_like` are bound as LIKE patterns exactly
/// as sent; `guids == Some(..)` restricts the list to that set.
#[derive(Debug, Default)]
pub(crate) struct UserFilter<'a> {
    pub name_like: Option<&'a str>,
    pub group_name_like: Option<&'a str>,
    pub status: Option<i64>,
    pub guids: Option<&'a [Vec<u8>]>,
}

/// Returns `(total, page)` ordered by name.
pub(crate) async fn list(
    pool: &SqlitePool,
    f: &UserFilter<'_>,
    current: i64,
    page_size: i64,
) -> Result<(i64, Vec<UserListItem>), sqlx::Error> {
    let mut conds: Vec<String> = Vec::new();
    if f.name_like.is_some() {
        conds.push("u.name like ?".to_owned());
    }
    if f.group_name_like.is_some() {
        conds.push("g.name like ?".to_owned());
    }
    if f.status.is_some() {
        conds.push("u.status = ?".to_owned());
    }
    if let Some(guids) = f.guids {
        if guids.is_empty() {
            return Ok((0, Vec::new()));
        }
        conds.push(format!("u.guid in ({})", vec!["?"; guids.len()].join(", ")));
    }
    let where_sql = if conds.is_empty() {
        String::new()
    } else {
        format!("where {}", conds.join(" and "))
    };
    let from = "from console_user u left join console_user_group g on g.guid = u.group_guid";

    // Values are always bound, never spliced into the SQL.
    macro_rules! bind_filters {
        ($q:expr) => {{
            let mut q = $q;
            if let Some(v) = f.name_like {
                q = q.bind(v);
            }
            if let Some(v) = f.group_name_like {
                q = q.bind(v);
            }
            if let Some(v) = f.status {
                q = q.bind(v);
            }
            if let Some(guids) = f.guids {
                for g in guids {
                    q = q.bind(g.as_slice());
                }
            }
            q
        }};
    }

    let count_sql = format!("select count(*) {from} {where_sql}");
    let (total,): (i64,) = bind_filters!(sqlx::query_as(&count_sql))
        .fetch_one(pool)
        .await?;
    let select_sql = format!(
        "select u.guid as guid, u.name as name, u.display_name as display_name,
                u.email as email, u.note as note, u.is_admin as is_admin, u.status as status,
                u.must_change_password as must_change_password,
                coalesce(g.name, '') as group_name,
                strftime('%Y-%m-%dT%H:%M:%S', u.created_at) as created_at
         {from} {where_sql} order by u.name limit ? offset ?"
    );
    let offset = current.saturating_sub(1).max(0).saturating_mul(page_size);
    let rows = bind_filters!(sqlx::query(&select_sql))
        .bind(page_size)
        .bind(offset)
        .fetch_all(pool)
        .await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in &rows {
        out.push(UserListItem {
            user: row_to_user(row)?,
            group_name: row.try_get("group_name")?,
            created_at: row
                .try_get::<Option<String>, _>("created_at")?
                .unwrap_or_default(),
        });
    }
    Ok((total, out))
}

/// Optional changes of `apply_patch`; `None` leaves the field unchanged.
/// `password_hash` is an already computed bcrypt hash.
#[derive(Debug, Default)]
pub(crate) struct UserPatch<'a> {
    pub display_name: Option<&'a str>,
    pub email: Option<&'a str>,
    pub note: Option<&'a str>,
    pub group_guid: Option<&'a [u8]>,
    pub is_admin: Option<bool>,
    pub password_hash: Option<&'a str>,
}

async fn user_status(conn: &mut SqliteConnection, guid: &[u8]) -> Result<Option<i64>, sqlx::Error> {
    let row: Option<(i64,)> = sqlx::query_as("select status from console_user where guid = ?")
        .bind(guid)
        .fetch_optional(conn)
        .await?;
    Ok(row.map(|r| r.0))
}

/// Applies `p` in one transaction. A new password also revokes every session of the
/// user. Removing admin rights is refused with `LastAdmin` when no other enabled admin
/// would remain; nothing is changed then.
pub(crate) async fn apply_patch(
    pool: &SqlitePool,
    guid: &[u8],
    p: &UserPatch<'_>,
) -> Result<(), UserWriteError> {
    let mut tx = pool.begin().await?;
    if user_status(&mut tx, guid).await?.is_none() {
        return Err(UserWriteError::NotFound);
    }
    sqlx::query(
        "update console_user set display_name = coalesce(?, display_name),
           email = coalesce(?, email), note = coalesce(?, note),
           group_guid = coalesce(?, group_guid)
         where guid = ?",
    )
    .bind(p.display_name)
    .bind(p.email)
    .bind(p.note)
    .bind(p.group_guid)
    .bind(guid)
    .execute(&mut tx)
    .await?;
    match p.is_admin {
        Some(true) => {
            sqlx::query("update console_user set is_admin = 1 where guid = ?")
                .bind(guid)
                .execute(&mut tx)
                .await?;
        }
        Some(false) => {
            let sql = format!(
                "update console_user set is_admin = 0 where guid = ?1 and {KEEPS_AN_ADMIN}"
            );
            let res = sqlx::query(&sql).bind(guid).execute(&mut tx).await?;
            if res.rows_affected() == 0 {
                return Err(UserWriteError::LastAdmin);
            }
        }
        None => {}
    }
    if let Some(hash) = p.password_hash {
        sqlx::query("update console_user set password_hash = ? where guid = ?")
            .bind(hash)
            .bind(guid)
            .execute(&mut tx)
            .await?;
        sessions::delete_user_sessions(&mut tx, guid).await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Enables or disables a user. Disabling revokes all of the user's sessions in the same
/// transaction, and is refused with `LastAdmin` for the last enabled admin.
pub(crate) async fn set_status(
    pool: &SqlitePool,
    guid: &[u8],
    enabled: bool,
) -> Result<(), UserWriteError> {
    let mut tx = pool.begin().await?;
    if user_status(&mut tx, guid).await?.is_none() {
        return Err(UserWriteError::NotFound);
    }
    if enabled {
        sqlx::query("update console_user set status = 1 where guid = ?")
            .bind(guid)
            .execute(&mut tx)
            .await?;
    } else {
        let sql =
            format!("update console_user set status = 0 where guid = ?1 and {KEEPS_AN_ADMIN}");
        let res = sqlx::query(&sql).bind(guid).execute(&mut tx).await?;
        if res.rows_affected() == 0 {
            return Err(UserWriteError::LastAdmin);
        }
        sessions::delete_user_sessions(&mut tx, guid).await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Deletes a disabled user in one transaction: unassigns its devices (`peer.user`),
/// and removes its sessions and the access rules that target it.
pub(crate) async fn delete_user(pool: &SqlitePool, guid: &[u8]) -> Result<(), UserWriteError> {
    let mut tx = pool.begin().await?;
    match user_status(&mut tx, guid).await? {
        None => return Err(UserWriteError::NotFound),
        Some(0) => {}
        Some(_) => return Err(UserWriteError::NotDisabled),
    }
    sqlx::query("update peer set user = null where user = ?")
        .bind(guid)
        .execute(&mut tx)
        .await?;
    sessions::delete_user_sessions(&mut tx, guid).await?;
    sqlx::query("delete from console_access_rule where target_type = 2 and target_guid = ?")
        .bind(guid)
        .execute(&mut tx)
        .await?;
    sqlx::query("delete from console_user where guid = ?")
        .bind(guid)
        .execute(&mut tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

/// Creates the `admin` user when no console user exists. Returns the generated password.
pub(crate) async fn ensure_initial_admin(pool: &SqlitePool) -> ResultType<Option<String>> {
    let password = generate_password();
    let hash = hash_password(&password)?;
    let guid = uuid::Uuid::new_v4().as_bytes().to_vec();
    // One statement, so concurrent starters cannot both create an admin.
    let res = sqlx::query(
        "insert into console_user(guid, name, password_hash, is_admin, status, must_change_password, group_guid)
         select ?, 'admin', ?, 1, 1, 1, (select guid from console_user_group where name = 'Default')
         where not exists (select 1 from console_user)",
    )
    .bind(guid)
    .bind(hash)
    .execute(pool)
    .await?;
    Ok((res.rows_affected() > 0).then_some(password))
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

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn initial_admin_insert_is_atomic() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("db.sqlite3").to_string_lossy().to_string();
        let pool = crate::console::open_pool(&path).await.unwrap();
        crate::console::db::migrations::migrate(&pool, &path)
            .await
            .unwrap();
        let tasks: Vec<_> = (0..6)
            .map(|_| {
                let pool = pool.clone();
                tokio::spawn(async move { ensure_initial_admin(&pool).await })
            })
            .collect();
        let mut created = 0;
        for t in tasks {
            if t.await.unwrap().unwrap().is_some() {
                created += 1;
            }
        }
        assert_eq!(created, 1);
        let (count,): (i64,) = sqlx::query_as("select count(*) from console_user")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 1);
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
