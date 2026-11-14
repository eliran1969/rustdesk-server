use super::JsonBody;
use crate::console::{
    db::devices::{self, SYSINFO_VER},
    error::ApiError,
    state::ConsoleState,
};
use axum::{extract::Extension, Json};
use serde_json::{json, Map, Value};

pub(crate) async fn login_options() -> Json<Value> {
    Json(json!([]))
}

fn str_field<'a>(m: &'a Map<String, Value>, k: &str) -> &'a str {
    m.get(k).and_then(Value::as_str).unwrap_or("")
}

/// `POST /api/sysinfo` (unauthenticated; the device proves itself with id + uuid).
pub(crate) async fn sysinfo(
    Extension(state): Extension<ConsoleState>,
    JsonBody(mut body): JsonBody<Map<String, Value>>,
) -> Result<&'static str, ApiError> {
    let guid = match devices::verify_peer(
        &state.db,
        str_field(&body, "id"),
        str_field(&body, "uuid"),
    )
    .await?
    {
        Some(guid) => guid,
        None => return Ok("ID_NOT_FOUND"),
    };
    body.remove("uuid");
    let preset = body
        .get("preset-username")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    let text = serde_json::to_string(&body).map_err(|e| ApiError::Internal(e.to_string()))?;
    devices::upsert_sysinfo(&state.db, &guid, &text).await?;
    if let Some(name) = preset {
        devices::assign_if_unassigned(&state.db, &guid, &name).await?;
    }
    Ok("SYSINFO_UPDATED")
}

pub(crate) async fn sysinfo_ver() -> &'static str {
    SYSINFO_VER
}

/// `POST /api/heartbeat`. Failed verification answers `{}` and writes nothing.
pub(crate) async fn heartbeat(
    Extension(state): Extension<ConsoleState>,
    JsonBody(body): JsonBody<Map<String, Value>>,
) -> Result<Json<Value>, ApiError> {
    let guid = match devices::verify_peer(
        &state.db,
        str_field(&body, "id"),
        str_field(&body, "uuid"),
    )
    .await?
    {
        Some(guid) => guid,
        None => return Ok(Json(json!({}))),
    };
    if devices::touch_last_online(&state.db, &guid).await? {
        Ok(Json(json!({})))
    } else {
        Ok(Json(json!({"sysinfo": true})))
    }
}

#[cfg(test)]
mod tests {
    use crate::console::test_util::{
        body_string, insert_peer, insert_user, test_app, test_app_with_db,
    };
    use axum::{body::Body, http::Request, Router};
    use serde_json::{json, Value};
    use tower::ServiceExt;

