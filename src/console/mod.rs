pub(crate) mod api;
pub(crate) mod error;
pub(crate) mod state;
#[cfg(test)]
pub(crate) mod test_util;

use crate::{common::get_arg_or, peer::PeerMap};
use axum::{routing::get, Router};
use hbb_common::{log, tokio};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use state::ConsoleState;
use std::{net::SocketAddr, str::FromStr, time::Duration};

const DEFAULT_PORT: u16 = 21114;

pub(crate) fn router(state: ConsoleState) -> Router {
    Router::new()
        .route("/api/login-options", get(api::client::login_options))
        .layer(axum::Extension(state))
}

async fn open_pool(db_url: &str) -> Result<sqlx::SqlitePool, sqlx::Error> {
    let opts = SqliteConnectOptions::from_str(db_url)?
        .create_if_missing(true)
        .busy_timeout(Duration::from_secs(5))
        .journal_mode(SqliteJournalMode::Wal);
    SqlitePoolOptions::new()
        .max_connections(4)
        .connect_with(opts)
        .await
}

/// Starts the web console in the background. Never fails: problems are logged only.
pub(crate) async fn start(pm: PeerMap, db_url: String) {
    let port: u16 = get_arg_or("API_PORT", DEFAULT_PORT.to_string())
        .parse()
        .unwrap_or_else(|_| {
            log::warn!("invalid API_PORT, using {}", DEFAULT_PORT);
            DEFAULT_PORT
        });
    if port == 0 {
        log::info!("web console disabled (API_PORT=0)");
        return;
    }
    tokio::spawn(async move {
        let db = match open_pool(&db_url).await {
            Ok(db) => db,
            Err(e) => {
                log::error!("console: failed to open database {}: {}", db_url, e);
                return;
            }
        };
        let addr = SocketAddr::from(([0, 0, 0, 0], port));
        let server = match axum::Server::try_bind(&addr) {
            Ok(builder) => builder,
            Err(e) => {
                log::error!("console: failed to bind {}: {}", addr, e);
                return;
            }
        };
        log::info!("web console listening on {}", addr);
        let app = router(ConsoleState { db, pm });
        if let Err(e) = server
            .serve(app.into_make_service_with_connect_info::<SocketAddr>())
            .await
        {
            log::error!("console: server stopped: {}", e);
        }
    });
}
