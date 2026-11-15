//! Web console. Release builds use `panic = 'abort'` (upstream Cargo.toml), so a panic
//! anywhere in this module would kill hbbs: console code must be panic-free, which the
//! lints below enforce. Tests may still unwrap.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing
    )
)]

pub(crate) mod api;
pub(crate) mod auth;
pub(crate) mod db;
pub(crate) mod error;
pub(crate) mod state;
pub(crate) mod web;
#[cfg(test)]
pub(crate) mod test_util;

use crate::{common::get_arg_or, peer::PeerMap};
use axum::{
    error_handling::HandleErrorLayer,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{any, get, patch, post, put},
    BoxError, Json, Router,
};
use hbb_common::{log, tokio, ResultType};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use state::ConsoleState;
use std::{net::SocketAddr, str::FromStr, sync::Arc, time::Duration};

const DEFAULT_PORT: u16 = 21114;
/// Time a client gets to send the complete request headers (slowloris guard).
const HEADER_READ_TIMEOUT: Duration = Duration::from_secs(10);
/// Upper bound for running one admitted request. Requests over the concurrency limit
/// are not queued: they are rejected at once (load shedding), so they never wait here.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
/// Requests handled at the same time across the whole console; more get 503 "Server busy".
const MAX_CONCURRENT_REQUESTS: usize = 256;

async fn handle_layer_error(err: BoxError) -> Response {
    let (status, msg) = if err.is::<tower::timeout::error::Elapsed>() {
        (StatusCode::SERVICE_UNAVAILABLE, "Request timed out")
    } else if err.is::<tower::load_shed::error::Overloaded>() {
        (StatusCode::SERVICE_UNAVAILABLE, "Server busy")
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, "Service unavailable")
    };
    (status, Json(serde_json::json!({ "error": msg }))).into_response()
}

pub(crate) fn router(state: ConsoleState) -> Router {
    let app = Router::new()
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
        .route("/api/users", get(api::users::list).post(api::users::create))
        .route("/api/users/force-logout", post(api::users::force_logout))
        .route("/api/users/invite", post(api::users::not_supported))
        .route("/api/users/tfa/totp/enforce", put(api::users::not_supported))
        .route(
            "/api/users/disable_login_verification",
            put(api::users::not_supported),
        )
        .route("/api/users/tfa/reset", put(api::users::not_supported))
        .route(
            "/api/users/:guid",
            patch(api::users::update).delete(api::users::delete),
        )
        .route("/api/users/:guid/enable", post(api::users::enable))
        .route("/api/users/:guid/disable", post(api::users::disable))
        .route(
            "/api/user-groups",
            get(api::user_groups::list).post(api::user_groups::create),
        )
        .route(
            "/api/user-groups/:guid",
            post(api::user_groups::add_users)
                .patch(api::user_groups::update)
                .delete(api::user_groups::delete),
        )
        .fallback(any(web::serve))
        .layer(axum::Extension(state));
    with_limits(app, REQUEST_TIMEOUT, MAX_CONCURRENT_REQUESTS)
}

fn with_limits(app: Router, timeout: Duration, max_concurrent: usize) -> Router {
    app.layer(
        tower::ServiceBuilder::new()
            .layer(HandleErrorLayer::new(handle_layer_error))
            .timeout(timeout)
            .load_shed()
            .layer(tower::limit::GlobalConcurrencyLimitLayer::new(max_concurrent)),
    )
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
    // Never create an empty database by accident (wrong DB_URL / wrong directory).
    let file = db_url.strip_prefix("sqlite://").or_else(|| db_url.strip_prefix("sqlite:")).unwrap_or(db_url);
    if !std::path::Path::new(file).exists() {
        return Err(hbb_common::anyhow::anyhow!("Database not found"));
    }
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
            Ok(builder) => builder.http1_header_read_timeout(HEADER_READ_TIMEOUT),
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;

    async fn slow() -> &'static str {
        tokio::time::sleep(Duration::from_millis(300)).await;
        "ok"
    }

    async fn call(app: &Router) -> (StatusCode, String) {
        let req = Request::builder().uri("/slow").body(Body::empty()).unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        let status = res.status();
        (status, test_util::body_string(res).await)
    }

    #[tokio::test]
    async fn handler_timeout_returns_503() {
        let app = with_limits(
            Router::new().route("/slow", get(slow)),
            Duration::from_millis(50),
            8,
        );
        let (status, body) = call(&app).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(body, r#"{"error":"Request timed out"}"#);
    }

    #[tokio::test]
    async fn over_limit_requests_are_shed_immediately() {
        let app = with_limits(
            Router::new().route("/slow", get(slow)),
            Duration::from_secs(5),
            1,
        );
        let (a, b) = tokio::join!(call(&app), call(&app));
        let mut got = [a, b];
        got.sort_by_key(|(s, _)| s.as_u16());
        assert_eq!(got[0], (StatusCode::OK, "ok".to_owned()));
        assert_eq!(
            got[1],
            (
                StatusCode::SERVICE_UNAVAILABLE,
                r#"{"error":"Server busy"}"#.to_owned()
            )
        );
    }

    #[tokio::test]
    async fn cli_set_password_refuses_missing_db() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing.sqlite3");
        let err = cli_set_password(&path.to_string_lossy(), "admin", "password-1")
            .await
            .unwrap_err();
        assert_eq!(err.to_string(), "Database not found");
        assert!(!path.exists());
    }
}
