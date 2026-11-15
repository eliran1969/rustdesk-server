use super::{users::parse_guid, JsonBody};
use crate::console::{
    auth::{AdminUser, AuthUser},
    db::devices::{is_online, verify_peer},
    error::ApiError,
    state::ConsoleState,
};
use axum::{
    extract::{Extension, Path, Query},
    response::{IntoResponse, Response},
    Json,
};
use serde_derive::Deserialize;
use serde_json::{json, Map, Value};
use sqlx::{Row, SqliteConnection};
use std::collections::HashMap;

const DEFAULT_PAGE_SIZE: i64 = 30;
const MAX_PAGE_SIZE: i64 = 100;

const FROM: &str = "from peer \
    left join console_peer_sysinfo s on s.peer_guid = peer.guid \
    left join console_user u on u.guid = peer.user \
    left join console_user_group ug on ug.guid = u.group_guid \
    left join console_peer_ext e on e.peer_guid = peer.guid \
    left join console_device_group dg on dg.guid = e.device_group_guid";

pub(crate) fn parse_page(params: &HashMap<String, String>) -> (i64, i64) {
    let num = |k: &str, default: i64| {
        params
            .get(k)
            .and_then(|v| v.trim().parse::<i64>().ok())
            .unwrap_or(default)
    };
    (
        num("current", 1).max(1),
        num("pageSize", DEFAULT_PAGE_SIZE).clamp(1, MAX_PAGE_SIZE),
    )
}

/// Builds the `info` object the client reads (`PeerPayload`): the stored sysinfo with
/// `device_name` taken from the console override, else from `hostname`, and `username`
/// taken from the console override when set. The sysinfo `uuid` is never exposed.
pub(crate) fn build_info(
    sysinfo: Option<&str>,
    device_name: Option<String>,
    device_username: Option<String>,
) -> Value {
    let mut info = match sysinfo.and_then(|s| serde_json::from_str::<Value>(s).ok()) {
        Some(Value::Object(m)) => m,
        _ => Map::new(),
    };
    info.remove("uuid");
    if let Some(name) = device_name {
        info.insert("device_name".to_owned(), Value::String(name));
    } else if let Some(hostname @ Value::String(_)) = info.get("hostname").cloned() {
        info.insert("device_name".to_owned(), hostname);
    }
    if let Some(username) = device_username {
        info.insert("username".to_owned(), Value::String(username));
    }
    Value::Object(info)
}

/// `GET /api/devices`: paginated device list, compatible with `res/devices.py`.
pub(crate) async fn list(
    Extension(state): Extension<ConsoleState>,
    AuthUser(user, _): AuthUser,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let (current, page_size) = parse_page(&params);
    let id_filter = params.get("id").filter(|v| !v.is_empty());
    let name_filter = params.get("device_name").filter(|v| !v.is_empty());
    let user_filter = params.get("user_name").filter(|v| !v.is_empty());
    let group_filter = params.get("group_name").filter(|v| !v.is_empty());
    let device_group_filter = params.get("device_group_name").filter(|v| !v.is_empty());

    let mut conds: Vec<&str> = Vec::new();
    if !user.is_admin {
        conds.push("peer.user = ?");
    }
    if id_filter.is_some() {
        conds.push("peer.id like ?");
    }
    if name_filter.is_some() {
        conds.push("coalesce(e.device_name, json_extract(s.sysinfo, '$.hostname')) like ?");
    }
    if user_filter.is_some() {
        conds.push("u.name like ?");
    }
    if group_filter.is_some() {
        conds.push("ug.name like ?");
    }
    if device_group_filter.is_some() {
        conds.push("dg.name like ?");
    }
    let where_sql = if conds.is_empty() {
        String::new()
    } else {
        format!("where {}", conds.join(" and "))
    };

    // Filter values are always bound, never spliced into the SQL.
    macro_rules! bind_filters {
        ($q:expr) => {{
            let mut q = $q;
            if !user.is_admin {
                q = q.bind(user.guid.clone());
            }
            if let Some(v) = id_filter {
                q = q.bind(v.clone());
            }
            if let Some(v) = name_filter {
                q = q.bind(v.clone());
            }
            if let Some(v) = user_filter {
                q = q.bind(v.clone());
            }
            if let Some(v) = group_filter {
                q = q.bind(v.clone());
            }
            if let Some(v) = device_group_filter {
                q = q.bind(v.clone());
            }
            q
        }};
    }

    let count_sql = format!("select count(*) {FROM} {where_sql}");
    let (total,): (i64,) = bind_filters!(sqlx::query_as(&count_sql))
        .fetch_one(&state.db)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let select_sql = format!(
        "select peer.guid as guid, peer.id as id, peer.status as status, peer.note as note, \
         s.sysinfo as sysinfo, cast(coalesce(s.last_online, peer.created_at) as text) as last_online, u.name as user_name, \
         ug.name as group_name, dg.name as device_group_name, e.device_name as device_name, \
         e.device_username as device_username \
         {FROM} {where_sql} order by peer.id limit ? offset ?"
    );
    let rows = bind_filters!(sqlx::query(&select_sql))
        .bind(page_size)
        .bind((current - 1).saturating_mul(page_size))
        .fetch_all(&state.db)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let internal = |e: sqlx::Error| ApiError::Internal(e.to_string());
    let mut data = Vec::with_capacity(rows.len());
    for row in rows {
        let guid: Vec<u8> = row.try_get("guid").map_err(internal)?;
        let id: String = row.try_get("id").map_err(internal)?;
        let status: Option<i64> = row.try_get("status").map_err(internal)?;
        let note: Option<String> = row.try_get("note").map_err(internal)?;
        let sysinfo: Option<String> = row.try_get("sysinfo").map_err(internal)?;
        let last_online: Option<String> = row.try_get("last_online").map_err(internal)?;
        let user_name: Option<String> = row.try_get("user_name").map_err(internal)?;
        let group_name: Option<String> = row.try_get("group_name").map_err(internal)?;
        let device_group_name: Option<String> =
            row.try_get("device_group_name").map_err(internal)?;
        let device_name: Option<String> = row.try_get("device_name").map_err(internal)?;
        let device_username: Option<String> = row.try_get("device_username").map_err(internal)?;
        let online = is_online(&state.pm, &id).await;
        data.push((
            online,
            json!({
                "guid": guid.iter().map(|b| format!("{b:02x}")).collect::<String>(),
                "id": id,
                "status": status.unwrap_or(1),
                "is_online": online,
                "last_online": last_online.map(|t| t.replacen(' ', "T", 1)),
                "user_name": user_name.unwrap_or_default(),
                "group_name": group_name.unwrap_or_default(),
                "device_group_name": device_group_name.unwrap_or_default(),
                "note": note.unwrap_or_default(),
                "info": build_info(sysinfo.as_deref(), device_name, device_username),
            }),
        ));
    }
    // Stable: online devices first within the page, `id` order otherwise.
    data.sort_by_key(|(online, _)| !*online);
    let data: Vec<Value> = data.into_iter().map(|(_, v)| v).collect();

    Ok(Json(json!({ "total": total, "data": data })))
}

