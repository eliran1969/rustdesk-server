use super::{router, state::ConsoleState};
use crate::peer::PeerMap;
use axum::{body::HttpBody, response::Response, Router};
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};

pub(crate) async fn test_pm() -> PeerMap {
    let path = std::env::temp_dir().join(format!("hbbs_test_{}.sqlite3", uuid::Uuid::new_v4()));
    PeerMap::with_db_url(&path.to_string_lossy()).await.unwrap()
}

pub(crate) async fn test_pool() -> SqlitePool {
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap()
}

/// Creates the `peer` table exactly as hbbs does (src/database.rs), so console
/// tests mirror production.
pub(crate) async fn create_peer_table(pool: &SqlitePool) {
    sqlx::query(
        "
        create table if not exists peer (
            guid blob primary key not null,
            id varchar(100) not null,
            uuid blob not null,
            pk blob not null,
            created_at datetime not null default(current_timestamp),
            user blob,
            status tinyint,
            note varchar(300),
            info text not null
        ) without rowid;
        create unique index if not exists index_peer_id on peer (id);
        create index if not exists index_peer_user on peer (user);
        create index if not exists index_peer_created_at on peer (created_at);
        create index if not exists index_peer_status on peer (status);
        ",
    )
    .execute(pool)
    .await
    .unwrap();
}

#[allow(dead_code)]
pub(crate) async fn insert_peer(pool: &SqlitePool, id: &str, uuid: &[u8]) {
    let guid = uuid::Uuid::new_v4().as_bytes().to_vec();
    sqlx::query("insert into peer(guid, id, uuid, pk, info) values(?, ?, ?, x'', '{}')")
        .bind(guid)
        .bind(id)
        .bind(uuid)
        .execute(pool)
        .await
        .unwrap();
}

/// Inserts a console user with a real bcrypt hash; returns its guid.
#[allow(dead_code)]
pub(crate) async fn insert_user(
    pool: &SqlitePool,
    name: &str,
    password: &str,
    is_admin: bool,
) -> Vec<u8> {
    let guid = uuid::Uuid::new_v4().as_bytes().to_vec();
    let hash = super::db::users::hash_password(password).unwrap();
    sqlx::query(
        "insert into console_user(guid, name, password_hash, is_admin) values(?, ?, ?, ?)",
    )
    .bind(&guid)
    .bind(name)
    .bind(hash)
    .bind(is_admin)
    .execute(pool)
    .await
    .unwrap();
    guid
}

pub(crate) async fn test_app() -> Router {
    let db = test_pool().await;
    create_peer_table(&db).await;
    super::db::migrations::migrate(&db, ":memory:").await.unwrap();
    router(ConsoleState {
        db,
        pm: test_pm().await,
    })
}

pub(crate) async fn body_string(res: Response) -> String {
    let mut body = res.into_body();
    let mut out = Vec::new();
    while let Some(chunk) = body.data().await {
        out.extend_from_slice(&chunk.unwrap());
    }
    String::from_utf8(out).unwrap()
}
