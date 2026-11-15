use crate::console::{
    auth::AuthUser, db::devices::is_online, error::ApiError, state::ConsoleState,
};
use axum::{
    extract::{Extension, Query},
    Json,
};
use serde_json::{json, Map, Value};
use sqlx::Row;
use std::collections::HashMap;

const DEFAULT_PAGE_SIZE: i64 = 30;
const MAX_PAGE_SIZE: i64 = 100;

const FROM: &str = "from peer \
    left join console_peer_sysinfo s on s.peer_guid = peer.guid \
    left join console_user u on u.guid = peer.user";

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
/// `device_name` taken from `hostname`. The sysinfo `uuid` is never exposed.
fn build_info(sysinfo: Option<&str>) -> Value {
    let mut info = match sysinfo.and_then(|s| serde_json::from_str::<Value>(s).ok()) {
        Some(Value::Object(m)) => m,
        _ => Map::new(),
    };
    info.remove("uuid");
    if let Some(hostname @ Value::String(_)) = info.get("hostname").cloned() {
        info.insert("device_name".to_owned(), hostname);
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

    let mut conds: Vec<&str> = Vec::new();
    if !user.is_admin {
        conds.push("peer.user = ?");
    }
    if id_filter.is_some() {
        conds.push("peer.id like ?");
    }
    if name_filter.is_some() {
        conds.push("json_extract(s.sysinfo, '$.hostname') like ?");
    }
    if user_filter.is_some() {
        conds.push("u.name like ?");
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
         s.sysinfo as sysinfo, cast(coalesce(s.last_online, peer.created_at) as text) as last_online, u.name as user_name \
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
                "note": note.unwrap_or_default(),
                "info": build_info(sysinfo.as_deref()),
            }),
        ));
    }
    // Stable: online devices first within the page, `id` order otherwise.
    data.sort_by_key(|(online, _)| !*online);
    let data: Vec<Value> = data.into_iter().map(|(_, v)| v).collect();

    Ok(Json(json!({ "total": total, "data": data })))
}

#[cfg(test)]
mod tests {
    use crate::console::test_util::{
        body_string, insert_peer, insert_user, login_token, test_app_with_db,
    };
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        Router,
    };
    use serde_json::Value;
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
}