const NOT_SUPPORTED: &str = "Not supported yet";

fn internal(e: impl std::fmt::Display) -> ApiError {
    ApiError::Internal(e.to_string())
}

/// Device fields a PATCH, an `assign` or a `--assign` CLI call may change. `None` leaves a
/// field alone; for `user_name`, `device_group_name`, `device_name` and
/// `device_username` an empty string clears it.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct DeviceChanges {
    note: Option<String>,
    device_name: Option<String>,
    device_username: Option<String>,
    user_name: Option<String>,
    device_group_name: Option<String>,
}

/// Empty string means "clear the override".
fn non_empty(v: &str) -> Option<&str> {
    (!v.is_empty()).then_some(v)
}

/// Applies `changes` to the device `peer_guid` on `conn` (normally a transaction, so a
/// failure part-way leaves nothing behind). Of the `peer` columns only `user` and `note`
/// are written; the overrides go to `console_peer_ext`.
pub(crate) async fn apply_device_changes(
    conn: &mut SqliteConnection,
    peer_guid: &[u8],
    changes: &DeviceChanges,
) -> Result<(), ApiError> {
    if let Some(name) = &changes.user_name {
        let user: Option<Vec<u8>> = match non_empty(name) {
            None => None,
            Some(name) => Some(
                sqlx::query_scalar("select guid from console_user where name = ?")
                    .bind(name)
                    .fetch_optional(&mut *conn)
                    .await
                    .map_err(internal)?
                    .ok_or_else(|| ApiError::BadRequest(format!("Unknown user: {name}")))?,
            ),
        };
        sqlx::query("update peer set user = ? where guid = ?")
            .bind(user)
            .bind(peer_guid)
            .execute(&mut *conn)
            .await
            .map_err(internal)?;
    }
    if let Some(name) = &changes.device_group_name {
        let group: Option<Vec<u8>> = match non_empty(name) {
            None => None,
            Some(name) => Some(
                sqlx::query_scalar("select guid from console_device_group where name = ?")
                    .bind(name)
                    .fetch_optional(&mut *conn)
                    .await
                    .map_err(internal)?
                    .ok_or_else(|| ApiError::BadRequest(format!("Unknown device group: {name}")))?,
            ),
        };
        sqlx::query(
            "insert into console_peer_ext(peer_guid, device_group_guid) values(?1, ?2)
             on conflict(peer_guid) do update set device_group_guid = excluded.device_group_guid",
        )
        .bind(peer_guid)
        .bind(group)
        .execute(&mut *conn)
        .await
        .map_err(internal)?;
    }
    if let Some(name) = &changes.device_name {
        sqlx::query(
            "insert into console_peer_ext(peer_guid, device_name) values(?1, ?2)
             on conflict(peer_guid) do update set device_name = excluded.device_name",
        )
        .bind(peer_guid)
        .bind(non_empty(name))
        .execute(&mut *conn)
        .await
        .map_err(internal)?;
    }
    if let Some(name) = &changes.device_username {
        sqlx::query(
            "insert into console_peer_ext(peer_guid, device_username) values(?1, ?2)
             on conflict(peer_guid) do update set device_username = excluded.device_username",
        )
        .bind(peer_guid)
        .bind(non_empty(name))
        .execute(&mut *conn)
        .await
        .map_err(internal)?;
    }
    if let Some(note) = &changes.note {
        sqlx::query("update peer set note = ? where guid = ?")
            .bind(note)
            .bind(peer_guid)
            .execute(&mut *conn)
            .await
            .map_err(internal)?;
    }
    Ok(())
}

