use super::JsonBody;
use crate::console::{
    auth::{self, AuthUser},
    db::{
        sessions::{self, SessionKind},
        users::{self, User, MIN_PASSWORD_LEN},
    },
    error::ApiError,
    state::ConsoleState,
};
use axum::{
    extract::{ConnectInfo, Extension},
    http::HeaderMap,
    Json,
};
use hbb_common::log;
use serde_derive::Deserialize;
use serde_json::{json, Value};
use std::net::SocketAddr;

const LOGIN_TYPE_ACCOUNT: &str = "account";

/// Body of `POST /api/login`, as sent by the client (`LoginRequest` in hbbs.dart).
/// Unknown fields (`deviceInfo`, `autoLogin`, ...) are ignored.
#[derive(Deserialize)]
pub(crate) struct LoginRequest {
    username: Option<String>,
    password: Option<String>,
    id: Option<String>,
    uuid: Option<String>,
    #[serde(rename = "type")]
    login_type: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct ChangePasswordRequest {
    old_password: String,
    new_password: String,
}

/// The `UserPayload` object the client understands.
fn user_json(u: &User) -> Value {
    json!({
        "name": u.name,
        "display_name": u.display_name,
        "email": u.email,
        "note": u.note,
        "status": u.status,
        "is_admin": u.is_admin,
    })
}

fn non_empty(s: &Option<String>) -> Option<&str> {
    s.as_deref().filter(|s| !s.is_empty())
}

pub(crate) async fn login(
    Extension(state): Extension<ConsoleState>,
    connect: Option<ConnectInfo<SocketAddr>>,
    headers: HeaderMap,
    JsonBody(req): JsonBody<LoginRequest>,
) -> Result<Json<Value>, ApiError> {
    match req.login_type.as_deref() {
        None | Some(LOGIN_TYPE_ACCOUNT) => {}
        Some(_) => return Err(ApiError::BadRequest("Unsupported login type".to_owned())),
    }
    let ip = auth::client_ip(&headers, connect.map(|c| c.0));
    if !state.limiter.check(ip) {
        return Err(ApiError::TooManyRequests);
    }
    let _slot = state.limiter.verify_slot().await?;
    // Re-check: failures may have been recorded while this request waited for a slot.
    if !state.limiter.check(ip) {
        return Err(ApiError::TooManyRequests);
    }
    let username = req.username.unwrap_or_default();
    let password = req.password.unwrap_or_default();
    let found = users::find_by_name(&state.db, &username).await?;
    let (user, hash) = match found {
        Some((user, hash)) => (Some(user), Some(hash)),
        None => (None, None),
    };
    let verified = auth::verify_password_blocking(password, hash).await?;
    let user = match user {
        Some(user) if verified && user.status == 1 => user,
        _ => {
            state.limiter.fail(ip);
            return Err(ApiError::WrongCredentials);
        }
    };
    state.limiter.success(ip);

    let (kind, peer_id, peer_uuid) = match (non_empty(&req.id), non_empty(&req.uuid)) {
        (Some(id), Some(uuid)) => {
            assign_peer(&state, &user, id, uuid).await;
            (SessionKind::Client, Some(id), Some(uuid))
        }
        _ => (SessionKind::Web, None, None),
    };
    let token = sessions::create_session(&state.db, &user.guid, kind, peer_id, peer_uuid).await?;
    Ok(Json(json!({
        "type": "access_token",
        "access_token": token,
        "user": user_json(&user),
    })))
}

/// Assigns the device to `user` only when the client's base64 uuid matches the stored
/// `peer.uuid` bytes. Writes only the `user` column of `peer`. Failures are logged:
/// the login itself still succeeds.
async fn assign_peer(state: &ConsoleState, user: &User, id: &str, uuid_b64: &str) {
    let uuid = match base64::decode(uuid_b64) {
        Ok(uuid) => uuid,
        Err(_) => {
            log::warn!("console: login from {} with malformed uuid", id);
            return;
        }
    };
    let res = sqlx::query("update peer set user = ? where id = ? and uuid = ?")
        .bind(&user.guid)
        .bind(id)
        .bind(uuid)
        .execute(&state.db)
        .await;
    match res {
        Ok(r) if r.rows_affected() == 0 => {
            log::warn!("console: login from {}: no peer with matching uuid", id)
        }
        Ok(_) => {}
        Err(e) => log::error!("console: failed to assign peer {}: {}", id, e),
    }
}

pub(crate) async fn current_user(AuthUser(user, _): AuthUser) -> Json<Value> {
    Json(user_json(&user))
}

pub(crate) async fn logout(
    Extension(state): Extension<ConsoleState>,
    _auth: AuthUser,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    if let Some(token) = auth::bearer_token(&headers) {
        sessions::delete_session(&state.db, &sessions::hash_token(token)).await?;
    }
    Ok(Json(json!({})))
}

pub(crate) async fn console_me(AuthUser(user, _): AuthUser) -> Json<Value> {
    let mut body = user_json(&user);
    if let Some(obj) = body.as_object_mut() {
        obj.insert(
            "must_change_password".to_owned(),
            json!(user.must_change_password),
        );
    }
    Json(body)
}

pub(crate) async fn change_password(
    Extension(state): Extension<ConsoleState>,
    AuthUser(user, _): AuthUser,
    connect: Option<ConnectInfo<SocketAddr>>,
    headers: HeaderMap,
    JsonBody(req): JsonBody<ChangePasswordRequest>,
) -> Result<Json<Value>, ApiError> {
    let ip = auth::client_ip(&headers, connect.map(|c| c.0));
    if !state.limiter.check(ip) {
        return Err(ApiError::TooManyRequests);
    }
    let _slot = state.limiter.verify_slot().await?;
    if !state.limiter.check(ip) {
        return Err(ApiError::TooManyRequests);
    }
    let (_, hash) = users::find_by_name(&state.db, &user.name)
        .await?
        .ok_or(ApiError::Unauthorized)?;
    if !auth::verify_password_blocking(req.old_password, Some(hash)).await? {
        state.limiter.fail(ip);
        return Err(ApiError::BadRequest("Wrong old password".to_owned()));
    }
    if req.new_password.chars().count() < MIN_PASSWORD_LEN {
        return Err(ApiError::BadRequest(format!(
            "Password must be at least {} characters",
            MIN_PASSWORD_LEN
        )));
    }
    let new_hash = auth::hash_password_blocking(req.new_password).await?;
    if !users::update_password_hash(&state.db, &user.guid, &new_hash, false).await? {
        return Err(ApiError::Unauthorized);
    }
    let current = auth::bearer_token(&headers)
        .map(sessions::hash_token)
        .ok_or(ApiError::Unauthorized)?;
    sessions::delete_other_sessions(&state.db, &user.guid, &current).await?;
    Ok(Json(json!({})))
}

#[cfg(test)]
mod tests {
    use crate::console::test_util::{
        body_string, insert_peer, insert_user, login_token, test_app_with_db,
    };
    use axum::{
        body::Body,
        extract::ConnectInfo,
        http::{Request, StatusCode},
        Router,
    };
    use serde_json::{json, Value};
    use sqlx::SqlitePool;
    use std::net::SocketAddr;
    use tower::ServiceExt;

