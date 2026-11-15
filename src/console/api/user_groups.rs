//! `/api/user-groups`: user group management with access rules (admin only).
//! Compatible with `res/user-groups.py`.

use super::{devices::parse_page, users::parse_guid, JsonBody};
use crate::console::{
    auth::AdminUser,
    db::{
        groups::{self, GroupError, GroupKind},
        migrations::DEFAULT_GROUP_NAME,
        rules::{self, Rule, RuleError},
    },
    error::ApiError,
    state::ConsoleState,
};
use axum::extract::{Extension, Path, Query};
use axum::Json;
use serde_derive::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{Row, SqliteConnection};
use std::collections::HashMap;

const OWNER: &str = "user_group";

fn internal(e: impl std::fmt::Display) -> ApiError {
    ApiError::Internal(e.to_string())
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn is_default(name: &str) -> bool {
    name.eq_ignore_ascii_case(DEFAULT_GROUP_NAME)
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
    let dup = matches!(&e, sqlx::Error::Database(d) if d.message().contains("UNIQUE constraint failed"));
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
        "select strftime('%Y-%m-%dT%H:%M:%S', created_at) from console_user_group where guid = ?",
    )
    .bind(guid)
    .fetch_one(&state.db)
    .await
    .map_err(internal)?;
    let count: i64 = sqlx::query_scalar("select count(*) from console_user where group_guid = ?")
        .bind(guid)
        .fetch_one(&state.db)
        .await
        .map_err(internal)?;
    let incoming = rules::get(&state.db, OWNER, guid, "in")
        .await
        .map_err(internal)?;
    let outgoing = rules::get(&state.db, OWNER, guid, "out")
        .await
        .map_err(internal)?;
    Ok(json!({
        "guid": hex(guid),
        "name": name,
        "note": note,
        "created_at": created.unwrap_or_default(),
        "allowed_incomings": rules_json(incoming),
        "allowed_outgoings": rules_json(outgoing),
        "user_count": count,
    }))
}

/// `GET /api/user-groups`: `name` is a LIKE pattern bound as sent.
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
    let (total, rows) = groups::list(&state.db, GroupKind::User, name, current, page_size)
        .await
        .map_err(group_err)?;
    let mut data = Vec::with_capacity(rows.len());
    for g in &rows {
        data.push(row_json(&state, &g.guid, &g.name, &g.note).await?);
    }
    Ok(Json(json!({ "total": total, "data": data })))
}

#[derive(Deserialize)]
pub(crate) struct CreateRequest {
    name: String,
    note: Option<String>,
    allowed_incomings: Option<Vec<RuleJson>>,
    allowed_outgoings: Option<Vec<RuleJson>>,
}

async fn replace_rules(
    conn: &mut SqliteConnection,
    guid: &[u8],
    incoming: Option<&[RuleJson]>,
    outgoing: Option<&[RuleJson]>,
) -> Result<(), ApiError> {
    if let Some(r) = incoming {
        rules::replace(&mut *conn, OWNER, guid, "in", &to_rules(r))
            .await
            .map_err(rule_err)?;
    }
    if let Some(r) = outgoing {
        rules::replace(&mut *conn, OWNER, guid, "out", &to_rules(r))
            .await
            .map_err(rule_err)?;
    }
    Ok(())
}

/// `POST /api/user-groups`: group and rules are written in one transaction.
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
    sqlx::query("insert into console_user_group(guid, name, note) values(?, ?, ?)")
        .bind(&guid)
        .bind(name)
        .bind(req.note.as_deref().unwrap_or(""))
        .execute(&mut tx)
        .await
        .map_err(dup_or_db)?;
    replace_rules(
        &mut tx,
        &guid,
        req.allowed_incomings.as_deref(),
        req.allowed_outgoings.as_deref(),
    )
    .await?;
    tx.commit().await.map_err(internal)?;
    Ok(Json(json!({})))
}

#[derive(Deserialize)]
pub(crate) struct UpdateRequest {
    name: Option<String>,
    note: Option<String>,
    allowed_incomings: Option<Vec<RuleJson>>,
    allowed_outgoings: Option<Vec<RuleJson>>,
}