    #[tokio::test]
    async fn login_options_returns_empty_json_array() {
        let app = test_app().await;
        let res = app
            .oneshot(Request::get("/api/login-options").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        assert_eq!(body_string(res).await, "[]");
    }

    fn post(path: &str, body: &Value) -> Request<Body> {
        Request::post(path).body(Body::from(body.to_string())).unwrap()
    }

    async fn call(app: &Router, path: &str, body: Value) -> (u16, String) {
        let res = app.clone().oneshot(post(path, &body)).await.unwrap();
        (res.status().as_u16(), body_string(res).await)
    }

    fn b64(b: &[u8]) -> String {
        base64::encode(b)
    }

    async fn sysinfo_count(db: &sqlx::SqlitePool) -> i64 {
        sqlx::query_scalar("select count(*) from console_peer_sysinfo")
            .fetch_one(db)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn sysinfo_unknown_id_returns_id_not_found() {
        let (app, _db) = test_app_with_db().await;
        let (st, body) = call(&app, "/api/sysinfo", json!({"id": "nope", "uuid": b64(b"u1")})).await;
        assert_eq!((st, body.as_str()), (200, "ID_NOT_FOUND"));
    }

    #[tokio::test]
    async fn sysinfo_wrong_uuid_rejected_and_not_written() {
        let (app, db) = test_app_with_db().await;
        insert_peer(&db, "p1", b"u1").await;
        let (_, body) = call(
            &app,
            "/api/sysinfo",
            json!({"id": "p1", "uuid": b64(b"u2"), "hostname": "h"}),
        )
        .await;
        assert_eq!(body, "ID_NOT_FOUND");
        assert_eq!(sysinfo_count(&db).await, 0);
        let (_, body) = call(&app, "/api/sysinfo", json!({"id": "p1", "uuid": "!!notb64"})).await;
        assert_eq!(body, "ID_NOT_FOUND");
        assert_eq!(sysinfo_count(&db).await, 0);
    }

    #[tokio::test]
    async fn sysinfo_valid_stores_json_and_returns_updated() {
        let (app, db) = test_app_with_db().await;
        insert_peer(&db, "p1", b"u1").await;
        let (_, body) = call(
            &app,
            "/api/sysinfo",
            json!({"id": "p1", "uuid": b64(b"u1"), "hostname": "box", "os": "linux", "extra": 1}),
        )
        .await;
        assert_eq!(body, "SYSINFO_UPDATED");
        let stored: String = sqlx::query_scalar("select sysinfo from console_peer_sysinfo")
            .fetch_one(&db)
            .await
            .unwrap();
        let v: Value = serde_json::from_str(&stored).unwrap();
        assert_eq!(v["hostname"], "box");
        assert_eq!(v["os"], "linux");
        assert_eq!(v["id"], "p1");
        assert!(v.get("uuid").is_none());
    }

    #[tokio::test]
    async fn sysinfo_ver_returns_version() {
        let app = test_app().await;
        let (st, body) = call(&app, "/api/sysinfo_ver", json!({})).await;
        assert_eq!((st, body.as_str()), (200, "1"));
    }

    #[tokio::test]
    async fn heartbeat_requests_sysinfo_when_missing() {
        let (app, db) = test_app_with_db().await;
        insert_peer(&db, "p1", b"u1").await;
        let (_, body) = call(
            &app,
            "/api/heartbeat",
            json!({"id": "p1", "uuid": b64(b"u1"), "ver": 1}),
        )
        .await;
        assert_eq!(body, "{\"sysinfo\":true}");
        let (_, body) = call(&app, "/api/heartbeat", json!({"id": "p1", "uuid": b64(b"bad")})).await;
        assert_eq!(body, "{}");
    }

    #[tokio::test]
    async fn heartbeat_updates_last_online() {
        let (app, db) = test_app_with_db().await;
        insert_peer(&db, "p1", b"u1").await;
        let uuid = b64(b"u1");
        call(&app, "/api/sysinfo", json!({"id": "p1", "uuid": uuid, "hostname": "h"})).await;
        let before: Option<String> =
            sqlx::query_scalar("select last_online from console_peer_sysinfo")
                .fetch_one(&db)
                .await
                .unwrap();
        assert!(before.is_none());
        let (_, body) = call(&app, "/api/heartbeat", json!({"id": "p1", "uuid": uuid})).await;
        assert_eq!(body, "{}");
        let after: Option<String> =
            sqlx::query_scalar("select last_online from console_peer_sysinfo")
                .fetch_one(&db)
                .await
                .unwrap();
        assert!(after.is_some());
    }

    #[tokio::test]
    async fn sysinfo_preset_username_assigns_unassigned_peer() {
        let (app, db) = test_app_with_db().await;
        insert_peer(&db, "p1", b"u1").await;
        insert_peer(&db, "p2", b"u2").await;
        let alice = insert_user(&db, "alice", "pw-alice-123", false).await;
        insert_user(&db, "bob", "pw-bob-12345", false).await;
        sqlx::query("update console_user set status = 0 where name = 'bob'")
            .execute(&db)
            .await
            .unwrap();
        let user_of = |id: &'static str| {
            let db = db.clone();
            async move {
                sqlx::query_scalar::<_, Option<Vec<u8>>>("select user from peer where id = ?")
                    .bind(id)
                    .fetch_one(&db)
                    .await
                    .unwrap()
            }
        };
        call(&app, "/api/sysinfo", json!({"id": "p1", "uuid": b64(b"u1"), "preset-username": "alice"})).await;
        assert_eq!(user_of("p1").await, Some(alice.clone()));
        // already assigned: not overwritten
        call(&app, "/api/sysinfo", json!({"id": "p1", "uuid": b64(b"u1"), "preset-username": "bob"})).await;
        assert_eq!(user_of("p1").await, Some(alice));
        // disabled user: not assigned
        call(&app, "/api/sysinfo", json!({"id": "p2", "uuid": b64(b"u2"), "preset-username": "bob"})).await;
        assert_eq!(user_of("p2").await, None);
    }

    #[tokio::test]
    async fn bind_fails_cleanly_when_port_in_use() {
        let hold = std::net::TcpListener::bind("0.0.0.0:0").unwrap();
        let port = hold.local_addr().unwrap().port();
        let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
        assert!(crate::console::bind(addr).is_err());
        drop(hold);
    }
}