    const PW: &str = "correct-horse";

    fn req(method: &str, uri: &str, body: Option<Value>, token: Option<&str>) -> Request<Body> {
        let mut b = Request::builder().method(method).uri(uri);
        if let Some(t) = token {
            b = b.header("Authorization", format!("Bearer {t}"));
        }
        let body = match body {
            Some(v) => Body::from(v.to_string()),
            None => Body::empty(),
        };
        b.body(body).unwrap()
    }

    async fn send(app: &Router, req: Request<Body>) -> (StatusCode, Value) {
        let res = app.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let text = body_string(res).await;
        (
            status,
            serde_json::from_str(&text).unwrap_or(Value::String(text)),
        )
    }

    async fn login(app: &Router, body: Value) -> (StatusCode, Value) {
        send(app, req("POST", "/api/login", Some(body), None)).await
    }

    async fn peer_user(db: &SqlitePool, id: &str) -> Option<Vec<u8>> {
        let (user,): (Option<Vec<u8>>,) = sqlx::query_as("select user from peer where id = ?")
            .bind(id)
            .fetch_one(db)
            .await
            .unwrap();
        user
    }

    #[tokio::test]
    async fn login_success_returns_token_and_user() {
        let (app, db) = test_app_with_db().await;
        insert_user(&db, "alice", PW, true).await;
        let (status, body) = login(&app, json!({"username": "alice", "password": PW})).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["type"], "access_token");
        assert_eq!(body["access_token"].as_str().unwrap().len(), 64);
        assert_eq!(body["user"]["name"], "alice");
        assert_eq!(body["user"]["status"], 1);
        assert_eq!(body["user"]["is_admin"], true);
    }

    #[tokio::test]
    async fn login_wrong_password_and_unknown_user_same_error() {
        let (app, db) = test_app_with_db().await;
        insert_user(&db, "alice", PW, false).await;
        let wrong = login(&app, json!({"username": "alice", "password": "nope-nope"})).await;
        let unknown = login(&app, json!({"username": "bob", "password": PW})).await;
        assert_eq!(wrong.0, StatusCode::UNAUTHORIZED);
        assert_eq!(wrong, unknown);
        assert_eq!(wrong.1, json!({"error": "Wrong username or password"}));
    }

