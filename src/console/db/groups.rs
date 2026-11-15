// Consumed by the group management API in later tasks.
#![allow(dead_code)]

use sqlx::{sqlite::SqliteRow, Row, SqlitePool};

use super::migrations::DEFAULT_GROUP_NAME;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Group {
    pub guid: Vec<u8>,
    pub name: String,
    pub note: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GroupKind {
    User,
    Device,
}

impl GroupKind {
    fn table(self) -> &'static str {
        match self {
            GroupKind::User => "console_user_group",
            GroupKind::Device => "console_device_group",
        }
    }
}

#[derive(Debug)]
pub(crate) enum GroupError {
    DuplicateName,
    NotFound,
    Db(sqlx::Error),
}

impl From<sqlx::Error> for GroupError {
    fn from(e: sqlx::Error) -> Self {
        GroupError::Db(e)
    }
}

impl std::fmt::Display for GroupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GroupError::DuplicateName => write!(f, "group name already exists"),
            GroupError::NotFound => write!(f, "group not found"),
            GroupError::Db(e) => write!(f, "database error: {}", e),
        }
    }
}

fn map_write_err(e: sqlx::Error) -> GroupError {
    let dup = match &e {
        sqlx::Error::Database(d) => d.message().contains("UNIQUE constraint failed"),
        _ => false,
    };
    if dup {
        GroupError::DuplicateName
    } else {
        GroupError::Db(e)
    }
}

fn row_to_group(row: &SqliteRow) -> Result<Group, sqlx::Error> {
    Ok(Group {
        guid: row.try_get("guid")?,
        name: row.try_get("name")?,
        note: row.try_get("note")?,
    })
}

/// Returns `(total, page)`; `name_like` is bound as a LIKE pattern exactly as sent.
pub(crate) async fn list(
    pool: &SqlitePool,
    kind: GroupKind,
    name_like: Option<&str>,
    current: i64,
    page_size: i64,
) -> Result<(i64, Vec<Group>), GroupError> {
    let t = kind.table();
    let total: (i64,) = sqlx::query_as(&format!(
        "select count(*) from {t} where (?1 is null or name like ?1)"
    ))
    .bind(name_like)
    .fetch_one(pool)
    .await?;
    let offset = current.saturating_sub(1).max(0).saturating_mul(page_size);
    let rows = sqlx::query(&format!(
        "select guid, name, note from {t} where (?1 is null or name like ?1)
         order by name limit ?2 offset ?3"
    ))
    .bind(name_like)
    .bind(page_size)
    .bind(offset)
    .fetch_all(pool)
    .await?;
    let groups = rows
        .iter()
        .map(row_to_group)
        .collect::<Result<Vec<_>, _>>()?;
    Ok((total.0, groups))
}

pub(crate) async fn find_by_name(
    pool: &SqlitePool,
    kind: GroupKind,
    name: &str,
) -> Result<Option<Group>, GroupError> {
    let row = sqlx::query(&format!(
        "select guid, name, note from {} where name = ?",
        kind.table()
    ))
    .bind(name)
    .fetch_optional(pool)
    .await?;
    Ok(row.as_ref().map(row_to_group).transpose()?)
}

pub(crate) async fn find_by_guid(
    pool: &SqlitePool,
    kind: GroupKind,
    guid: &[u8],
) -> Result<Option<Group>, GroupError> {
    let row = sqlx::query(&format!(
        "select guid, name, note from {} where guid = ?",
        kind.table()
    ))
    .bind(guid)
    .fetch_optional(pool)
    .await?;
    Ok(row.as_ref().map(row_to_group).transpose()?)
}

pub(crate) async fn create(
    pool: &SqlitePool,
    kind: GroupKind,
    name: &str,
    note: &str,
) -> Result<Vec<u8>, GroupError> {
    let guid = uuid::Uuid::new_v4().as_bytes().to_vec();
    sqlx::query(&format!(
        "insert into {}(guid, name, note) values(?, ?, ?)",
        kind.table()
    ))
    .bind(&guid)
    .bind(name)
    .bind(note)
    .execute(pool)
    .await
    .map_err(map_write_err)?;
    Ok(guid)
}

pub(crate) async fn update(
    pool: &SqlitePool,
    kind: GroupKind,
    guid: &[u8],
    name: Option<&str>,
    note: Option<&str>,
) -> Result<(), GroupError> {
    let res = sqlx::query(&format!(
        "update {} set name = coalesce(?, name), note = coalesce(?, note) where guid = ?",
        kind.table()
    ))
    .bind(name)
    .bind(note)
    .bind(guid)
    .execute(pool)
    .await
    .map_err(map_write_err)?;
    if res.rows_affected() == 0 {
        return Err(GroupError::NotFound);
    }
    Ok(())
}

pub(crate) async fn delete(
    pool: &SqlitePool,
    kind: GroupKind,
    guid: &[u8],
) -> Result<(), GroupError> {
    let res = sqlx::query(&format!("delete from {} where guid = ?", kind.table()))
        .bind(guid)
        .execute(pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(GroupError::NotFound);
    }
    Ok(())
}

/// The built-in `Default` user group (created by the v2 migration).
pub(crate) async fn default_user_group(pool: &SqlitePool) -> Result<Group, GroupError> {
    find_by_name(pool, GroupKind::User, DEFAULT_GROUP_NAME)
        .await?
        .ok_or(GroupError::NotFound)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::console::{db::migrations::migrate, test_util::test_pool};

    #[tokio::test]
    async fn group_names_unique_case_insensitive() {
        let pool = test_pool().await;
        migrate(&pool, ":memory:").await.unwrap();
        create(&pool, GroupKind::User, "Sales", "").await.unwrap();
        assert!(matches!(
            create(&pool, GroupKind::User, "sales", "").await,
            Err(GroupError::DuplicateName)
        ));
        // Separate namespaces per kind.
        create(&pool, GroupKind::Device, "sales", "").await.unwrap();
        let (total, rows) = list(&pool, GroupKind::User, Some("s%"), 1, 10)
            .await
            .unwrap();
        assert_eq!(total, 1);
        assert_eq!(rows.first().map(|g| g.name.as_str()), Some("Sales"));
        assert_eq!(default_user_group(&pool).await.unwrap().name, "Default");
    }
}
