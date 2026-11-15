use hbb_common::{anyhow::anyhow, log, ResultType};
use sqlx::SqlitePool;

const TARGET_VERSION: i64 = 2;

const SCHEMA_V1: &str = "
create table if not exists console_meta (
  key text primary key not null,
  value text
);
create table if not exists console_user (
  guid blob primary key not null,
  name varchar(100) not null unique collate nocase,
  display_name varchar(100) not null default '',
  email varchar(200) not null default '',
  note varchar(300) not null default '',
  password_hash text not null,
  is_admin tinyint not null default 0,
  status tinyint not null default 1,
  must_change_password tinyint not null default 0,
  created_at datetime not null default(current_timestamp)
);
create table if not exists console_session (
  token_hash blob primary key not null,
  user_guid blob not null,
  kind varchar(10) not null,
  peer_id varchar(100), peer_uuid varchar(100),
  created_at datetime not null default(current_timestamp),
  last_seen datetime not null default(current_timestamp)
);
create table if not exists console_peer_sysinfo (
  peer_guid blob primary key not null,
  sysinfo text not null,
  last_online datetime,
  updated_at datetime not null default(current_timestamp)
);
";

async fn current_version(pool: &SqlitePool) -> ResultType<i64> {
    let exists: Option<(String,)> =
        sqlx::query_as("select name from sqlite_master where type = 'table' and name = 'console_meta'")
            .fetch_optional(pool)
            .await?;
    if exists.is_none() {
        return Ok(0);
    }
    let row: Option<(String,)> =
        sqlx::query_as("select value from console_meta where key = 'schema_version'")
            .fetch_optional(pool)
            .await?;
    Ok(row.and_then(|r| r.0.parse().ok()).unwrap_or(0))
}

async fn backup(pool: &SqlitePool, db_path: &str) -> ResultType<()> {
    if db_path.is_empty() || db_path == ":memory:" {
        return Ok(());
    }
    let size = match std::fs::metadata(db_path) {
        Ok(m) => m.len(),
        Err(_) => return Ok(()),
    };
    if size == 0 {
        return Ok(());
    }
    // Flush the WAL into the main file so the copy is consistent.
    sqlx::query("pragma wal_checkpoint(TRUNCATE)")
        .execute(pool)
        .await?;
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| anyhow!("system clock error: {}", e))?
        .as_secs();
    let dest = format!("{}.bak-{}", db_path, ts);
    std::fs::copy(db_path, &dest).map_err(|e| anyhow!("backup to {} failed: {}", dest, e))?;
    log::info!("console: database backed up to {}", dest);
    Ok(())
}

const SCHEMA_V2: &str = "
create table if not exists console_user_group (
  guid blob primary key not null,
  name varchar(100) not null unique collate nocase,
  note varchar(300) not null default '',
  created_at datetime not null default(current_timestamp)
);
create table if not exists console_device_group (
  guid blob primary key not null,
  name varchar(100) not null unique collate nocase,
  note varchar(300) not null default '',
  created_at datetime not null default(current_timestamp)
);
create table if not exists console_access_rule (
  owner_kind varchar(12) not null,
  owner_guid blob not null,
  direction varchar(3) not null,
  target_type tinyint not null,
  target_guid blob not null,
  primary key (owner_kind, owner_guid, direction, target_type, target_guid)
);
create table if not exists console_peer_ext (
  peer_guid blob primary key not null,
  device_group_guid blob,
  device_name varchar(200),
  device_username varchar(200)
);
";

pub(crate) const DEFAULT_GROUP_NAME: &str = "Default";

async fn apply_v1(pool: &SqlitePool) -> ResultType<()> {
    let mut tx = pool.begin().await?;
    sqlx::query(SCHEMA_V1).execute(&mut tx).await?;
    set_version(&mut tx, 1).await?;
    tx.commit().await?;
    Ok(())
}

async fn apply_v2(pool: &SqlitePool) -> ResultType<()> {
    let mut tx = pool.begin().await?;
    sqlx::query(SCHEMA_V2).execute(&mut tx).await?;
    let cols: Vec<(String,)> = sqlx::query_as("select name from pragma_table_info('console_user')")
        .fetch_all(&mut tx)
        .await?;
    if !cols.iter().any(|c| c.0 == "group_guid") {
        sqlx::query("alter table console_user add column group_guid blob")
            .execute(&mut tx)
            .await?;
    }
    sqlx::query(
        "insert into console_user_group(guid, name) select ?, ?
         where not exists (select 1 from console_user_group where name = ?)",
    )
    .bind(uuid::Uuid::new_v4().as_bytes().to_vec())
    .bind(DEFAULT_GROUP_NAME)
    .bind(DEFAULT_GROUP_NAME)
    .execute(&mut tx)
    .await?;
    sqlx::query(
        "update console_user set group_guid =
           (select guid from console_user_group where name = ?)
         where group_guid is null",
    )
    .bind(DEFAULT_GROUP_NAME)
    .execute(&mut tx)
    .await?;
    set_version(&mut tx, 2).await?;
    tx.commit().await?;
    Ok(())
}

