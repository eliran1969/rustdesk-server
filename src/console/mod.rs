pub(crate) mod api;
pub(crate) mod auth;
pub(crate) mod db;
pub(crate) mod error;
pub(crate) mod state;
#[cfg(test)]
pub(crate) mod test_util;

use crate::{common::get_arg_or, peer::PeerMap};
use axum::{
    routing::{get, post},
    Router,
};
use hbb_common::{log, tokio, ResultType};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use state::ConsoleState;
use std::{net::SocketAddr, str::FromStr, sync::Arc, time::Duration};

const DEFAULT_PORT: u16 = 21114;

pub(crate) fn router(state: ConsoleState) -> Router {
    Router::new()
        .route("/api/login-options", get(api::client::login_options))
        .route("/api/sysinfo", post(api::client::sysinfo))
        .route("/api/sysinfo_ver", post(api::client::sysinfo_ver))
        .route("/api/heartbeat", post(api::client::heartbeat))
        .route("/api/login", post(api::account::login))
        .route("/api/currentUser", post(api::account::current_user))
        .route("/api/logout", post(api::account::logout))
        .route("/api/devices", get(api::devices::list))
        .route("/api/console/me", get(api::account::console_me))
        .route(
            "/api/console/change-password",
            post(api::account::change_password),
        )
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

/// CLI entry (rustdesk-utils): sets a console user's password. Returns whether the user exists.
pub async fn cli_set_password(db_url: &str, name: &str, password: &str) -> ResultType<bool> {
    let pool = open_pool(db_url).await?;
    db::migrations::migrate(&pool, db_url).await?;
    db::users::set_password(&pool, name, password, false).await
}

pub(crate) fn bind(addr: SocketAddr) -> std::io::Result<std::net::TcpListener> {
    std::net::TcpListener::bind(addr)
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
        if let Err(e) = db::migrations::migrate(&db, &db_url).await {
            log::error!("console: database migration failed: {}", e);
            return;
        }
        match db::users::ensure_initial_admin(&db).await {
            Ok(Some(pw)) => log::warn!(
                "Console initial admin password: {pw} (change it on first login)"
            ),
            Ok(None) => {}
            Err(e) => log::error!("console: failed to create initial admin: {}", e),
        }
        let addr = SocketAddr::from(([0, 0, 0, 0], port));
        let listener = match bind(addr) {
            Ok(l) => l,
            Err(e) => {
                log::error!("console: failed to bind {}: {}", addr, e);
                return;
            }
        };
        let server = match axum::Server::from_tcp(listener) {
            Ok(builder) => builder,
            Err(e) => {
                log::error!("console: failed to start server on {}: {}", addr, e);
                return;
            }
        };
        log::info!("web console listening on {}", addr);
        let app = router(ConsoleState {
            db,
            pm,
            limiter: Arc::new(auth::LoginLimiter::new()),
        });
        if let Err(e) = server
            .serve(app.into_make_service_with_connect_info::<SocketAddr>())
            .await
        {
            log::error!("console: server stopped: {}", e);
        }
    });
}
