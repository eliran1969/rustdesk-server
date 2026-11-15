//! `/api/users`: user management (admin) and the "accessible users" list (any user).
//! Compatible with the client's `group_model.dart` and `res/users.py`.

use super::{devices::parse_page, JsonBody};
use crate::console::{
    auth::{self, AdminUser, AuthUser},
    db::{
        access,
        groups::{self, GroupError, GroupKind},
        sessions,
        users::{
            self, NewUser, UserFilter, UserListItem, UserPatch, UserWriteError, MIN_PASSWORD_LEN,
        },
    },
    error::ApiError,
    state::ConsoleState,
};
use axum::{
    extract::{Extension, Path, Query},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_derive::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;

const ADMIN_REQUIRED: &str = "Admin required!";
const NOT_SUPPORTED: &str = "Not supported yet";
const SELF_PROTECTED: &str = "Cannot change your own admin status";
const LAST_ADMIN: &str = "Cannot remove the last active administrator";

fn internal(e: impl std::fmt::Display) -> ApiError {
    ApiError::Internal(e.to_string())
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// A guid is 32 hex digits (16 bytes); anything else is "not found".
fn parse_guid(s: &str) -> Result<Vec<u8>, ApiError> {
    if s.len() != 32 || !s.is_ascii() {
        return Err(ApiError::NotFound);
    }
    s.as_bytes()
        .chunks(2)
        .map(|pair| {
            std::str::from_utf8(pair)
                .ok()
                .and_then(|p| u8::from_str_radix(p, 16).ok())
                .ok_or(ApiError::NotFound)
        })
        .collect()
}

/// Maps store outcomes to the client-facing errors.
pub(super) fn write_err(e: UserWriteError) -> ApiError {
    match e {
        UserWriteError::Duplicate => ApiError::BadRequest("User already exists".to_owned()),
        UserWriteError::NotFound => ApiError::NotFound,
        UserWriteError::LastAdmin => ApiError::BadRequest(LAST_ADMIN.to_owned()),
        UserWriteError::NotDisabled => ApiError::BadRequest("Disable the user first".to_owned()),
        UserWriteError::Db(e) => internal(e),
    }
}

fn check_password(p: &str) -> Result<(), ApiError> {
    if p.chars().count() < MIN_PASSWORD_LEN {
        return Err(ApiError::BadRequest(format!(
            "Password must be at least {} characters",
            MIN_PASSWORD_LEN
        )));
    }
    Ok(())
}

/// Resolves a user group by name; an empty name means `Default`.
async fn resolve_group(state: &ConsoleState, name: &str) -> Result<Vec<u8>, ApiError> {
    let group = if name.is_empty() {
        groups::default_user_group(&state.db).await.map(Some)
    } else {
        groups::find_by_name(&state.db, GroupKind::User, name).await
    };
    match group {
        Ok(Some(g)) => Ok(g.guid),
        Ok(None) | Err(GroupError::NotFound) => {
            Err(ApiError::BadRequest(format!("Unknown group: {name}")))
        }
        Err(e) => Err(internal(e)),
    }
}

/// bcrypt off the runtime, sharing the console-wide password slots so admin actions
/// cannot take more CPU from hbbs than logins can.
async fn hash_password(state: &ConsoleState, password: String) -> Result<String, ApiError> {
    let _slot = state.limiter.verify_slot().await?;
    auth::hash_password_blocking(password).await
}

fn row_json(item: &UserListItem) -> Value {
    let u = &item.user;
    json!({
        "guid": hex(&u.guid),
        "name": u.name,
        "display_name": u.display_name,
        "email": u.email,
        "note": u.note,
        "status": u.status,
        "is_admin": u.is_admin,
        "group_name": item.group_name,
        "created_at": item.created_at,
    })
}

/// `GET /api/users`. Admins see everyone; with `accessible` (any value, even empty)
/// every user may ask for the users accessible to them. `status=1` keeps only enabled
/// users; `name` and `group_name` are LIKE patterns bound as sent.
pub(crate) async fn list(
    Extension(state): Extension<ConsoleState>,
    AuthUser(user, _): AuthUser,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Response, ApiError> {
    let accessible = if params.contains_key("accessible") {
        access::accessible_user_guids(&state.db, &user)
            .await
            .map_err(internal)?
    } else if user.is_admin {
        None
    } else {
        return Ok((
            StatusCode::FORBIDDEN,
            Json(json!({ "error": ADMIN_REQUIRED })),
        )
            .into_response());
    };
    let (current, page_size) = parse_page(&params);
    let non_empty = |k: &str| params.get(k).map(String::as_str).filter(|v| !v.is_empty());
    let status = match params.get("status").map(|s| s.trim()) {
        Some("1") => Some(1),
        Some("0") => Some(0),
        _ => None,
    };
    let filter = UserFilter {
        name_like: non_empty("name"),
        group_name_like: non_empty("group_name"),
        status,
        guids: accessible.as_deref(),
    };
    let (total, rows) = users::list(&state.db, &filter, current, page_size)
        .await
        .map_err(internal)?;
    let data: Vec<Value> = rows.iter().map(row_json).collect();
    Ok(Json(json!({ "total": total, "data": data })).into_response())
}

#[derive(Deserialize)]
pub(crate) struct CreateUserRequest {
    name: String,
    password: String,
    group_name: Option<String>,
    email: Option<String>,
    note: Option<String>,
    is_admin: Option<bool>,
}

/// `POST /api/users`: creates an enabled user (in `Default` without `group_name`).
pub(crate) async fn create(
    Extension(state): Extension<ConsoleState>,
    AdminUser(_): AdminUser,
    JsonBody(req): JsonBody<CreateUserRequest>,
) -> Result<Json<Value>, ApiError> {
    let name = req.name.trim();
    if name.is_empty() {
        return Err(ApiError::BadRequest("Name is required".to_owned()));
    }
    check_password(&req.password)?;
    let group_guid = resolve_group(&state, req.group_name.as_deref().unwrap_or("").trim()).await?;
    if users::find_by_name(&state.db, name).await?.is_some() {
        return Err(write_err(UserWriteError::Duplicate));
    }
    let hash = hash_password(&state, req.password).await?;
    users::create_user_with_profile(
        &state.db,
        &NewUser {
            name,
            password_hash: &hash,
            is_admin: req.is_admin.unwrap_or(false),
            group_guid: Some(&group_guid),
            email: req.email.as_deref().unwrap_or(""),
            note: req.note.as_deref().unwrap_or(""),
        },
    )
    .await
    .map_err(write_err)?;
    Ok(Json(json!({})))
}

#[derive(Deserialize)]
pub(crate) struct UpdateUserRequest {
    display_name: Option<String>,
    email: Option<String>,
    note: Option<String>,
    password: Option<String>,
    group_name: Option<String>,
    is_admin: Option<bool>,
}

/// `PATCH /api/users/{guid}`: every field optional. A new password revokes all of the
/// user's sessions; an empty `group_name` moves the user to `Default`.
pub(crate) async fn update(
    Extension(state): Extension<ConsoleState>,
    AdminUser(caller): AdminUser,
    Path(guid): Path<String>,
    JsonBody(req): JsonBody<UpdateUserRequest>,
) -> Result<Json<Value>, ApiError> {
    let guid = parse_guid(&guid)?;
    if users::find_by_guid(&state.db, &guid).await?.is_none() {
        return Err(ApiError::NotFound);
    }
    if guid == caller.guid && req.is_admin == Some(false) {
        return Err(ApiError::BadRequest(SELF_PROTECTED.to_owned()));
    }
    if let Some(p) = &req.password {
        check_password(p)?;
    }
    let group_guid = match req.group_name.as_deref() {
        Some(name) => Some(resolve_group(&state, name.trim()).await?),
        None => None,
    };
    let hash = match req.password {
        Some(p) => Some(hash_password(&state, p).await?),
        None => None,
    };
    let patch = UserPatch {
        display_name: req.display_name.as_deref(),
        email: req.email.as_deref(),
        note: req.note.as_deref(),
        group_guid: group_guid.as_deref(),
        is_admin: req.is_admin,
        password_hash: hash.as_deref(),
    };
    users::apply_patch(&state.db, &guid, &patch)
        .await
        .map_err(write_err)?;
    Ok(Json(json!({})))
}

async fn set_status(
    state: &ConsoleState,
    caller: &users::User,
    guid: &str,
    enabled: bool,
) -> Result<Json<Value>, ApiError> {
    let guid = parse_guid(guid)?;
    if !enabled && guid == caller.guid {
        return Err(ApiError::BadRequest(SELF_PROTECTED.to_owned()));
    }
    users::set_status(&state.db, &guid, enabled)
        .await
        .map_err(write_err)?;
    Ok(Json(json!({})))
}

/// `POST /api/users/{guid}/enable`.
pub(crate) async fn enable(
    Extension(state): Extension<ConsoleState>,
    AdminUser(caller): AdminUser,
    Path(guid): Path<String>,
) -> Result<Json<Value>, ApiError> {
    set_status(&state, &caller, &guid, true).await
}

/// `POST /api/users/{guid}/disable`: also revokes all of the user's sessions.
pub(crate) async fn disable(
    Extension(state): Extension<ConsoleState>,
    AdminUser(caller): AdminUser,
    Path(guid): Path<String>,
) -> Result<Json<Value>, ApiError> {
    set_status(&state, &caller, &guid, false).await
}

/// `DELETE /api/users/{guid}`: only disabled users; unassigns their devices.
pub(crate) async fn delete(
    Extension(state): Extension<ConsoleState>,
    AdminUser(caller): AdminUser,
    Path(guid): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let guid = parse_guid(&guid)?;
    if guid == caller.guid {
        return Err(ApiError::BadRequest(SELF_PROTECTED.to_owned()));
    }
    users::delete_user(&state.db, &guid)
        .await
        .map_err(write_err)?;
    Ok(Json(json!({})))
}

#[derive(Deserialize)]
pub(crate) struct ForceLogoutRequest {
    user_guids: Vec<String>,
}

/// `POST /api/users/force-logout`: revokes every session of the listed users.
pub(crate) async fn force_logout(
    Extension(state): Extension<ConsoleState>,
    AdminUser(_): AdminUser,
    JsonBody(req): JsonBody<ForceLogoutRequest>,
) -> Result<Json<Value>, ApiError> {
    let guids = req
        .user_guids
        .iter()
        .map(|g| parse_guid(g).map_err(|_| ApiError::BadRequest(format!("Invalid user guid: {g}"))))
        .collect::<Result<Vec<_>, _>>()?;
    sessions::delete_sessions_of_users(&state.db, &guids)
        .await
        .map_err(internal)?;
    Ok(Json(json!({})))
}

/// Invite, 2FA enforcement/reset and login-verification toggles (not implemented yet).
pub(crate) async fn not_supported(AdminUser(_): AdminUser) -> ApiError {
    ApiError::BadRequest(NOT_SUPPORTED.to_owned())
}

#[cfg(test)]
mod tests {
    use crate::console::{
        db::groups::{self, GroupKind},
        test_util::{body_string, insert_peer, insert_user, login_token, test_app_with_db},
    };
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        Router,
    };
    use serde_json::{json, Value};
    use sqlx::SqlitePool;
    use tower::ServiceExt;

    const PW: &str = "correct-horse";

    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    async fn send(
        app: &Router,
        method: &str,
        uri: &str,
        token: Option<&str>,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut b = Request::builder().method(method).uri(uri);
        if let Some(t) = token {
            b = b.header("Authorization", format!("Bearer {t}"));
        }
        let body = body
            .map(|v| Body::from(v.to_string()))
            .unwrap_or_else(Body::empty);
        let res = app.clone().oneshot(b.body(body).unwrap()).await.unwrap();
        let status = res.status();
        let text = body_string(res).await;
        (
            status,
            serde_json::from_str(&text).unwrap_or(Value::String(text)),
        )
    }

    fn names(body: &Value) -> Vec<String> {
        let mut v: Vec<String> = body["data"]
            .as_array()
            .unwrap()
            .iter()
            .map(|u| u["name"].as_str().unwrap().to_owned())
            .collect();
        v.sort();
        v
    }

    async fn set_group(db: &SqlitePool, user: &[u8], group: &[u8]) {
        sqlx::query("update console_user set group_guid = ? where guid = ?")
            .bind(group)
            .bind(user)
            .execute(db)
            .await
            .unwrap();
    }

    async fn session_count(db: &SqlitePool, user: &[u8]) -> i64 {
        let (n,): (i64,) =
            sqlx::query_as("select count(*) from console_session where user_guid = ?")
                .bind(user)
                .fetch_one(db)
                .await
                .unwrap();
        n
    }

    #[tokio::test]
    async fn users_list_admin_paginated_and_filtered() {
        let (app, db) = test_app_with_db().await;
        insert_user(&db, "admin", PW, true).await;
        insert_user(&db, "adele", PW, false).await;
        let bob = insert_user(&db, "bob", PW, false).await;
        let sales = groups::create(&db, GroupKind::User, "Sales", "")
            .await
            .unwrap();
        set_group(&db, &bob, &sales).await;
        let token = login_token(&app, "admin", PW).await;

        let (s, all) = send(
            &app,
            "GET",
            "/api/users?current=1&pageSize=2",
            Some(&token),
            None,
        )
        .await;
        assert_eq!(s, StatusCode::OK, "{all}");
        assert_eq!(all["total"], 3);
        assert_eq!(all["data"].as_array().unwrap().len(), 2);
        let (_, p2) = send(
            &app,
            "GET",
            "/api/users?current=2&pageSize=2",
            Some(&token),
            None,
        )
        .await;
        assert_eq!(p2["data"].as_array().unwrap().len(), 1);

        let (_, by_name) = send(&app, "GET", "/api/users?name=%25ad%25", Some(&token), None).await;
        assert_eq!(by_name["total"], 2);
        assert_eq!(names(&by_name), vec!["adele", "admin"]);

        let (_, by_group) = send(
            &app,
            "GET",
            "/api/users?group_name=Sales",
            Some(&token),
            None,
        )
        .await;
        assert_eq!(by_group["total"], 1);
        let row = &by_group["data"][0];
        assert_eq!(row["name"], "bob");
        assert_eq!(row["group_name"], "Sales");
        assert_eq!(row["guid"], hex(&bob));
        assert_eq!(row["status"], 1);
        assert_eq!(row["is_admin"], false);
        let created = row["created_at"].as_str().unwrap();
        assert_eq!(created.len(), 19, "{created}");
        assert_eq!(&created[10..11], "T");
        for k in ["display_name", "email", "note"] {
            assert_eq!(row[k], "", "{k}");
        }

        // Both filters combine with AND.
        let (_, none) = send(
            &app,
            "GET",
            "/api/users?group_name=Sales&name=ad%25",
            Some(&token),
            None,
        )
        .await;
        assert_eq!(none["total"], 0);
    }

    #[tokio::test]
    async fn users_list_non_admin_requires_accessible() {
        let (app, db) = test_app_with_db().await;
        insert_user(&db, "bob", PW, false).await;
        let token = login_token(&app, "bob", PW).await;
        let (s, body) = send(&app, "GET", "/api/users", Some(&token), None).await;
        assert_eq!(s, StatusCode::FORBIDDEN);
        assert_eq!(body, json!({"error": "Admin required!"}));
        let (s, body) = send(
            &app,
            "GET",
            "/api/users?current=1&pageSize=100&accessible=&status=1",
            Some(&token),
            None,
        )
        .await;
        assert_eq!(s, StatusCode::OK, "{body}");
        assert_eq!(names(&body), vec!["bob"]);
    }

    #[tokio::test]
    async fn users_accessible_same_group_and_outgoing_rules() {
        let (app, db) = test_app_with_db().await;
        let admin = insert_user(&db, "admin", PW, true).await;
        let a = insert_user(&db, "a", PW, false).await;
        let b = insert_user(&db, "b", PW, false).await;
        let c = insert_user(&db, "c", PW, false).await;
        let d = insert_user(&db, "d", PW, false).await;
        let g1 = groups::create(&db, GroupKind::User, "G1", "")
            .await
            .unwrap();
        let g2 = groups::create(&db, GroupKind::User, "G2", "")
            .await
            .unwrap();
        let g3 = groups::create(&db, GroupKind::User, "G3", "")
            .await
            .unwrap();
        set_group(&db, &a, &g1).await;
        set_group(&db, &b, &g1).await;
        set_group(&db, &c, &g2).await;
        set_group(&db, &d, &g3).await;
        set_group(&db, &admin, &g3).await;
        sqlx::query(
            "insert into console_access_rule(owner_kind, owner_guid, direction, target_type, target_guid)
             values('user_group', ?, 'out', 0, ?)",
        )
        .bind(&g1)
        .bind(&g2)
        .execute(&db)
        .await
        .unwrap();
        // Disabled users are dropped by status=1.
        sqlx::query("update console_user set status = 0 where guid = ?")
            .bind(&d)
            .execute(&db)
            .await
            .unwrap();

        let uri = "/api/users?current=1&pageSize=100&accessible=&status=1";
        let ta = login_token(&app, "a", PW).await;
        let (s, body) = send(&app, "GET", uri, Some(&ta), None).await;
        assert_eq!(s, StatusCode::OK, "{body}");
        assert_eq!(names(&body), vec!["a", "b", "c"]);
        assert_eq!(body["total"], 3);

        let tc = login_token(&app, "c", PW).await;
        let (_, body) = send(&app, "GET", uri, Some(&tc), None).await;
        assert_eq!(names(&body), vec!["c"]);

        // An `in` rule on G3 granting type 2 (user c) makes G3 visible to c.
        sqlx::query(
            "insert into console_access_rule(owner_kind, owner_guid, direction, target_type, target_guid)
             values('user_group', ?, 'in', 2, ?)",
        )
        .bind(&g3)
        .bind(&c)
        .execute(&db)
        .await
        .unwrap();
        let (_, body) = send(&app, "GET", uri, Some(&tc), None).await;
        assert_eq!(names(&body), vec!["admin", "c"]);

        // Admin with `accessible` sees everyone (status=1 still applies).
        let tadmin = login_token(&app, "admin", PW).await;
        let (_, body) = send(&app, "GET", uri, Some(&tadmin), None).await;
        assert_eq!(names(&body), vec!["a", "admin", "b", "c"]);
    }

    #[tokio::test]
    async fn create_user_validates_and_defaults_group() {
        let (app, db) = test_app_with_db().await;
        insert_user(&db, "admin", PW, true).await;
        insert_user(&db, "bob", PW, false).await;
        groups::create(&db, GroupKind::User, "Sales", "")
            .await
            .unwrap();
        let token = login_token(&app, "admin", PW).await;
        let bob_token = login_token(&app, "bob", PW).await;

        let (s, _) = send(
            &app,
            "POST",
            "/api/users",
            Some(&bob_token),
            Some(json!({"name": "x", "password": "password123"})),
        )
        .await;
        assert_eq!(s, StatusCode::FORBIDDEN);

        let (s, body) = send(
            &app,
            "POST",
            "/api/users",
            Some(&token),
            Some(json!({"name": "carol", "password": "short"})),
        )
        .await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{body}");

        let (s, body) = send(
            &app,
            "POST",
            "/api/users",
            Some(&token),
            Some(json!({"name": "carol", "password": "password123", "group_name": "Nope"})),
        )
        .await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{body}");

        let (s, body) = send(
            &app,
            "POST",
            "/api/users",
            Some(&token),
            Some(json!({"name": "BOB", "password": "password123"})),
        )
        .await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
        assert_eq!(body, json!({"error": "User already exists"}));

        let (s, body) = send(
            &app,
            "POST",
            "/api/users",
            Some(&token),
            Some(
                json!({"name": "carol", "password": "password123", "email": "c@x.io", "note": "n"}),
            ),
        )
        .await;
        assert_eq!(s, StatusCode::OK, "{body}");
        let (s, body) = send(&app, "POST", "/api/users", Some(&token),
            Some(json!({"name": "dave", "password": "password123", "group_name": "sales", "is_admin": true}))).await;
        assert_eq!(s, StatusCode::OK, "{body}");

        let (_, list) = send(&app, "GET", "/api/users?name=carol", Some(&token), None).await;
        let row = &list["data"][0];
        assert_eq!(row["group_name"], "Default");
        assert_eq!(row["email"], "c@x.io");
        assert_eq!(row["note"], "n");
        assert_eq!(row["is_admin"], false);
        let (_, list) = send(&app, "GET", "/api/users?name=dave", Some(&token), None).await;
        assert_eq!(list["data"][0]["group_name"], "Sales");
        assert_eq!(list["data"][0]["is_admin"], true);
        // The new user can log in with the given password.
        login_token(&app, "carol", "password123").await;
    }

    #[tokio::test]
    async fn disable_user_deletes_sessions() {
        let (app, db) = test_app_with_db().await;
        insert_user(&db, "admin", PW, true).await;
        let bob = insert_user(&db, "bob", PW, false).await;
        let token = login_token(&app, "admin", PW).await;
        let bob_token = login_token(&app, "bob", PW).await;
        let (s, _) = send(&app, "GET", "/api/users?accessible", Some(&bob_token), None).await;
        assert_eq!(s, StatusCode::OK);

        let uri = format!("/api/users/{}/disable", hex(&bob));
        let (s, body) = send(&app, "POST", &uri, Some(&token), None).await;
        assert_eq!(s, StatusCode::OK, "{body}");
        assert_eq!(body, json!({}));
        assert_eq!(session_count(&db, &bob).await, 0);
        let (s, _) = send(&app, "GET", "/api/users?accessible", Some(&bob_token), None).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED);

        let (_, list) = send(&app, "GET", "/api/users?name=bob", Some(&token), None).await;
        assert_eq!(list["data"][0]["status"], 0);
        let uri = format!("/api/users/{}/enable", hex(&bob));
        let (s, _) = send(&app, "POST", &uri, Some(&token), None).await;
        assert_eq!(s, StatusCode::OK);
        login_token(&app, "bob", PW).await;

        // Invalid or unknown guid -> 404.
        let (s, _) = send(&app, "POST", "/api/users/zz/disable", Some(&token), None).await;
        assert_eq!(s, StatusCode::NOT_FOUND);
        let uri = format!("/api/users/{}/disable", "00".repeat(16));
        let (s, _) = send(&app, "POST", &uri, Some(&token), None).await;
        assert_eq!(s, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn delete_requires_disabled_and_unassigns_devices() {
        let (app, db) = test_app_with_db().await;
        insert_user(&db, "admin", PW, true).await;
        let bob = insert_user(&db, "bob", PW, false).await;
        login_token(&app, "bob", PW).await;
        insert_peer(&db, "100", b"u").await;
        sqlx::query("update peer set user = ? where id = '100'")
            .bind(&bob)
            .execute(&db)
            .await
            .unwrap();
        let g = groups::create(&db, GroupKind::User, "G", "").await.unwrap();
        sqlx::query(
            "insert into console_access_rule(owner_kind, owner_guid, direction, target_type, target_guid)
             values('user_group', ?, 'in', 2, ?)",
        )
        .bind(&g)
        .bind(&bob)
        .execute(&db)
        .await
        .unwrap();
        let token = login_token(&app, "admin", PW).await;

        let uri = format!("/api/users/{}", hex(&bob));
        let (s, body) = send(&app, "DELETE", &uri, Some(&token), None).await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
        assert_eq!(body, json!({"error": "Disable the user first"}));

        send(&app, "POST", &format!("{uri}/disable"), Some(&token), None).await;
        // A session created after disabling (e.g. inserted directly) is cleaned up too.
        sqlx::query(
            "insert into console_session(token_hash, user_guid, kind) values(x'01', ?, 'web')",
        )
        .bind(&bob)
        .execute(&db)
        .await
        .unwrap();
        let (s, body) = send(&app, "DELETE", &uri, Some(&token), None).await;
        assert_eq!(s, StatusCode::OK, "{body}");

        let (user,): (Option<Vec<u8>>,) = sqlx::query_as("select user from peer where id = '100'")
            .fetch_one(&db)
            .await
            .unwrap();
        assert!(user.is_none());
        assert_eq!(session_count(&db, &bob).await, 0);
        let (rules,): (i64,) = sqlx::query_as("select count(*) from console_access_rule")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(rules, 0);
        let (s, _) = send(&app, "DELETE", &uri, Some(&token), None).await;
        assert_eq!(s, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn admin_cannot_disable_or_demote_self() {
        let (app, db) = test_app_with_db().await;
        let admin = insert_user(&db, "admin", PW, true).await;
        let other = insert_user(&db, "root", PW, true).await;
        let token = login_token(&app, "admin", PW).await;
        let me = hex(&admin);
        let self_err = json!({"error": "Cannot change your own admin status"});
        let last_err = json!({"error": "Cannot remove the last active administrator"});

        let (s, body) = send(
            &app,
            "POST",
            &format!("/api/users/{me}/disable"),
            Some(&token),
            None,
        )
        .await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
        assert_eq!(body, self_err);
        let (s, body) = send(
            &app,
            "PATCH",
            &format!("/api/users/{me}"),
            Some(&token),
            Some(json!({"is_admin": false})),
        )
        .await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
        assert_eq!(body, self_err);
        let (s, body) = send(
            &app,
            "DELETE",
            &format!("/api/users/{me}"),
            Some(&token),
            None,
        )
        .await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
        assert_eq!(body, self_err);

        // Demoting the other admin works while `admin` remains enabled.
        let (s, body) = send(
            &app,
            "PATCH",
            &format!("/api/users/{}", hex(&other)),
            Some(&token),
            Some(json!({"is_admin": false, "display_name": "Root"})),
        )
        .await;
        assert_eq!(s, StatusCode::OK, "{body}");
        let (_, list) = send(&app, "GET", "/api/users?name=root", Some(&token), None).await;
        assert_eq!(list["data"][0]["is_admin"], false);
        assert_eq!(list["data"][0]["display_name"], "Root");

        // The last-admin guard: `admin` is now the only enabled admin. Through the
        // API only self-actions could reach it (already rejected above), so the
        // store guard is checked directly.
        use crate::console::db::users::{apply_patch, set_status, UserPatch, UserWriteError};
        let res = set_status(&db, &admin, false).await;
        assert!(matches!(res, Err(UserWriteError::LastAdmin)), "{res:?}");
        let demote = UserPatch {
            is_admin: Some(false),
            ..Default::default()
        };
        let res = apply_patch(&db, &admin, &demote).await;
        assert!(matches!(res, Err(UserWriteError::LastAdmin)), "{res:?}");
        // A disabled admin does not count: `root` promoted but disabled changes nothing.
        sqlx::query("update console_user set is_admin = 1, status = 0 where guid = ?")
            .bind(&other)
            .execute(&db)
            .await
            .unwrap();
        let res = set_status(&db, &admin, false).await;
        assert!(matches!(res, Err(UserWriteError::LastAdmin)), "{res:?}");
        // With a second enabled admin the guard lets the change through.
        set_status(&db, &other, true).await.unwrap();
        set_status(&db, &admin, false).await.unwrap();
        assert_eq!(session_count(&db, &admin).await, 0);
        // The handler maps the store's guard to the exact client-facing body.
        use axum::response::IntoResponse;
        let res = super::write_err(UserWriteError::LastAdmin).into_response();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        let body: Value = serde_json::from_str(&body_string(res).await).unwrap();
        assert_eq!(body, last_err);
    }

    #[tokio::test]
    async fn force_logout_deletes_sessions() {
        let (app, db) = test_app_with_db().await;
        insert_user(&db, "admin", PW, true).await;
        let bob = insert_user(&db, "bob", PW, false).await;
        let carol = insert_user(&db, "carol", PW, false).await;
        let token = login_token(&app, "admin", PW).await;
        let bob_token = login_token(&app, "bob", PW).await;
        login_token(&app, "bob", PW).await;
        login_token(&app, "carol", PW).await;
        assert_eq!(session_count(&db, &bob).await, 2);

        let (s, _) = send(
            &app,
            "POST",
            "/api/users/force-logout",
            Some(&bob_token),
            Some(json!({"user_guids": [hex(&carol)]})),
        )
        .await;
        assert_eq!(s, StatusCode::FORBIDDEN);

        let (s, body) = send(
            &app,
            "POST",
            "/api/users/force-logout",
            Some(&token),
            Some(json!({"user_guids": [hex(&bob)]})),
        )
        .await;
        assert_eq!(s, StatusCode::OK, "{body}");
        assert_eq!(body, json!({}));
        assert_eq!(session_count(&db, &bob).await, 0);
        assert_eq!(session_count(&db, &carol).await, 1);
        let (s, _) = send(&app, "GET", "/api/users?accessible", Some(&bob_token), None).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn unsupported_user_endpoints_return_not_supported() {
        let (app, db) = test_app_with_db().await;
        insert_user(&db, "admin", PW, true).await;
        insert_user(&db, "bob", PW, false).await;
        let token = login_token(&app, "admin", PW).await;
        let bob_token = login_token(&app, "bob", PW).await;
        let cases = [
            ("POST", "/api/users/invite"),
            ("PUT", "/api/users/tfa/totp/enforce"),
            ("PUT", "/api/users/disable_login_verification"),
            ("PUT", "/api/users/tfa/reset"),
        ];
        for (m, uri) in cases {
            let (s, body) = send(&app, m, uri, Some(&token), Some(json!({"user_guids": []}))).await;
            assert_eq!(s, StatusCode::BAD_REQUEST, "{m} {uri}");
            assert_eq!(body, json!({"error": "Not supported yet"}), "{m} {uri}");
            let (s, _) = send(&app, m, uri, Some(&bob_token), None).await;
            assert_eq!(s, StatusCode::FORBIDDEN, "{m} {uri}");
        }
    }
}