async fn set_version(tx: &mut sqlx::SqliteConnection, v: i64) -> ResultType<()> {
    sqlx::query("insert or replace into console_meta(key, value) values('schema_version', ?)")
        .bind(v.to_string())
        .execute(tx)
        .await?;
    Ok(())
}

/// Applies additive console migrations step by step (each in its own
/// transaction), backing up the database file once before the first step
/// whenever the schema version is about to be raised.
pub(crate) async fn migrate(pool: &SqlitePool, db_path: &str) -> ResultType<()> {
    let version = current_version(pool).await?;
    if version >= TARGET_VERSION {
        return Ok(());
    }
    backup(pool, db_path).await?;
    if version < 1 {
        apply_v1(pool).await?;
    }
    apply_v2(pool).await?;
    log::info!("console: schema migrated to version {}", TARGET_VERSION);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::console::open_pool;

    fn baks(dir: &std::path::Path) -> usize {
        std::fs::read_dir(dir)
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .contains(".bak-")
            })
            .count()
    }

    async fn version(pool: &SqlitePool) -> String {
        sqlx::query_as::<_, (String,)>("select value from console_meta where key='schema_version'")
            .fetch_one(pool)
            .await
            .unwrap()
            .0
    }

    #[tokio::test]
    async fn migrate_preserves_upstream_peers() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("db_v2.sqlite3");
        std::fs::copy(
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/upstream_db_v2.sqlite3"),
            &path,
        )
        .unwrap();
        let p = path.to_string_lossy().to_string();
        let pool = open_pool(&p).await.unwrap();
        let before: (i64,) = sqlx::query_as("select count(*) from peer")
            .fetch_one(&pool)
            .await
            .unwrap();
        migrate(&pool, &p).await.unwrap();
        let after: (i64,) = sqlx::query_as("select count(*) from peer")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(before, after);
        for t in ["console_meta", "console_user", "console_session", "console_peer_sysinfo"] {
            sqlx::query(&format!("select count(*) from {}", t))
                .fetch_one(&pool)
                .await
                .unwrap();
        }
        assert_eq!(version(&pool).await, "2");
        assert_eq!(baks(dir.path()), 1);
    }

    #[tokio::test]
    async fn migrate_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("db_v2.sqlite3");
        std::fs::copy(
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/upstream_db_v2.sqlite3"),
            &path,
        )
        .unwrap();
        let p = path.to_string_lossy().to_string();
        let pool = open_pool(&p).await.unwrap();
        migrate(&pool, &p).await.unwrap();
        migrate(&pool, &p).await.unwrap();
        assert_eq!(baks(dir.path()), 1);
        assert_eq!(version(&pool).await, "2");
    }

    #[tokio::test]
    async fn migrate_fresh_db() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fresh.sqlite3");
        let p = path.to_string_lossy().to_string();
        let pool = open_pool(&p).await.unwrap();
        migrate(&pool, &p).await.unwrap();
        assert_eq!(version(&pool).await, "2");
    }

    async fn v1_db(dir: &std::path::Path) -> (SqlitePool, String) {
        let path = dir.join("v1.sqlite3");
        let p = path.to_string_lossy().to_string();
        let pool = open_pool(&p).await.unwrap();
        sqlx::query(SCHEMA_V1).execute(&pool).await.unwrap();
        sqlx::query("insert into console_meta(key,value) values('schema_version','1')")
            .execute(&pool)
            .await
            .unwrap();
        for n in ["alice", "bob"] {
            sqlx::query("insert into console_user(guid,name,password_hash) values(?,?,'x')")
                .bind(uuid::Uuid::new_v4().as_bytes().to_vec())
                .bind(n)
                .execute(&pool)
                .await
                .unwrap();
        }
        (pool, p)
    }

    #[tokio::test]
    async fn migrate_v2_from_v1_keeps_users_and_assigns_default_group() {
        let dir = tempfile::tempdir().unwrap();
        let (pool, p) = v1_db(dir.path()).await;
        migrate(&pool, &p).await.unwrap();
        assert_eq!(version(&pool).await, "2");
        assert_eq!(baks(dir.path()), 1);
        let rows: Vec<(String, String)> = sqlx::query_as(
            "select u.name, g.name from console_user u
             join console_user_group g on g.guid = u.group_guid order by u.name",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(
            rows,
            vec![
                ("alice".to_string(), "Default".to_string()),
                ("bob".to_string(), "Default".to_string())
            ]
        );
    }

    #[tokio::test]
    async fn migrate_v2_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let (pool, p) = v1_db(dir.path()).await;
        migrate(&pool, &p).await.unwrap();
        migrate(&pool, &p).await.unwrap();
        // Re-running the v2 step directly (e.g. after a crash) must be harmless too.
        apply_v2(&pool).await.unwrap();
        assert_eq!(baks(dir.path()), 1);
        assert_eq!(version(&pool).await, "2");
        let n: (i64,) = sqlx::query_as("select count(*) from console_user_group")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(n.0, 1);
    }
}