async fn ensure_peer(conn: &mut SqliteConnection, guid: &[u8]) -> Result<(), ApiError> {
    let n: i64 = sqlx::query_scalar("select count(*) from peer where guid = ?")
        .bind(guid)
        .fetch_one(conn)
        .await
        .map_err(internal)?;
    if n == 0 {
        return Err(ApiError::NotFound);
    }
    Ok(())
}

async fn change_device(
    state: &ConsoleState,
    guid: &str,
    changes: &DeviceChanges,
) -> Result<Json<Value>, ApiError> {
    let guid = parse_guid(guid)?;
    let mut tx = state.db.begin().await.map_err(internal)?;
    ensure_peer(&mut tx, &guid).await?;
    apply_device_changes(&mut tx, &guid, changes).await?;
    tx.commit().await.map_err(internal)?;
    Ok(Json(json!({})))
}

async fn set_status(state: &ConsoleState, guid: &str, status: i64) -> Result<Json<Value>, ApiError> {
    let guid = parse_guid(guid)?;
    let res = sqlx::query("update peer set status = ? where guid = ?")
        .bind(status)
        .bind(&guid)
        .execute(&state.db)
        .await
        .map_err(internal)?;
    if res.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }
    Ok(Json(json!({})))
}

/// `POST /api/devices/{guid}/enable`.
pub(crate) async fn enable(
    Extension(state): Extension<ConsoleState>,
    AdminUser(_): AdminUser,
    Path(guid): Path<String>,
) -> Result<Json<Value>, ApiError> {
    set_status(&state, &guid, 1).await
}

/// `POST /api/devices/{guid}/disable`.
pub(crate) async fn disable(
    Extension(state): Extension<ConsoleState>,
    AdminUser(_): AdminUser,
    Path(guid): Path<String>,
) -> Result<Json<Value>, ApiError> {
    set_status(&state, &guid, 0).await
}

/// `DELETE /api/devices/{guid}`: only a disabled device. Removes its console rows and its
/// `peer` row in one transaction, then drops it from hbbs' in-memory map.
pub(crate) async fn delete(
    Extension(state): Extension<ConsoleState>,
    AdminUser(_): AdminUser,
    Path(guid): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let guid = parse_guid(&guid)?;
    let mut tx = state.db.begin().await.map_err(internal)?;
    let row: Option<(String, Option<i64>)> =
        sqlx::query_as("select id, status from peer where guid = ?")
            .bind(&guid)
            .fetch_optional(&mut tx)
            .await
            .map_err(internal)?;
    let (id, status) = row.ok_or(ApiError::NotFound)?;
    if status != Some(0) {
        return Err(ApiError::BadRequest("Disable the device first".to_owned()));
    }
    for sql in [
        "delete from console_peer_sysinfo where peer_guid = ?",
        "delete from console_peer_ext where peer_guid = ?",
        "delete from peer where guid = ?",
    ] {
        sqlx::query(sql)
            .bind(&guid)
            .execute(&mut tx)
            .await
            .map_err(internal)?;
    }
    tx.commit().await.map_err(internal)?;
    state.pm.remove(&id).await;
    Ok(Json(json!({})))
}

/// `PATCH /api/devices/{guid}`.
pub(crate) async fn update(
    Extension(state): Extension<ConsoleState>,
    AdminUser(_): AdminUser,
    Path(guid): Path<String>,
    JsonBody(changes): JsonBody<DeviceChanges>,
) -> Result<Json<Value>, ApiError> {
    change_device(&state, &guid, &changes).await
}

#[derive(Deserialize)]
pub(crate) struct AssignRequest {
    #[serde(rename = "type")]
    kind: String,
    value: String,
}