/// `PATCH /api/user-groups/{guid}`: sent fields replace, unsent fields stay.
pub(crate) async fn update(
    Extension(state): Extension<ConsoleState>,
    AdminUser(_): AdminUser,
    Path(guid): Path<String>,
    JsonBody(req): JsonBody<UpdateRequest>,
) -> Result<Json<Value>, ApiError> {
    let guid = parse_guid(&guid)?;
    let current = groups::find_by_guid(&state.db, GroupKind::User, &guid)
        .await
        .map_err(group_err)?
        .ok_or(ApiError::NotFound)?;
    let name = req.name.as_deref().map(str::trim);
    if let Some(n) = name {
        if n.is_empty() {
            return Err(ApiError::BadRequest("Name is required".to_owned()));
        }
        if is_default(&current.name) && n != current.name && !n.eq_ignore_ascii_case(&current.name)
        {
            return Err(ApiError::BadRequest(
                "Cannot rename the default group".to_owned(),
            ));
        }
    }
    let mut tx = state.db.begin().await.map_err(internal)?;
    sqlx::query(
        "update console_user_group set name = coalesce(?, name), note = coalesce(?, note)
         where guid = ?",
    )
    .bind(name)
    .bind(req.note.as_deref())
    .bind(&guid)
    .execute(&mut tx)
    .await
    .map_err(dup_or_db)?;
    replace_rules(
        &mut tx,
        &guid,
        req.allowed_incomings.as_deref(),
        req.allowed_outgoings.as_deref(),
    )
    .await?;
    tx.commit().await.map_err(internal)?;
    Ok(Json(json!({})))
}

/// `DELETE /api/user-groups/{guid}`: empty, non-default groups only; also removes
/// rules the group owns or is the target of.
pub(crate) async fn delete(
    Extension(state): Extension<ConsoleState>,
    AdminUser(_): AdminUser,
    Path(guid): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let guid = parse_guid(&guid)?;
    let mut tx = state.db.begin().await.map_err(internal)?;
    let row = sqlx::query("select name from console_user_group where guid = ?")
        .bind(&guid)
        .fetch_optional(&mut tx)
        .await
        .map_err(internal)?
        .ok_or(ApiError::NotFound)?;
    let name: String = row.try_get("name").map_err(internal)?;
    if is_default(&name) {
        return Err(ApiError::BadRequest(
            "Cannot delete the default group".to_owned(),
        ));
    }
    let members: i64 = sqlx::query_scalar("select count(*) from console_user where group_guid = ?")
        .bind(&guid)
        .fetch_one(&mut tx)
        .await
        .map_err(internal)?;
    if members > 0 {
        return Err(ApiError::BadRequest("Group is not empty".to_owned()));
    }
    sqlx::query(
        "delete from console_access_rule where (owner_kind = 'user_group' and owner_guid = ?1)
            or (target_type = 0 and target_guid = ?1)",
    )
    .bind(&guid)
    .execute(&mut tx)
    .await
    .map_err(internal)?;
    sqlx::query("delete from console_user_group where guid = ?")
        .bind(&guid)
        .execute(&mut tx)
        .await
        .map_err(internal)?;
    tx.commit().await.map_err(internal)?;
    Ok(Json(json!({})))
}

/// `POST /api/user-groups/{guid}`: body is an array of user guids to move into the group.
pub(crate) async fn add_users(
    Extension(state): Extension<ConsoleState>,
    AdminUser(_): AdminUser,
    Path(guid): Path<String>,
    JsonBody(user_guids): JsonBody<Vec<String>>,
) -> Result<Json<Value>, ApiError> {
    let guid = parse_guid(&guid)?;
    let mut tx = state.db.begin().await.map_err(internal)?;
    let exists: i64 = sqlx::query_scalar("select count(*) from console_user_group where guid = ?")
        .bind(&guid)
        .fetch_one(&mut tx)
        .await
        .map_err(internal)?;
    if exists == 0 {
        return Err(ApiError::NotFound);
    }
    for g in &user_guids {
        let unknown = || ApiError::BadRequest(format!("Unknown user: {g}"));
        let ug = parse_guid(g).map_err(|_| unknown())?;
        let res = sqlx::query("update console_user set group_guid = ? where guid = ?")
            .bind(&guid)
            .bind(&ug)
            .execute(&mut tx)
            .await
            .map_err(internal)?;
        if res.rows_affected() == 0 {
            return Err(unknown());
        }
    }
    tx.commit().await.map_err(internal)?;
    Ok(Json(json!({})))
}

