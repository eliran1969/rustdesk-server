//! `/api/device-groups`: device group management with access rules (admin only) and
//! `/api/device-group/accessible`. Compatible with `res/device-groups.py`.

use super::{devices::parse_page, users::parse_guid, JsonBody};
use crate::console::{
    auth::{AdminUser, AuthUser},
    db::{
        access,
        groups::{self, GroupError, GroupKind},
        rules::{self, Rule, RuleError},
    },
    error::ApiError,
    state::ConsoleState,
};
use axum::extract::{Extension, Path, Query};
use axum::Json;
use serde_derive::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::SqliteConnection;
use std::collections::HashMap;

const OWNER: &str = "device_group";

fn internal(e: impl std::fmt::Display) -> ApiError {
    ApiError::Internal(e.to_string())
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn group_err(e: GroupError) -> ApiError {
    match e {
        GroupError::DuplicateName => ApiError::BadRequest("Group already exists".to_owned()),
        GroupError::NotFound => ApiError::NotFound,
        GroupError::Db(e) => internal(e),
    }
}

fn rule_err(e: RuleError) -> ApiError {
    match e {
        RuleError::Unknown(n) => ApiError::BadRequest(format!("Unknown group or user: {n}")),
        RuleError::InvalidType(t) => ApiError::BadRequest(format!("Invalid rule type: {t}")),
        RuleError::Db(e) => internal(e),
    }
}

fn dup_or_db(e: sqlx::Error) -> ApiError {
    let dup =
        matches!(&e, sqlx::Error::Database(d) if d.message().contains("UNIQUE constraint failed"));
    if dup {
        group_err(GroupError::DuplicateName)
    } else {
        internal(e)
    }
}

#[derive(Deserialize, Serialize)]
pub(crate) struct RuleJson {
    #[serde(rename = "type")]
    kind: i64,
    name: String,
}

fn to_rules(v: &[RuleJson]) -> Vec<Rule> {
    v.iter()
        .map(|r| Rule {
            target_type: r.kind,
            name: r.name.clone(),
        })
        .collect()
}

fn rules_json(v: Vec<Rule>) -> Vec<RuleJson> {
    v.into_iter()
        .map(|r| RuleJson {
            kind: r.target_type,
            name: r.name,
        })
        .collect()
}

async fn row_json(
    state: &ConsoleState,
    guid: &[u8],
    name: &str,
    note: &str,
) -> Result<Value, ApiError> {
    let created: Option<String> = sqlx::query_scalar(
        "select strftime('%Y-%m-%dT%H:%M:%S', created_at) from console_device_group where guid = ?",
    )
    .bind(guid)
    .fetch_one(&state.db)
    .await
    .map_err(internal)?;
    let count: i64 =
        sqlx::query_scalar("select count(*) from console_peer_ext where device_group_guid = ?")
            .bind(guid)
            .fetch_one(&state.db)
            .await
            .map_err(internal)?;
    let incoming = rules::get(&state.db, OWNER, guid, "in")
        .await
        .map_err(internal)?;
    Ok(json!({
        "guid": hex(guid),
        "name": name,
        "note": note,
        "created_at": created.unwrap_or_default(),
        "allowed_incomings": rules_json(incoming),
        "device_count": count,
    }))
}

/// `GET /api/device-groups`: `name` is a LIKE pattern bound as sent.
pub(crate) async fn list(
    Extension(state): Extension<ConsoleState>,
    AdminUser(_): AdminUser,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let (current, page_size) = parse_page(&params);
    let name = params
        .get("name")
        .map(String::as_str)
        .filter(|v| !v.is_empty());
    let (total, rows) = groups::list(&state.db, GroupKind::Device, name, current, page_size)
        .await
        .map_err(group_err)?;
    let mut data = Vec::with_capacity(rows.len());
    for g in &rows {
        data.push(row_json(&state, &g.guid, &g.name, &g.note).await?);
    }
    Ok(Json(json!({ "total": total, "data": data })))
}

async fn replace_incoming(
    conn: &mut SqliteConnection,
    guid: &[u8],
    incoming: Option<&[RuleJson]>,
) -> Result<(), ApiError> {
    if let Some(r) = incoming {
        rules::replace(&mut *conn, OWNER, guid, "in", &to_rules(r))
            .await
            .map_err(rule_err)?;
    }
    Ok(())
}

#[derive(Deserialize)]
pub(crate) struct CreateRequest {
    name: String,
    note: Option<String>,
    allowed_incomings: Option<Vec<RuleJson>>,
}

/// `POST /api/device-groups`: group and rules are written in one transaction.
pub(crate) async fn create(
    Extension(state): Extension<ConsoleState>,
    AdminUser(_): AdminUser,
    JsonBody(req): JsonBody<CreateRequest>,
) -> Result<Json<Value>, ApiError> {
    let name = req.name.trim();
    if name.is_empty() {
        return Err(ApiError::BadRequest("Name is required".to_owned()));
    }
    let guid = uuid::Uuid::new_v4().as_bytes().to_vec();
    let mut tx = state.db.begin().await.map_err(internal)?;
    sqlx::query("insert into console_device_group(guid, name, note) values(?, ?, ?)")
        .bind(&guid)
        .bind(name)
        .bind(req.note.as_deref().unwrap_or(""))
        .execute(&mut tx)
        .await
        .map_err(dup_or_db)?;
    replace_incoming(&mut tx, &guid, req.allowed_incomings.as_deref()).await?;
    tx.commit().await.map_err(internal)?;
    Ok(Json(json!({})))
}

#[derive(Deserialize)]
pub(crate) struct UpdateRequest {
    name: Option<String>,
    note: Option<String>,
    allowed_incomings: Option<Vec<RuleJson>>,
}

/// `PATCH /api/device-groups/{guid}`: sent fields replace, unsent fields stay.
pub(crate) async fn update(
    Extension(state): Extension<ConsoleState>,
    AdminUser(_): AdminUser,
    Path(guid): Path<String>,
    JsonBody(req): JsonBody<UpdateRequest>,
) -> Result<Json<Value>, ApiError> {
    let guid = parse_guid(&guid)?;
    groups::find_by_guid(&state.db, GroupKind::Device, &guid)
        .await
        .map_err(group_err)?
        .ok_or(ApiError::NotFound)?;
    let name = req.name.as_deref().map(str::trim);
    if name.is_some_and(str::is_empty) {
        return Err(ApiError::BadRequest("Name is required".to_owned()));
    }
    let mut tx = state.db.begin().await.map_err(internal)?;
    sqlx::query(
        "update console_device_group set name = coalesce(?, name), note = coalesce(?, note)
         where guid = ?",
    )
    .bind(name)
    .bind(req.note.as_deref())
    .bind(&guid)
    .execute(&mut tx)
    .await
    .map_err(dup_or_db)?;
    replace_incoming(&mut tx, &guid, req.allowed_incomings.as_deref()).await?;
    tx.commit().await.map_err(internal)?;
    Ok(Json(json!({})))
}

async fn ensure_exists(conn: &mut SqliteConnection, guid: &[u8]) -> Result<(), ApiError> {
    let exists: i64 =
        sqlx::query_scalar("select count(*) from console_device_group where guid = ?")
            .bind(guid)
            .fetch_one(conn)
            .await
            .map_err(internal)?;
    if exists == 0 {
        return Err(ApiError::NotFound);
    }
    Ok(())
}

/// `DELETE /api/device-groups/{guid}`: unassigns its devices and removes rules the group
/// owns or is the target of.
pub(crate) async fn delete(
    Extension(state): Extension<ConsoleState>,
    AdminUser(_): AdminUser,
    Path(guid): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let guid = parse_guid(&guid)?;
    let mut tx = state.db.begin().await.map_err(internal)?;
    ensure_exists(&mut tx, &guid).await?;
    sqlx::query("update console_peer_ext set device_group_guid = null where device_group_guid = ?")
        .bind(&guid)
        .execute(&mut tx)
        .await
        .map_err(internal)?;
    sqlx::query(
        "delete from console_access_rule where (owner_kind = 'device_group' and owner_guid = ?1)
            or (target_type = 1 and target_guid = ?1)",
    )
    .bind(&guid)
    .execute(&mut tx)
    .await
    .map_err(internal)?;
    sqlx::query("delete from console_device_group where guid = ?")
        .bind(&guid)
        .execute(&mut tx)
        .await
        .map_err(internal)?;
    tx.commit().await.map_err(internal)?;
    Ok(Json(json!({})))
}

/// `POST /api/device-groups/{guid}`: body is an array of device IDs (`peer.id`) to move
/// into the group, replacing any previous group. All-or-nothing.
pub(crate) async fn add_devices(
    Extension(state): Extension<ConsoleState>,
    AdminUser(_): AdminUser,
    Path(guid): Path<String>,
    JsonBody(ids): JsonBody<Vec<String>>,
) -> Result<Json<Value>, ApiError> {
    let guid = parse_guid(&guid)?;
    let mut tx = state.db.begin().await.map_err(internal)?;
    ensure_exists(&mut tx, &guid).await?;
    for id in &ids {
        let peer: Option<Vec<u8>> = sqlx::query_scalar("select guid from peer where id = ?")
            .bind(id)
            .fetch_optional(&mut tx)
            .await
            .map_err(internal)?;
        let peer = peer.ok_or_else(|| ApiError::BadRequest(format!("Unknown device: {id}")))?;
        sqlx::query(
            "insert into console_peer_ext(peer_guid, device_group_guid) values(?1, ?2)
             on conflict(peer_guid) do update set device_group_guid = excluded.device_group_guid",
        )
        .bind(&peer)
        .bind(&guid)
        .execute(&mut tx)
        .await
        .map_err(internal)?;
    }
    tx.commit().await.map_err(internal)?;
    Ok(Json(json!({})))
}

/// `DELETE /api/device-groups/{guid}/devices`: body is an array of device IDs; only
/// devices currently in this group are unassigned, unknown IDs are ignored.
pub(crate) async fn remove_devices(
    Extension(state): Extension<ConsoleState>,
    AdminUser(_): AdminUser,
    Path(guid): Path<String>,
    JsonBody(ids): JsonBody<Vec<String>>,
) -> Result<Json<Value>, ApiError> {
    let guid = parse_guid(&guid)?;
    let mut tx = state.db.begin().await.map_err(internal)?;
    ensure_exists(&mut tx, &guid).await?;
    for id in &ids {
        sqlx::query(
            "update console_peer_ext set device_group_guid = null
             where device_group_guid = ?1
               and peer_guid in (select guid from peer where id = ?2)",
        )
        .bind(&guid)
        .bind(id)
        .execute(&mut tx)
        .await
        .map_err(internal)?;
    }
    tx.commit().await.map_err(internal)?;
    Ok(Json(json!({})))
}

/// `GET /api/device-group/accessible`: admins see all device groups, others those their
/// rules reach. Rows are `{guid, name}`.
pub(crate) async fn accessible(
    Extension(state): Extension<ConsoleState>,
    AuthUser(user, _): AuthUser,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let (current, page_size) = parse_page(&params);
    let visible = access::accessible_device_group_guids(&state.db, &user)
        .await
        .map_err(internal)?;
    let all = groups::list(&state.db, GroupKind::Device, None, 1, i64::MAX)
        .await
        .map_err(group_err)?
        .1;
    let rows: Vec<_> = all
        .into_iter()
        .filter(|g| match &visible {
            Some(v) => v.contains(&g.guid),
            None => true,
        })
        .collect();
    let total = rows.len();
    let skip = usize::try_from((current - 1).saturating_mul(page_size)).unwrap_or(usize::MAX);
    let take = usize::try_from(page_size).unwrap_or(usize::MAX);
    let data: Vec<Value> = rows
        .iter()
        .skip(skip)
        .take(take)
        .map(|g| json!({ "guid": hex(&g.guid), "name": g.name }))
        .collect();
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
    use serde_json::{json, Value};
    use sqlx::SqlitePool;
    use tower::ServiceExt;

    const PW: &str = "correct-horse";

    async fn send(
        app: &Router,
        method: &str,
        uri: &str,
        token: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let b = Request::builder()
            .method(method)
            .uri(uri)
            .header("Authorization", format!("Bearer {token}"));
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

    async fn find(app: &Router, token: &str, name: &str) -> Value {
        let (_, l) = send(
            app,
            "GET",
            &format!("/api/device-groups?name={name}"),
            token,
            None,
        )
        .await;
        l["data"][0].clone()
    }

    fn uri_of(g: &Value) -> String {
        format!("/api/device-groups/{}", g["guid"].as_str().unwrap())
    }

    async fn peer_group(db: &SqlitePool, id: &str) -> Option<Vec<u8>> {
        sqlx::query_scalar(
            "select e.device_group_guid from console_peer_ext e join peer p on p.guid = e.peer_guid
             where p.id = ?",
        )
        .bind(id)
        .fetch_optional(db)
        .await
        .unwrap()
        .flatten()
    }

    async fn mk(app: &Router, t: &str, body: Value) -> (StatusCode, Value) {
        send(app, "POST", "/api/device-groups", t, Some(body)).await
    }

    #[tokio::test]
    async fn device_groups_crud() {
        let (app, db) = test_app_with_db().await;
        insert_user(&db, "admin", PW, true).await;
        insert_user(&db, "bob", PW, false).await;
        let t = login_token(&app, "admin", PW).await;
        let bob_t = login_token(&app, "bob", PW).await;
        let (s, _) = send(&app, "GET", "/api/device-groups", &bob_t, None).await;
        assert_eq!(s, StatusCode::FORBIDDEN);

        mk(&app, &t, json!({"name": "Ops"})).await;
        let (s, b) = mk(
            &app,
            &t,
            json!({"name": "Servers", "note": "n",
                "allowed_incomings": [{"type": 0, "name": "Default"}, {"type": 2, "name": "bob"}]}),
        )
        .await;
        assert_eq!(s, StatusCode::OK, "{b}");
        assert_eq!(b, json!({}));
        let (s, b) = mk(&app, &t, json!({"name": "servers"})).await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
        assert_eq!(b, json!({"error": "Group already exists"}));
        let (s, b) = mk(
            &app,
            &t,
            json!({"name": "X", "allowed_incomings": [{"type": 0, "name": "Nope"}]}),
        )
        .await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
        assert_eq!(b, json!({"error": "Unknown group or user: Nope"}));
        assert_eq!(find(&app, &t, "X").await, Value::Null);
        let (s, b) = mk(
            &app,
            &t,
            json!({"name": "X", "allowed_incomings": [{"type": 1, "name": "Ops"}]}),
        )
        .await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
        assert_eq!(b, json!({"error": "Invalid rule type: 1"}));

        let g = find(&app, &t, "Servers").await;
        assert_eq!(g["note"], "n");
        assert_eq!(g["device_count"], 0);
        assert_eq!(g["created_at"].as_str().unwrap().len(), 19);
        assert_eq!(g["allowed_incomings"].as_array().unwrap().len(), 2);
        assert!(g.get("allowed_outgoings").is_none());
        let (_, all) = send(
            &app,
            "GET",
            "/api/device-groups?current=1&pageSize=1",
            &t,
            None,
        )
        .await;
        assert_eq!(all["total"], 2);
        assert_eq!(all["data"].as_array().unwrap().len(), 1);

        let uri = uri_of(&g);
        let (s, b) = send(&app, "PATCH", &uri, &t, Some(json!({"name": "Srv2"}))).await;
        assert_eq!(s, StatusCode::OK, "{b}");
        let g = find(&app, &t, "Srv2").await;
        assert_eq!(g["note"], "n");
        assert_eq!(g["allowed_incomings"].as_array().unwrap().len(), 2);
        let (s, _) = send(&app, "DELETE", &uri, &t, None).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(find(&app, &t, "Srv2").await, Value::Null);
        let (s, _) = send(&app, "PATCH", &uri, &t, Some(json!({"note": "x"}))).await;
        assert_eq!(s, StatusCode::NOT_FOUND);
        let (s, _) = send(&app, "DELETE", &uri, &t, None).await;
        assert_eq!(s, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn add_devices_replaces_previous_group_atomically() {
        let (app, db) = test_app_with_db().await;
        insert_user(&db, "admin", PW, true).await;
        let t = login_token(&app, "admin", PW).await;
        insert_peer(&db, "111", b"u1").await;
        insert_peer(&db, "222", b"u2").await;
        mk(&app, &t, json!({"name": "A"})).await;
        mk(&app, &t, json!({"name": "B"})).await;
        let a = find(&app, &t, "A").await;
        let b = find(&app, &t, "B").await;
        let (s, r) = send(&app, "POST", &uri_of(&a), &t, Some(json!(["111", "222"]))).await;
        assert_eq!(s, StatusCode::OK, "{r}");
        assert_eq!(r, json!({}));
        assert_eq!(find(&app, &t, "A").await["device_count"], 2);
        let (s, _) = send(&app, "POST", &uri_of(&b), &t, Some(json!(["222"]))).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(find(&app, &t, "A").await["device_count"], 1);
        assert_eq!(find(&app, &t, "B").await["device_count"], 1);
        let bg = hex(&peer_group(&db, "222").await.unwrap());
        assert_eq!(bg, b["guid"].as_str().unwrap());
        let (s, _) = send(
            &app,
            "POST",
            &format!("/api/device-groups/{}", "00".repeat(16)),
            &t,
            Some(json!([])),
        )
        .await;
        assert_eq!(s, StatusCode::NOT_FOUND);
    }

    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    #[tokio::test]
    async fn add_unknown_device_changes_nothing() {
        let (app, db) = test_app_with_db().await;
        insert_user(&db, "admin", PW, true).await;
        let t = login_token(&app, "admin", PW).await;
        insert_peer(&db, "111", b"u1").await;
        mk(&app, &t, json!({"name": "A"})).await;
        let a = find(&app, &t, "A").await;
        let (s, r) = send(&app, "POST", &uri_of(&a), &t, Some(json!(["111", "999"]))).await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
        assert_eq!(r, json!({"error": "Unknown device: 999"}));
        assert_eq!(find(&app, &t, "A").await["device_count"], 0);
        assert!(peer_group(&db, "111").await.is_none());
    }

    #[tokio::test]
    async fn remove_devices_only_from_this_group() {
        let (app, db) = test_app_with_db().await;
        insert_user(&db, "admin", PW, true).await;
        let t = login_token(&app, "admin", PW).await;
        insert_peer(&db, "111", b"u1").await;
        insert_peer(&db, "222", b"u2").await;
        mk(&app, &t, json!({"name": "A"})).await;
        mk(&app, &t, json!({"name": "B"})).await;
        let a = find(&app, &t, "A").await;
        let b = find(&app, &t, "B").await;
        send(&app, "POST", &uri_of(&a), &t, Some(json!(["111"]))).await;
        send(&app, "POST", &uri_of(&b), &t, Some(json!(["222"]))).await;
        let uri = format!("{}/devices", uri_of(&a));
        let (s, r) = send(
            &app,
            "DELETE",
            &uri,
            &t,
            Some(json!(["111", "222", "nope"])),
        )
        .await;
        assert_eq!(s, StatusCode::OK, "{r}");
        assert_eq!(r, json!({}));
        assert!(peer_group(&db, "111").await.is_none());
        assert!(peer_group(&db, "222").await.is_some());
        assert_eq!(find(&app, &t, "A").await["device_count"], 0);
        assert_eq!(find(&app, &t, "B").await["device_count"], 1);
    }

    #[tokio::test]
    async fn delete_group_unassigns_devices() {
        let (app, db) = test_app_with_db().await;
        insert_user(&db, "admin", PW, true).await;
        let t = login_token(&app, "admin", PW).await;
        insert_peer(&db, "111", b"u1").await;
        mk(
            &app,
            &t,
            json!({"name": "A", "allowed_incomings": [{"type": 0, "name": "Default"}]}),
        )
        .await;
        let a = find(&app, &t, "A").await;
        send(&app, "POST", &uri_of(&a), &t, Some(json!(["111"]))).await;
        // A user group reaching A through an `out` rule.
        let (s, r) = send(
            &app,
            "POST",
            "/api/user-groups",
            &t,
            Some(json!({"name": "U", "allowed_outgoings": [{"type": 1, "name": "A"}]})),
        )
        .await;
        assert_eq!(s, StatusCode::OK, "{r}");
        let (s, r) = send(&app, "DELETE", &uri_of(&a), &t, None).await;
        assert_eq!(s, StatusCode::OK, "{r}");
        assert!(peer_group(&db, "111").await.is_none());
        let ext: i64 = sqlx::query_scalar("select count(*) from console_peer_ext")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(ext, 1, "peer ext row stays, only unassigned");
        let rules: i64 = sqlx::query_scalar("select count(*) from console_access_rule")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(rules, 0);
    }

    #[tokio::test]
    async fn accessible_device_groups_by_rules() {
        let (app, db) = test_app_with_db().await;
        insert_user(&db, "admin", PW, true).await;
        insert_user(&db, "bob", PW, false).await;
        insert_user(&db, "eve", PW, false).await;
        let t = login_token(&app, "admin", PW).await;
        let bob_t = login_token(&app, "bob", PW).await;
        let eve_t = login_token(&app, "eve", PW).await;
        mk(&app, &t, json!({"name": "ViaOut"})).await;
        send(
            &app,
            "POST",
            "/api/user-groups",
            &t,
            Some(json!({"name": "Ops", "allowed_outgoings": [{"type": 1, "name": "ViaOut"}]})),
        )
        .await;
        mk(
            &app,
            &t,
            json!({"name": "ByGroup", "allowed_incomings": [{"type": 0, "name": "Default"}]}),
        )
        .await;
        mk(
            &app,
            &t,
            json!({"name": "ByUser", "allowed_incomings": [{"type": 2, "name": "bob"}]}),
        )
        .await;
        mk(&app, &t, json!({"name": "Hidden"})).await;
        // Move eve into Ops so she reaches ViaOut; bob stays in Default.
        let (_, ug) = send(&app, "GET", "/api/user-groups?name=Ops", &t, None).await;
        let ops = ug["data"][0]["guid"].as_str().unwrap().to_owned();
        let eve_guid: Vec<u8> =
            sqlx::query_scalar("select guid from console_user where name = 'eve'")
                .fetch_one(&db)
                .await
                .unwrap();
        send(
            &app,
            "POST",
            &format!("/api/user-groups/{ops}"),
            &t,
            Some(json!([hex(&eve_guid)])),
        )
        .await;

        let names = |v: &Value| -> Vec<String> {
            v["data"]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| r["name"].as_str().unwrap().to_owned())
                .collect()
        };
        let uri = "/api/device-group/accessible?current=1&pageSize=100";
        let (s, all) = send(&app, "GET", uri, &t, None).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(all["total"], 4);
        assert!(all["data"][0]["guid"].as_str().unwrap().len() == 32);
        let (_, b) = send(&app, "GET", uri, &bob_t, None).await;
        assert_eq!(names(&b), vec!["ByGroup", "ByUser"]);
        assert_eq!(b["total"], 2);
        let (_, e) = send(&app, "GET", uri, &eve_t, None).await;
        assert_eq!(names(&e), vec!["ViaOut"]);
        let (_, p) = send(
            &app,
            "GET",
            "/api/device-group/accessible?current=2&pageSize=1",
            &bob_t,
            None,
        )
        .await;
        assert_eq!(p["total"], 2);
        assert_eq!(names(&p), vec!["ByUser"]);
    }
}