/// `POST /api/devices/{guid}/assign` (`res/devices.py`): one `{type, value}` change.
pub(crate) async fn assign(
    Extension(state): Extension<ConsoleState>,
    AdminUser(_): AdminUser,
    Path(guid): Path<String>,
    JsonBody(req): JsonBody<AssignRequest>,
) -> Result<Json<Value>, ApiError> {
    let value = Some(req.value);
    let changes = match req.kind.as_str() {
        "user_name" => DeviceChanges {
            user_name: value,
            ..Default::default()
        },
        "device_group_name" => DeviceChanges {
            device_group_name: value,
            ..Default::default()
        },
        "note" => DeviceChanges {
            note: value,
            ..Default::default()
        },
        "device_username" => DeviceChanges {
            device_username: value,
            ..Default::default()
        },
        "device_name" => DeviceChanges {
            device_name: value,
            ..Default::default()
        },
        "strategy_name" | "ab" => return Err(ApiError::BadRequest(NOT_SUPPORTED.to_owned())),
        other => return Err(ApiError::BadRequest(format!("Invalid type: {other}"))),
    };
    change_device(&state, &guid, &changes).await
}

#[derive(Deserialize)]
pub(crate) struct CliRequest {
    id: String,
    uuid: String,
    #[serde(flatten)]
    changes: DeviceChanges,
    strategy_name: Option<String>,
    address_book_name: Option<String>,
}

/// `POST /api/devices/cli` (the client's `--assign`): the client prints the response
/// body, or "Done!" when it is empty, so outcomes are plain text with status 200.
pub(crate) async fn cli(
    Extension(state): Extension<ConsoleState>,
    AdminUser(_): AdminUser,
    JsonBody(req): JsonBody<CliRequest>,
) -> Result<Response, ApiError> {
    let guid = match verify_peer(&state.db, &req.id, &req.uuid).await? {
        Some(guid) => guid,
        None => return Ok("Device not found".into_response()),
    };
    let mut tx = state.db.begin().await.map_err(internal)?;
    apply_device_changes(&mut tx, &guid, &req.changes).await?;
    tx.commit().await.map_err(internal)?;
    let unsupported: Vec<&str> = [
        ("strategy_name", req.strategy_name.is_some()),
        ("address_book_name", req.address_book_name.is_some()),
    ]
    .into_iter()
    .filter_map(|(name, sent)| sent.then_some(name))
    .collect();
    if unsupported.is_empty() {
        Ok(().into_response())
    } else {
        Ok(format!("Applied; not supported yet: {}", unsupported.join(", ")).into_response())
    }
}

/// `POST /api/devices/deploy`: this server does not require deployment.
pub(crate) async fn deploy() -> Json<Value> {
    Json(json!({ "result": "NOT_ENABLED" }))
}