    #[tokio::test]
    async fn login_unsupported_type_rejected() {
        let (app, _db) = test_app_with_db().await;
        let (status, body) = login(
            &app,
            json!({"username": "a", "password": "b", "type": "sms_code"}),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body, json!({"error": "Unsupported login type"}));
    }

    #[tokio::test]
    async fn login_disabled_user_rejected() {
        let (app, db) = test_app_with_db().await;
        insert_user(&db, "alice", PW, false).await;
        sqlx::query("update console_user set status = 0 where name = 'alice'")
            .execute(&db)
            .await
            .unwrap();
        let (status, body) = login(&app, json!({"username": "alice", "password": PW})).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body, json!({"error": "Wrong username or password"}));
    }

    #[tokio::test]
    async fn login_rate_limited_after_10_failures() {
        let (app, db) = test_app_with_db().await;
        insert_user(&db, "alice", PW, false).await;
        let ip: SocketAddr = "10.1.2.3:5555".parse().unwrap();
        let attempt = |pw: &str| {
            let mut r = req(
                "POST",
                "/api/login",
                Some(json!({"username": "alice", "password": pw})),
                None,
            );
            r.extensions_mut().insert(ConnectInfo(ip));
            r
        };
        for _ in 0..10 {
            let (status, _) = send(&app, attempt("wrong-password")).await;
            assert_eq!(status, StatusCode::UNAUTHORIZED);
        }
        let (status, _) = send(&app, attempt(PW)).await;
        assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
        // Another IP is not affected.
        let mut other = attempt(PW);
        other
            .extensions_mut()
            .insert(ConnectInfo("10.9.9.9:1".parse::<SocketAddr>().unwrap()));
        assert_eq!(send(&app, other).await.0, StatusCode::OK);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn parallel_failures_cannot_overshoot_rate_limit() {
        let (app, db) = test_app_with_db().await;
        insert_user(&db, "alice", PW, false).await;
        let ip: SocketAddr = "10.4.4.4:1".parse().unwrap();
        let tasks: Vec<_> = (0..20)
            .map(|_| {
                let app = app.clone();
                let mut r = req(
                    "POST",
                    "/api/login",
                    Some(json!({"username": "alice", "password": "wrong-password"})),
                    None,
                );
                r.extensions_mut().insert(ConnectInfo(ip));
                tokio::spawn(async move { app.oneshot(r).await.unwrap().status() })
            })
            .collect();
        let mut unauthorized = 0;
        for t in tasks {
            match t.await.unwrap() {
                StatusCode::UNAUTHORIZED => unauthorized += 1,
                StatusCode::TOO_MANY_REQUESTS => {}
                other => panic!("unexpected status {other}"),
            }
        }
        // At most one extra attempt can slip in per additional verify slot.
        assert!((10..=11).contains(&unauthorized), "{unauthorized}");
    }

    #[tokio::test]
    async fn login_from_client_assigns_peer_only_with_matching_uuid() {
        let (app, db) = test_app_with_db().await;
        let guid = insert_user(&db, "alice", PW, false).await;
        insert_peer(&db, "123", b"u1").await;
        insert_peer(&db, "456", b"u2").await;

        let (status, _) = login(
            &app,
            json!({"username": "alice", "password": PW, "id": "456",
                   "uuid": base64::encode(b"other")}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(peer_user(&db, "456").await, None);

        let (status, _) = login(
            &app,
            json!({"username": "alice", "password": PW, "id": "123",
                   "uuid": base64::encode(b"u1"), "type": "account"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(peer_user(&db, "123").await, Some(guid));
        let (kind,): (String,) =
            sqlx::query_as("select kind from console_session where peer_id = '123'")
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(kind, "client");
    }

    #[tokio::test]
    async fn current_user_requires_valid_token() {
        let (app, db) = test_app_with_db().await;
        insert_user(&db, "alice", PW, false).await;
        let (status, _) = send(&app, req("POST", "/api/currentUser", None, None)).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        let bogus = "0".repeat(64);
        let (status, _) = send(&app, req("POST", "/api/currentUser", None, Some(&bogus))).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);

        let token = login_token(&app, "alice", PW).await;
        let (status, body) = send(
            &app,
            req(
                "POST",
                "/api/currentUser",
                Some(json!({"id": "1", "uuid": "x"})),
                Some(&token),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["name"], "alice");
        assert_eq!(body["status"], 1);

        // A user disabled after login loses access immediately.
        sqlx::query("update console_user set status = 0 where name = 'alice'")
            .execute(&db)
            .await
            .unwrap();
        let (status, _) = send(&app, req("POST", "/api/currentUser", None, Some(&token))).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn logout_invalidates_token() {
        let (app, db) = test_app_with_db().await;
        let guid = insert_user(&db, "alice", PW, false).await;
        insert_peer(&db, "123", b"u1").await;
        let (_, body) = login(
            &app,
            json!({"username": "alice", "password": PW, "id": "123",
                   "uuid": base64::encode(b"u1")}),
        )
        .await;
        let token = body["access_token"].as_str().unwrap().to_owned();
        let (status, body) = send(&app, req("POST", "/api/logout", None, Some(&token))).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, json!({}));
        let (status, _) = send(&app, req("POST", "/api/currentUser", None, Some(&token))).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        // The device assignment is kept.
        assert_eq!(peer_user(&db, "123").await, Some(guid));
    }

    #[tokio::test]
    async fn token_stored_hashed() {
        let (app, db) = test_app_with_db().await;
        insert_user(&db, "alice", PW, false).await;
        let token = login_token(&app, "alice", PW).await;
        let (total,): (i64,) = sqlx::query_as("select count(*) from console_session")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(total, 1);
        for plain in [token.as_bytes().to_vec(), hex_decode(&token)] {
            let (count,): (i64,) =
                sqlx::query_as("select count(*) from console_session where token_hash = ?")
                    .bind(plain)
                    .fetch_one(&db)
                    .await
                    .unwrap();
            assert_eq!(count, 0);
        }
    }

    fn hex_decode(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    #[tokio::test]
    async fn console_me_returns_must_change_password() {
        let (app, db) = test_app_with_db().await;
        insert_user(&db, "alice", PW, true).await;
        let token = login_token(&app, "alice", PW).await;
        let (status, body) = send(&app, req("GET", "/api/console/me", None, Some(&token))).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["name"], "alice");
        assert_eq!(body["is_admin"], true);
        assert_eq!(body["must_change_password"], false);
        let (status, _) = send(&app, req("GET", "/api/console/me", None, None)).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn must_change_password_blocks_other_routes_until_changed() {
        let (app, db) = test_app_with_db().await;
        insert_user(&db, "alice", PW, true).await;
        sqlx::query("update console_user set must_change_password = 1 where name = 'alice'")
            .execute(&db)
            .await
            .unwrap();
        let token = login_token(&app, "alice", PW).await;
        let (status, body) = send(&app, req("GET", "/api/devices", None, Some(&token))).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(body, json!({"error": "Password change required"}));
        // The exempt routes still work.
        let (status, _) = send(&app, req("GET", "/api/console/me", None, Some(&token))).await;
        assert_eq!(status, StatusCode::OK);
        let (status, _) = send(&app, req("POST", "/api/currentUser", None, Some(&token))).await;
        assert_eq!(status, StatusCode::OK);

        let (status, body) = send(
            &app,
            req(
                "POST",
                "/api/console/change-password",
                Some(json!({"old_password": PW, "new_password": "brand-new-pw"})),
                Some(&token),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let (status, body) = send(&app, req("GET", "/api/devices", None, Some(&token))).await;
        assert_eq!(status, StatusCode::OK, "{body}");
    }

    #[tokio::test]
    async fn change_password_clears_flag() {
        let (app, db) = test_app_with_db().await;
        insert_user(&db, "alice", PW, false).await;
        sqlx::query("update console_user set must_change_password = 1 where name = 'alice'")
            .execute(&db)
            .await
            .unwrap();
        let other = login_token(&app, "alice", PW).await;
        let token = login_token(&app, "alice", PW).await;
        let (_, me) = send(&app, req("GET", "/api/console/me", None, Some(&token))).await;
        assert_eq!(me["must_change_password"], true);

        let change = |old: &str, new: &str| {
            req(
                "POST",
                "/api/console/change-password",
                Some(json!({"old_password": old, "new_password": new})),
                Some(&token),
            )
        };
        let (status, body) = send(&app, change("wrong-old-pw", "brand-new-pw")).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(body["error"].is_string());
        let (status, _) = send(&app, change(PW, "short")).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);

        let (status, body) = send(&app, change(PW, "brand-new-pw")).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let (_, me) = send(&app, req("GET", "/api/console/me", None, Some(&token))).await;
        assert_eq!(me["must_change_password"], false);
        // Other sessions are revoked; the current one survives.
        let (status, _) = send(&app, req("POST", "/api/currentUser", None, Some(&other))).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        login_token(&app, "alice", "brand-new-pw").await;
        assert_eq!(
            login(&app, json!({"username": "alice", "password": PW}))
                .await
                .0,
            StatusCode::UNAUTHORIZED
        );
    }
}
