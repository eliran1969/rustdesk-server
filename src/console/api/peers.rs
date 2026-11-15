//! `GET /api/peers`: the devices a signed-in user may see, in the client's `PeerPayload`
//! shape (`flutter/lib/common/hbbs/hbbs.dart`), read by `group_model.dart`.

use super::devices::{build_info, parse_page};
use crate::console::{auth::AuthUser, db::access, error::ApiError, state::ConsoleState};
use axum::{
    extract::{Extension, Query},
    Json,
};
use serde_json::{json, Value};
use sqlx::Row;
use std::collections::HashMap;

const FROM: &str = "from peer \
    left join console_peer_sysinfo s on s.peer_guid = peer.guid \
    left join console_user u on u.guid = peer.user \
    left join console_peer_ext e on e.peer_guid = peer.guid \
    left join console_device_group dg on dg.guid = e.device_group_guid";

fn internal(e: impl std::fmt::Display) -> ApiError {
    ApiError::Internal(e.to_string())
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// A JSON array of lowercase hex guids, matched in SQL through `json_each`, so a set of
/// any size is a single bound parameter.
fn hex_set(guids: &[Vec<u8>]) -> String {
    Value::from(guids.iter().map(|g| hex(g)).collect::<Vec<_>>()).to_string()
}

/// `GET /api/peers`: admins see every device; other users see the devices assigned to
/// the users accessible to them (same rules as `/api/users?accessible`) plus the devices
/// in the device groups accessible to them. `status=1` keeps enabled devices only.
pub(crate) async fn list(
    Extension(state): Extension<ConsoleState>,
    AuthUser(user, _): AuthUser,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let (current, page_size) = parse_page(&params);
    let users = access::accessible_user_guids(&state.db, &user)
        .await
        .map_err(internal)?;
    let device_groups = access::accessible_device_group_guids(&state.db, &user)
        .await
        .map_err(internal)?;
    let enabled_only = params.get("status").map(|s| s.trim()) == Some("1");

    let mut conds: Vec<&str> = Vec::new();
    let mut binds: Vec<String> = Vec::new();
    // Fail closed: the restriction is keyed on the caller not being an admin; a missing
    // set for a non-admin is treated as empty, never as "everything".
    if !user.is_admin {
        conds.push(
            "(lower(hex(peer.user)) in (select value from json_each(?)) \
             or lower(hex(e.device_group_guid)) in (select value from json_each(?)))",
        );
        binds.push(hex_set(users.as_deref().unwrap_or_default()));
        binds.push(hex_set(device_groups.as_deref().unwrap_or_default()));
    }
    if enabled_only {
        conds.push("(peer.status is null or peer.status = 1)");
    }
    let where_sql = if conds.is_empty() {
        String::new()
    } else {
        format!("where {}", conds.join(" and "))
    };

    let count_sql = format!("select count(*) {FROM} {where_sql}");
    let mut count_q = sqlx::query_as::<_, (i64,)>(&count_sql);
    for b in &binds {
        count_q = count_q.bind(b);
    }
    let (total,) = count_q.fetch_one(&state.db).await.map_err(internal)?;

    let select_sql = format!(
        "select peer.id as id, peer.status as status, peer.note as note, peer.user as user, \
         u.name as user_name, dg.name as device_group_name, s.sysinfo as sysinfo, \
         e.device_name as device_name, e.device_username as device_username \
         {FROM} {where_sql} order by peer.id limit ? offset ?"
    );
    let mut q = sqlx::query(&select_sql);
    for b in &binds {
        q = q.bind(b);
    }
    let rows = q
        .bind(page_size)
        .bind((current - 1).saturating_mul(page_size))
        .fetch_all(&state.db)
        .await
        .map_err(internal)?;

    let mut data = Vec::with_capacity(rows.len());
    for row in rows {
        let id: String = row.try_get("id").map_err(internal)?;
        let status: Option<i64> = row.try_get("status").map_err(internal)?;
        let note: Option<String> = row.try_get("note").map_err(internal)?;
        let user_guid: Option<Vec<u8>> = row.try_get("user").map_err(internal)?;
        let user_name: Option<String> = row.try_get("user_name").map_err(internal)?;
        let device_group_name: Option<String> =
            row.try_get("device_group_name").map_err(internal)?;
        let sysinfo: Option<String> = row.try_get("sysinfo").map_err(internal)?;
        let device_name: Option<String> = row.try_get("device_name").map_err(internal)?;
        let device_username: Option<String> = row.try_get("device_username").map_err(internal)?;
        data.push(json!({
            "id": id,
            "info": build_info(sysinfo.as_deref(), device_name, device_username),
            "status": status.unwrap_or(1),
            "user": user_guid.as_deref().map(hex).unwrap_or_default(),
            "user_name": user_name.unwrap_or_default(),
            "device_group_name": device_group_name.unwrap_or_default(),
            // A device note is admin-only information.
            "note": if user.is_admin { note.unwrap_or_default() } else { String::new() },
        }));
    }
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

    async fn get(app: &Router, uri: &str, token: &str) -> (StatusCode, Value) {
        let req = Request::builder()
            .method("GET")
            .uri(uri)
            .header("Authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let text = body_string(res).await;
        (
            status,
            serde_json::from_str(&text).unwrap_or(Value::String(text)),
        )
    }

    async fn exec(db: &SqlitePool, sql: &str, binds: &[&[u8]]) {
        let mut q = sqlx::query(sql);
        for b in binds {
            q = q.bind(b.to_vec());
        }
        q.execute(db).await.unwrap();
    }

    async fn assign_user(db: &SqlitePool, id: &str, user: &[u8]) {
        sqlx::query("update peer set user = ? where id = ?")
            .bind(user)
            .bind(id)
            .execute(db)
            .await
            .unwrap();
    }

    async fn put_in_group(db: &SqlitePool, id: &str, group: &[u8]) {
        sqlx::query(
            "insert into console_peer_ext(peer_guid, device_group_guid)
             select guid, ? from peer where id = ?",
        )
        .bind(group)
        .bind(id)
        .execute(db)
        .await
        .unwrap();
    }

    fn ids(body: &Value) -> Vec<String> {
        let mut v: Vec<String> = body["data"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d["id"].as_str().unwrap().to_owned())
            .collect();
        v.sort();
        v
    }

    #[tokio::test]
    async fn peers_accessible_for_non_admin() {
        let (app, db) = test_app_with_db().await;
        insert_user(&db, "admin", PW, true).await;
        let bob = insert_user(&db, "bob", PW, false).await;
        let carol = insert_user(&db, "carol", PW, false).await;
        let dave = insert_user(&db, "dave", PW, false).await;
        // dave lives in another user group with no rules: not accessible to bob.
        let other = [7u8; 16];
        exec(
            &db,
            "insert into console_user_group(guid, name) values(?, 'Other')",
            &[&other],
        )
        .await;
        exec(
            &db,
            "update console_user set group_guid = ? where guid = ?",
            &[&other, &dave],
        )
        .await;
        // "Servers" lets bob in (type 2 rule); "Hidden" has no rules.
        let servers = [8u8; 16];
        let hidden = [9u8; 16];
        exec(
            &db,
            "insert into console_device_group(guid, name) values(?, 'Servers')",
            &[&servers],
        )
        .await;
        exec(
            &db,
            "insert into console_device_group(guid, name) values(?, 'Hidden')",
            &[&hidden],
        )
        .await;
        exec(
            &db,
            "insert into console_access_rule(owner_kind, owner_guid, direction, target_type, target_guid)
             values('device_group', ?, 'in', 2, ?)",
            &[&servers, &bob],
        )
        .await;
        for id in ["p1", "p2", "p3", "p4", "p5", "p6", "p7"] {
            insert_peer(&db, id, id.as_bytes()).await;
        }
        assign_user(&db, "p1", &bob).await;
        assign_user(&db, "p2", &carol).await;
        assign_user(&db, "p3", &dave).await;
        put_in_group(&db, "p4", &servers).await;
        put_in_group(&db, "p5", &hidden).await;
        assign_user(&db, "p7", &carol).await;
        sqlx::query("update peer set status = 0 where id = 'p7'")
            .execute(&db)
            .await
            .unwrap();
        sqlx::query(
            "insert into console_peer_sysinfo(peer_guid, sysinfo)
             select guid, '{\"hostname\":\"h1\",\"os\":\"Windows 10\",\"username\":\"w\",\"uuid\":\"x\"}'
             from peer where id = 'p1'",
        )
        .execute(&db)
        .await
        .unwrap();
        sqlx::query(
            "insert into console_peer_ext(peer_guid, device_name, device_username)
             select guid, 'Laptop', 'bobby' from peer where id = 'p1'",
        )
        .execute(&db)
        .await
        .unwrap();

        let t = login_token(&app, "bob", PW).await;
        let uri = "/api/peers?current=1&pageSize=100&accessible=&status=1";
        let (s, b) = get(&app, uri, &t).await;
        assert_eq!(s, StatusCode::OK, "{b}");
        assert_eq!(ids(&b), ["p1", "p2", "p4"]);
        assert_eq!(b["total"], 3);
        let (_, b) = get(&app, "/api/peers?accessible=", &t).await;
        assert_eq!(ids(&b), ["p1", "p2", "p4", "p7"]);

        let (_, b) = get(&app, uri, &t).await;
        let p1 = b["data"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["id"] == "p1")
            .unwrap()
            .clone();
        assert_eq!(p1["user"], hex(&bob));
        assert_eq!(p1["user_name"], "bob");
        assert_eq!(p1["status"], 1);
        assert_eq!(p1["note"], "");
        assert_eq!(p1["device_group_name"], "");
        assert_eq!(p1["info"]["device_name"], "Laptop");
        assert_eq!(p1["info"]["username"], "bobby");
        assert_eq!(p1["info"]["os"], "Windows 10");
        assert!(p1["info"].get("uuid").is_none());
        let p4 = b["data"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["id"] == "p4")
            .unwrap()
            .clone();
        assert_eq!(p4["user"], "");
        assert_eq!(p4["user_name"], "");
        assert_eq!(p4["device_group_name"], "Servers");

        // Paging.
        let (_, b) = get(&app, "/api/peers?current=2&pageSize=2&status=1", &t).await;
        assert_eq!(b["total"], 3);
        assert_eq!(b["data"].as_array().unwrap().len(), 1);

        // Admins see everything.
        let at = login_token(&app, "admin", PW).await;
        let (_, b) = get(&app, uri, &at).await;
        assert_eq!(ids(&b), ["p1", "p2", "p3", "p4", "p5", "p6"]);
    }

    #[tokio::test]
    async fn peers_fail_closed_for_non_admin_without_group_and_hide_note() {
        let (app, db) = test_app_with_db().await;
        insert_user(&db, "admin", PW, true).await;
        let eve = insert_user(&db, "eve", PW, false).await;
        let frank = insert_user(&db, "frank", PW, false).await;
        insert_user(&db, "gina", PW, false).await;
        for id in ["p1", "p2", "p3"] {
            insert_peer(&db, id, id.as_bytes()).await;
        }
        assign_user(&db, "p1", &eve).await;
        assign_user(&db, "p2", &frank).await;
        sqlx::query("update peer set note = 'secret' where id = 'p1'")
            .execute(&db)
            .await
            .unwrap();
        // Take every non-admin out of `Default`: no group, no rules.
        sqlx::query("update console_user set group_guid = null where is_admin = 0")
            .execute(&db)
            .await
            .unwrap();

        // No user group, no rules: eve sees only her own device, with the note blanked.
        let t = login_token(&app, "eve", PW).await;
        let (s, b) = get(&app, "/api/peers?accessible=", &t).await;
        assert_eq!(s, StatusCode::OK, "{b}");
        assert_eq!(ids(&b), ["p1"]);
        assert_eq!(b["total"], 1);
        assert_eq!(b["data"][0]["note"], "");

        // A non-admin with nothing assigned sees nothing.
        let t = login_token(&app, "gina", PW).await;
        let (_, b) = get(&app, "/api/peers?accessible=", &t).await;
        assert!(ids(&b).is_empty(), "{b}");
        assert_eq!(b["total"], 0);

        // Admins still see the note.
        let at = login_token(&app, "admin", PW).await;
        let (_, b) = get(&app, "/api/peers", &at).await;
        assert_eq!(ids(&b), ["p1", "p2", "p3"]);
        let p1 = b["data"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["id"] == "p1")
            .unwrap()
            .clone();
        assert_eq!(p1["note"], "secret");
    }

    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }
}