#[cfg(test)]
mod tests {
    use crate::console::test_util::{
        body_string, insert_peer, insert_user, login_token, test_app_with_db, test_app_with_db_pm,
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

    async fn get(app: &Router, uri: &str, token: Option<&str>) -> (StatusCode, Value) {
        let mut b = Request::builder().method("GET").uri(uri);
        if let Some(t) = token {
            b = b.header("Authorization", format!("Bearer {t}"));
        }
        let res = app
            .clone()
            .oneshot(b.body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = res.status();
        let text = body_string(res).await;
        (
            status,
            serde_json::from_str(&text).unwrap_or(Value::String(text)),
        )
    }

    async fn set_sysinfo(db: &SqlitePool, id: &str, sysinfo: &str) {
        sqlx::query(
            "insert into console_peer_sysinfo(peer_guid, sysinfo, last_online) \
             select guid, ?, current_timestamp from peer where id = ?",
        )
        .bind(sysinfo)
        .bind(id)
        .execute(db)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn devices_admin_sees_all_paginated() {
        let (app, db) = test_app_with_db().await;
        insert_user(&db, "admin", PW, true).await;
        for i in 0..35 {
            insert_peer(&db, &format!("{i:03}"), b"u").await;
        }
        let token = login_token(&app, "admin", PW).await;
        let (s, p1) = get(&app, "/api/devices?current=1&pageSize=30", Some(&token)).await;
        assert_eq!(s, StatusCode::OK, "{p1}");
        assert_eq!(p1["total"], 35);
        assert_eq!(p1["data"].as_array().unwrap().len(), 30);
        let (_, p2) = get(&app, "/api/devices?current=2&pageSize=30", Some(&token)).await;
        assert_eq!(p2["data"].as_array().unwrap().len(), 5);
        assert_eq!(p2["total"], 35);
        assert_eq!(p2["data"][0]["status"], 1);
    }

    #[tokio::test]
    async fn devices_last_online_falls_back_to_created_at() {
        let (app, db) = test_app_with_db().await;
        insert_user(&db, "admin", PW, true).await;
        insert_peer(&db, "100", b"a").await;
        let token = login_token(&app, "admin", PW).await;
        let (_, body) = get(&app, "/api/devices", Some(&token)).await;
        let t = body["data"][0]["last_online"].as_str().unwrap();
        assert_eq!(t.len(), 19, "{t}");
        assert_eq!(&t[10..11], "T");
        let (created,): (String,) =
            sqlx::query_as("select cast(created_at as text) from peer where id = '100'")
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(t, created.replacen(' ', "T", 1));
    }

    #[tokio::test]
    async fn devices_filter_by_name_wildcard() {
        let (app, db) = test_app_with_db().await;
        insert_user(&db, "admin", PW, true).await;
        insert_peer(&db, "100", b"a").await;
        insert_peer(&db, "200", b"b").await;
        set_sysinfo(&db, "100", r#"{"hostname":"my-laptop"}"#).await;
        set_sysinfo(&db, "200", r#"{"hostname":"desktop"}"#).await;
        let token = login_token(&app, "admin", PW).await;
        let (_, body) = get(&app, "/api/devices?device_name=%25lap%25", Some(&token)).await;
        assert_eq!(body["total"], 1);
        assert_eq!(body["data"].as_array().unwrap().len(), 1);
        assert_eq!(body["data"][0]["id"], "100");
        let t = body["data"][0]["last_online"].as_str().unwrap();
        assert_eq!(t.len(), 19);
        assert_eq!(&t[10..11], "T");
    }

    #[tokio::test]
    async fn devices_non_admin_sees_only_own() {
        let (app, db) = test_app_with_db().await;
        let guid = insert_user(&db, "bob", PW, false).await;
        insert_peer(&db, "100", b"a").await;
        insert_peer(&db, "200", b"b").await;
        sqlx::query("update peer set user = ? where id = '100'")
            .bind(guid)
            .execute(&db)
            .await
            .unwrap();
        let token = login_token(&app, "bob", PW).await;
        let (_, body) = get(&app, "/api/devices", Some(&token)).await;
        assert_eq!(body["total"], 1);
        assert_eq!(body["data"][0]["id"], "100");
        assert_eq!(body["data"][0]["user_name"], "bob");
    }

    #[tokio::test]
    async fn devices_without_token_401() {
        let (app, _db) = test_app_with_db().await;
        let (s, _) = get(&app, "/api/devices", None).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn devices_info_maps_hostname_to_device_name() {
        let (app, db) = test_app_with_db().await;
        insert_user(&db, "admin", PW, true).await;
        insert_peer(&db, "100", b"a").await;
        set_sysinfo(
            &db,
            "100",
            r#"{"hostname":"box","os":"windows","username":"u1","uuid":"secret","version":"1.4"}"#,
        )
        .await;
        let token = login_token(&app, "admin", PW).await;
        let (_, body) = get(&app, "/api/devices", Some(&token)).await;
        let info = &body["data"][0]["info"];
        assert_eq!(info["device_name"], "box");
        assert_eq!(info["os"], "windows");
        assert_eq!(info["username"], "u1");
        assert!(info.get("uuid").is_none());
        assert_eq!(body["data"][0]["guid"].as_str().unwrap().len(), 32);
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

    async fn guid_of(app: &Router, token: &str, id: &str) -> String {
        let (_, b) = get(app, &format!("/api/devices?id={id}"), Some(token)).await;
        b["data"][0]["guid"].as_str().unwrap().to_owned()
    }

    async fn peer_col(db: &SqlitePool, id: &str, col: &str) -> Option<String> {
        sqlx::query_scalar(&format!("select cast({col} as text) from peer where id = ?"))
            .bind(id)
            .fetch_one(db)
            .await
            .unwrap()
    }

    async fn peer_user_name(db: &SqlitePool, id: &str) -> Option<String> {
        sqlx::query_scalar(
            "select u.name from peer p left join console_user u on u.guid = p.user where p.id = ?",
        )
        .bind(id)
        .fetch_one(db)
        .await
        .unwrap()
    }

    async fn ext_of(
        db: &SqlitePool,
        id: &str,
    ) -> (Option<String>, Option<String>, Option<String>) {
        sqlx::query_as(
            "select g.name, e.device_name, e.device_username from console_peer_ext e
             join peer p on p.guid = e.peer_guid
             left join console_device_group g on g.guid = e.device_group_guid where p.id = ?",
        )
        .bind(id)
        .fetch_optional(db)
        .await
        .unwrap()
        .unwrap_or((None, None, None))
    }

    async fn mk_device_group(app: &Router, token: &str, name: &str) {
        let (s, b) = send(
            app,
            "POST",
            "/api/device-groups",
            Some(token),
            Some(json!({ "name": name })),
        )
        .await;
        assert_eq!(s, StatusCode::OK, "{b}");
    }

    #[tokio::test]
    async fn devices_list_includes_groups_and_overrides() {
        let (app, db) = test_app_with_db().await;
        insert_user(&db, "admin", PW, true).await;
        insert_peer(&db, "100", b"a").await;
        insert_peer(&db, "200", b"b").await;
        set_sysinfo(&db, "100", r#"{"hostname":"box","username":"u1","os":"linux"}"#).await;
        set_sysinfo(&db, "200", r#"{"hostname":"other","username":"u2"}"#).await;
        let t = login_token(&app, "admin", PW).await;
        mk_device_group(&app, &t, "Servers").await;
        let g = guid_of(&app, &t, "100").await;
        let (s, b) = send(
            &app,
            "PATCH",
            &format!("/api/devices/{g}"),
            Some(&t),
            Some(json!({"device_name": "Renamed", "device_username": "alice",
                "device_group_name": "Servers", "user_name": "admin", "note": "n1"})),
        )
        .await;
        assert_eq!(s, StatusCode::OK, "{b}");
        assert_eq!(b, json!({}));

        let (_, b) = get(&app, "/api/devices?device_group_name=Servers", Some(&t)).await;
        assert_eq!(b["total"], 1, "{b}");
        let d = &b["data"][0];
        assert_eq!(d["id"], "100");
        assert_eq!(d["info"]["device_name"], "Renamed");
        assert_eq!(d["info"]["username"], "alice");
        assert_eq!(d["info"]["os"], "linux");
        assert_eq!(d["device_group_name"], "Servers");
        assert_eq!(d["group_name"], "Default");
        assert_eq!(d["user_name"], "admin");
        assert_eq!(d["note"], "n1");

        let (_, b) = get(&app, "/api/devices?group_name=Default", Some(&t)).await;
        assert_eq!(b["total"], 1);
        let (_, b) = get(&app, "/api/devices?device_group_name=Nope", Some(&t)).await;
        assert_eq!(b["total"], 0);
        let (_, b) = get(&app, "/api/devices?device_name=Ren%25", Some(&t)).await;
        assert_eq!(b["total"], 1);
        let (_, b) = get(&app, "/api/devices?id=200", Some(&t)).await;
        let d = &b["data"][0];
        assert_eq!(d["device_group_name"], "");
        assert_eq!(d["group_name"], "");
        assert_eq!(d["info"]["device_name"], "other");
        assert_eq!(d["info"]["username"], "u2");

        // Empty strings clear the overrides, the group and the assignment.
        send(
            &app,
            "PATCH",
            &format!("/api/devices/{g}"),
            Some(&t),
            Some(json!({"device_name": "", "device_username": "", "device_group_name": "",
                "user_name": ""})),
        )
        .await;
        let (_, b) = get(&app, "/api/devices?id=100", Some(&t)).await;
        let d = &b["data"][0];
        assert_eq!(d["info"]["device_name"], "box");
        assert_eq!(d["info"]["username"], "u1");
        assert_eq!(d["device_group_name"], "");
        assert_eq!(d["user_name"], "");

        let (s, b) = send(
            &app,
            "PATCH",
            &format!("/api/devices/{g}"),
            Some(&t),
            Some(json!({"device_group_name": "Nope"})),
        )
        .await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
        assert_eq!(b, json!({"error": "Unknown device group: Nope"}));
        let (s, _) = send(
            &app,
            "PATCH",
            &format!("/api/devices/{}", "0".repeat(32)),
            Some(&t),
            Some(json!({"note": "x"})),
        )
        .await;
        assert_eq!(s, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn enable_disable_device_sets_status() {
        let (app, db) = test_app_with_db().await;
        insert_user(&db, "admin", PW, true).await;
        insert_user(&db, "bob", PW, false).await;
        insert_peer(&db, "100", b"a").await;
        let t = login_token(&app, "admin", PW).await;
        let bob = login_token(&app, "bob", PW).await;
        let g = guid_of(&app, &t, "100").await;
        let disable = format!("/api/devices/{g}/disable");
        let enable = format!("/api/devices/{g}/enable");

        let (s, b) = send(&app, "POST", &disable, Some(&t), None).await;
        assert_eq!(s, StatusCode::OK, "{b}");
        assert_eq!(b, json!({}));
        assert_eq!(peer_col(&db, "100", "status").await.as_deref(), Some("0"));
        let (_, b) = get(&app, "/api/devices?id=100", Some(&t)).await;
        assert_eq!(b["data"][0]["status"], 0);

        let (s, _) = send(&app, "POST", &enable, Some(&t), None).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(peer_col(&db, "100", "status").await.as_deref(), Some("1"));

        let (s, _) = send(&app, "POST", &disable, Some(&bob), None).await;
        assert_eq!(s, StatusCode::FORBIDDEN);
        assert_eq!(peer_col(&db, "100", "status").await.as_deref(), Some("1"));
        let (s, _) = send(&app, "POST", "/api/devices/zz/disable", Some(&t), None).await;
        assert_eq!(s, StatusCode::NOT_FOUND);
        let missing = format!("/api/devices/{}/enable", "0".repeat(32));
        let (s, _) = send(&app, "POST", &missing, Some(&t), None).await;
        assert_eq!(s, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn delete_device_requires_disabled_and_removes_everywhere() {
        let (app, db, pm) = test_app_with_db_pm().await;
        insert_user(&db, "admin", PW, true).await;
        insert_peer(&db, "100", b"a").await;
        insert_peer(&db, "200", b"b").await;
        set_sysinfo(&db, "100", r#"{"hostname":"box"}"#).await;
        let t = login_token(&app, "admin", PW).await;
        let g = guid_of(&app, &t, "100").await;
        let uri = format!("/api/devices/{g}");
        send(&app, "PATCH", &uri, Some(&t), Some(json!({"device_name": "X"}))).await;
        pm.get_or("100").await;
        assert!(pm.get_in_memory("100").await.is_some());

        let (s, b) = send(&app, "DELETE", &uri, Some(&t), None).await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
        assert_eq!(b, json!({"error": "Disable the device first"}));
        assert!(pm.get_in_memory("100").await.is_some());

        send(&app, "POST", &format!("{uri}/disable"), Some(&t), None).await;
        let (s, b) = send(&app, "DELETE", &uri, Some(&t), None).await;
        assert_eq!(s, StatusCode::OK, "{b}");
        assert_eq!(b, json!({}));
        for (sql, want) in [
            ("select count(*) from peer where id = '100'", 0),
            ("select count(*) from peer", 1),
            ("select count(*) from console_peer_sysinfo", 0),
            ("select count(*) from console_peer_ext", 0),
        ] {
            let n: i64 = sqlx::query_scalar(sql).fetch_one(&db).await.unwrap();
            assert_eq!(n, want, "{sql}");
        }
        assert!(pm.get_in_memory("100").await.is_none());

        let (s, _) = send(&app, "DELETE", &uri, Some(&t), None).await;
        assert_eq!(s, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn assign_user_and_group_and_reject_unsupported() {
        let (app, db) = test_app_with_db().await;
        insert_user(&db, "admin", PW, true).await;
        insert_user(&db, "bob", PW, false).await;
        insert_peer(&db, "100", b"a").await;
        let t = login_token(&app, "admin", PW).await;
        mk_device_group(&app, &t, "Servers").await;
        let g = guid_of(&app, &t, "100").await;
        let uri = format!("/api/devices/{g}/assign");
        let assign = |kind: &str, value: &str| {
            let body = json!({"type": kind, "value": value});
            let (app, uri, t) = (app.clone(), uri.clone(), t.clone());
            async move { send(&app, "POST", &uri, Some(&t), Some(body)).await }
        };

        let (s, b) = assign("user_name", "bob").await;
        assert_eq!(s, StatusCode::OK, "{b}");
        assert_eq!(b, json!({}));
        assert_eq!(peer_user_name(&db, "100").await.as_deref(), Some("bob"));
        let (s, _) = assign("device_group_name", "Servers").await;
        assert_eq!(s, StatusCode::OK);
        assign("device_name", "PC").await;
        assign("device_username", "alice").await;
        assign("note", "hello").await;
        assert_eq!(
            ext_of(&db, "100").await,
            (
                Some("Servers".to_owned()),
                Some("PC".to_owned()),
                Some("alice".to_owned())
            )
        );
        assert_eq!(peer_col(&db, "100", "note").await.as_deref(), Some("hello"));

        let (s, b) = assign("user_name", "nobody").await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
        assert_eq!(b, json!({"error": "Unknown user: nobody"}));
        assert_eq!(peer_user_name(&db, "100").await.as_deref(), Some("bob"));
        for kind in ["strategy_name", "ab"] {
            let (s, b) = assign(kind, "x").await;
            assert_eq!(s, StatusCode::BAD_REQUEST);
            assert_eq!(b, json!({"error": "Not supported yet"}));
        }
        let (s, _) = assign("bogus", "x").await;
        assert_eq!(s, StatusCode::BAD_REQUEST);

        let (s, _) = assign("user_name", "").await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(peer_col(&db, "100", "user").await, None);

        let bob = login_token(&app, "bob", PW).await;
        let body = json!({"type": "note", "value": "x"});
        let (s, _) = send(&app, "POST", &uri, Some(&bob), Some(body)).await;
        assert_eq!(s, StatusCode::FORBIDDEN);
    }

    async fn cli(app: &Router, token: Option<&str>, body: Value) -> (StatusCode, String) {
        let mut b = Request::builder().method("POST").uri("/api/devices/cli");
        if let Some(t) = token {
            b = b.header("Authorization", format!("Bearer {t}"));
        }
        let res = app
            .clone()
            .oneshot(b.body(Body::from(body.to_string())).unwrap())
            .await
            .unwrap();
        let status = res.status();
        (status, body_string(res).await)
    }

    #[tokio::test]
    async fn cli_assign_with_matching_uuid_applies() {
        let (app, db) = test_app_with_db().await;
        insert_user(&db, "admin", PW, true).await;
        insert_user(&db, "bob", PW, false).await;
        insert_peer(&db, "100", b"uuid-100").await;
        let t = login_token(&app, "admin", PW).await;
        mk_device_group(&app, &t, "Servers").await;
        let uuid = base64::encode(b"uuid-100");

        let (s, body) = cli(
            &app,
            Some(&t),
            json!({"id": "100", "uuid": uuid, "user_name": "bob", "note": "n",
                "device_group_name": "Servers", "device_name": "PC", "device_username": "al"}),
        )
        .await;
        assert_eq!(s, StatusCode::OK, "{body}");
        assert_eq!(body, "");
        assert_eq!(peer_user_name(&db, "100").await.as_deref(), Some("bob"));
        assert_eq!(peer_col(&db, "100", "note").await.as_deref(), Some("n"));
        assert_eq!(
            ext_of(&db, "100").await,
            (
                Some("Servers".to_owned()),
                Some("PC".to_owned()),
                Some("al".to_owned())
            )
        );

        let (s, body) = cli(
            &app,
            Some(&t),
            json!({"id": "100", "uuid": uuid, "note": "n2", "strategy_name": "s",
                "address_book_name": "ab", "address_book_tag": "t"}),
        )
        .await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(
            body,
            "Applied; not supported yet: strategy_name, address_book_name"
        );
        assert_eq!(peer_col(&db, "100", "note").await.as_deref(), Some("n2"));
        let (_, body) = cli(
            &app,
            Some(&t),
            json!({"id": "100", "uuid": uuid, "address_book_name": "ab"}),
        )
        .await;
        assert_eq!(body, "Applied; not supported yet: address_book_name");

        let (s, body) = cli(
            &app,
            Some(&t),
            json!({"id": "100", "uuid": uuid, "user_name": "nobody", "note": "n3"}),
        )
        .await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
        assert_eq!(body, r#"{"error":"Unknown user: nobody"}"#);
        assert_eq!(peer_col(&db, "100", "note").await.as_deref(), Some("n2"));
    }

    #[tokio::test]
    async fn cli_wrong_uuid_changes_nothing() {
        let (app, db) = test_app_with_db().await;
        insert_user(&db, "admin", PW, true).await;
        insert_user(&db, "bob", PW, false).await;
        insert_peer(&db, "100", b"uuid-100").await;
        let t = login_token(&app, "admin", PW).await;
        for (id, uuid) in [
            ("100", base64::encode(b"other")),
            ("100", "not base64!".to_owned()),
            ("999", base64::encode(b"uuid-100")),
        ] {
            let (s, body) = cli(
                &app,
                Some(&t),
                json!({"id": id, "uuid": uuid, "user_name": "bob", "note": "n"}),
            )
            .await;
            assert_eq!(s, StatusCode::OK);
            assert_eq!(body, "Device not found");
        }
        assert_eq!(peer_col(&db, "100", "user").await, None);
        assert_eq!(peer_col(&db, "100", "note").await, None);
    }

    #[tokio::test]
    async fn cli_requires_admin_token() {
        let (app, db) = test_app_with_db().await;
        insert_user(&db, "admin", PW, true).await;
        insert_user(&db, "bob", PW, false).await;
        insert_peer(&db, "100", b"uuid-100").await;
        let bob = login_token(&app, "bob", PW).await;
        let body = json!({"id": "100", "uuid": base64::encode(b"uuid-100"), "note": "n"});
        let (s, _) = cli(&app, None, body.clone()).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED);
        let (s, _) = cli(&app, Some("deadbeef"), body.clone()).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED);
        let (s, _) = cli(&app, Some(&bob), body).await;
        assert_eq!(s, StatusCode::FORBIDDEN);
        assert_eq!(peer_col(&db, "100", "note").await, None);
    }

    #[tokio::test]
    async fn deploy_returns_not_enabled() {
        let (app, _db) = test_app_with_db().await;
        let (s, b) = send(
            &app,
            "POST",
            "/api/devices/deploy",
            None,
            Some(json!({"id": "100", "uuid": "eA==", "pk": "eA=="})),
        )
        .await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(b, json!({"result": "NOT_ENABLED"}));
        let (s, b) = send(&app, "POST", "/api/devices/deploy", Some("x"), None).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(b, json!({"result": "NOT_ENABLED"}));
    }
}
