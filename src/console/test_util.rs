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

pub(crate) async fn test_app() -> Router {
    router(ConsoleState {
        db: test_pool().await,
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