#[cfg(test)]
mod tests {
    use crate::console::{
        db::groups::{self, GroupKind},
        test_util::{body_string, insert_user, login_token, test_app_with_db},
    };
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        Router,
    };
    use serde_json::{json, Value};
    use tower::ServiceExt;

    const PW: &str = "correct-horse";

    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

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
            &format!("/api/user-groups?name={name}"),
            token,
            None,
        )
        .await;
        l["data"][0].clone()
    }

    #[tokio::test]
    async fn user_groups_crud_with_rules() {
        let (app, db) = test_app_with_db().await;
        insert_user(&db, "admin", PW, true).await;
        insert_user(&db, "bob", PW, false).await;
        let t = login_token(&app, "admin", PW).await;
        let bob_t = login_token(&app, "bob", PW).await;
        let (s, _) = send(&app, "GET", "/api/user-groups", &bob_t, None).await;
        assert_eq!(s, StatusCode::FORBIDDEN);

        let (s, b) = send(
            &app,
            "POST",
            "/api/user-groups",
            &t,
            Some(json!({"name": "Ops"})),
        )
        .await;
        assert_eq!(s, StatusCode::OK, "{b}");
        assert_eq!(b, json!({}));
        let (s, b) = send(
            &app,
            "POST",
            "/api/user-groups",
            &t,
            Some(json!({"name": "Sales", "note": "n",
                "allowed_incomings": [{"type": 0, "name": "Ops"}, {"type": 2, "name": "bob"}],
                "allowed_outgoings": [{"type": 0, "name": "Ops"}]})),
        )
        .await;
        assert_eq!(s, StatusCode::OK, "{b}");
        let (s, b) = send(
            &app,
            "POST",
            "/api/user-groups",
            &t,
            Some(json!({"name": "sales"})),
        )
        .await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
        assert_eq!(b, json!({"error": "Group already exists"}));

        let sales = find(&app, &t, "Sales").await;
        assert_eq!(sales["note"], "n");
        assert_eq!(sales["user_count"], 0);
        assert_eq!(sales["created_at"].as_str().unwrap().len(), 19);
        assert_eq!(sales["allowed_incomings"].as_array().unwrap().len(), 2);
        assert_eq!(
            sales["allowed_outgoings"],
            json!([{"type": 0, "name": "Ops"}])
        );
        let (_, all) = send(
            &app,
            "GET",
            "/api/user-groups?current=1&pageSize=2",
            &t,
            None,
        )
        .await;
        assert_eq!(all["total"], 3);
        assert_eq!(all["data"].as_array().unwrap().len(), 2);
        let def = find(&app, &t, "Default").await;
        assert_eq!(def["user_count"], 2);

        let uri = format!("/api/user-groups/{}", sales["guid"].as_str().unwrap());
        let (s, b) = send(&app, "PATCH", &uri, &t, Some(json!({"name": "Sales2"}))).await;
        assert_eq!(s, StatusCode::OK, "{b}");
        assert_eq!(find(&app, &t, "Sales2").await["note"], "n");
        let (s, _) = send(&app, "DELETE", &uri, &t, None).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(find(&app, &t, "Sales2").await, Value::Null);
        let (s, _) = send(&app, "PATCH", &uri, &t, Some(json!({"note": "x"}))).await;
        assert_eq!(s, StatusCode::NOT_FOUND);
        let (s, _) = send(&app, "PATCH", "/api/user-groups/zz", &t, Some(json!({}))).await;
        assert_eq!(s, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn patch_replaces_only_sent_rules() {
        let (app, db) = test_app_with_db().await;
        insert_user(&db, "admin", PW, true).await;
        let t = login_token(&app, "admin", PW).await;
        send(
            &app,
            "POST",
            "/api/user-groups",
            &t,
            Some(json!({"name": "Ops"})),
        )
        .await;
        send(
            &app,
            "POST",
            "/api/user-groups",
            &t,
            Some(json!({"name": "G", "allowed_incomings": [{"type": 0, "name": "Ops"}],
                "allowed_outgoings": [{"type": 0, "name": "Ops"}]})),
        )
        .await;
        let g = find(&app, &t, "G").await;
        let uri = format!("/api/user-groups/{}", g["guid"].as_str().unwrap());
        let (s, b) = send(
            &app,
            "PATCH",
            &uri,
            &t,
            Some(json!({"allowed_incomings": []})),
        )
        .await;
        assert_eq!(s, StatusCode::OK, "{b}");
        let g = find(&app, &t, "G").await;
        assert_eq!(g["allowed_incomings"], json!([]));
        assert_eq!(g["allowed_outgoings"], json!([{"type": 0, "name": "Ops"}]));
        let (s, _) = send(&app, "PATCH", &uri, &t, Some(json!({"note": "z"}))).await;
        assert_eq!(s, StatusCode::OK);
        let g = find(&app, &t, "G").await;
        assert_eq!(g["note"], "z");
        assert_eq!(g["allowed_outgoings"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn delete_non_empty_or_default_rejected() {
        let (app, db) = test_app_with_db().await;
        insert_user(&db, "admin", PW, true).await;
        let bob = insert_user(&db, "bob", PW, false).await;
        let t = login_token(&app, "admin", PW).await;
        let ops = groups::create(&db, GroupKind::User, "Ops", "")
            .await
            .unwrap();
        let other = groups::create(&db, GroupKind::User, "Other", "")
            .await
            .unwrap();
        let default = groups::default_user_group(&db).await.unwrap().guid;
        sqlx::query("update console_user set group_guid = ? where guid = ?")
            .bind(&ops)
            .bind(&bob)
            .execute(&db)
            .await
            .unwrap();
        for (owner, target) in [(&other, &ops), (&ops, &other)] {
            sqlx::query(
                "insert into console_access_rule(owner_kind, owner_guid, direction, target_type, target_guid)
                 values('user_group', ?, 'out', 0, ?)",
            )
            .bind(owner)
            .bind(target)
            .execute(&db)
            .await
            .unwrap();
        }
        let uri = format!("/api/user-groups/{}", hex(&ops));
        let (s, b) = send(&app, "DELETE", &uri, &t, None).await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
        assert_eq!(b, json!({"error": "Group is not empty"}));

        let duri = format!("/api/user-groups/{}", hex(&default));
        let (s, b) = send(&app, "DELETE", &duri, &t, None).await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
        assert_eq!(b, json!({"error": "Cannot delete the default group"}));
        let (s, b) = send(&app, "PATCH", &duri, &t, Some(json!({"name": "Main"}))).await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
        assert_eq!(b, json!({"error": "Cannot rename the default group"}));
        let (s, _) = send(
            &app,
            "PATCH",
            &duri,
            &t,
            Some(json!({"name": "default", "note": "x"})),
        )
        .await;
        assert_eq!(s, StatusCode::OK);

        // Emptied: rules owned by or targeting the group go away with it.
        sqlx::query("update console_user set group_guid = ? where guid = ?")
            .bind(&default)
            .bind(&bob)
            .execute(&db)
            .await
            .unwrap();
        let (s, b) = send(&app, "DELETE", &uri, &t, None).await;
        assert_eq!(s, StatusCode::OK, "{b}");
        let n: i64 = sqlx::query_scalar("select count(*) from console_access_rule")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(n, 0);
        let (s, _) = send(&app, "DELETE", &uri, &t, None).await;
        assert_eq!(s, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn add_users_moves_them() {
        let (app, db) = test_app_with_db().await;
        insert_user(&db, "admin", PW, true).await;
        let bob = insert_user(&db, "bob", PW, false).await;
        let eve = insert_user(&db, "eve", PW, false).await;
        let t = login_token(&app, "admin", PW).await;
        send(
            &app,
            "POST",
            "/api/user-groups",
            &t,
            Some(json!({"name": "Ops"})),
        )
        .await;
        let ops = find(&app, &t, "Ops").await;
        let uri = format!("/api/user-groups/{}", ops["guid"].as_str().unwrap());
        let bad = "00".repeat(16);
        let (s, b) = send(&app, "POST", &uri, &t, Some(json!([hex(&bob), bad]))).await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
        assert_eq!(b, json!({"error": format!("Unknown user: {bad}")}));
        assert_eq!(find(&app, &t, "Ops").await["user_count"], 0);

        let (s, b) = send(&app, "POST", &uri, &t, Some(json!([hex(&bob), hex(&eve)]))).await;
        assert_eq!(s, StatusCode::OK, "{b}");
        assert_eq!(b, json!({}));
        assert_eq!(find(&app, &t, "Ops").await["user_count"], 2);
        let (s, _) = send(
            &app,
            "POST",
            &format!("/api/user-groups/{bad}"),
            &t,
            Some(json!([])),
        )
        .await;
        assert_eq!(s, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn rules_unknown_name_rejected() {
        let (app, db) = test_app_with_db().await;
        insert_user(&db, "admin", PW, true).await;
        let t = login_token(&app, "admin", PW).await;
        let (s, b) = send(
            &app,
            "POST",
            "/api/user-groups",
            &t,
            Some(json!({"name": "G", "allowed_incomings": [{"type": 0, "name": "Nope"}]})),
        )
        .await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
        assert_eq!(b, json!({"error": "Unknown group or user: Nope"}));
        // Rolled back: the group was not created.
        assert_eq!(find(&app, &t, "G").await, Value::Null);
        let (s, b) = send(
            &app,
            "POST",
            "/api/user-groups",
            &t,
            Some(json!({"name": "G", "allowed_incomings": [{"type": 1, "name": "x"}]})),
        )
        .await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
        assert_eq!(b, json!({"error": "Invalid rule type: 1"}));
        assert_eq!(find(&app, &t, "G").await, Value::Null);
    }
}
