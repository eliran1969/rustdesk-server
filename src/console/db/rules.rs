// Consumed by the group management API in later tasks.
#![allow(dead_code)]

use sqlx::{Row, SqliteConnection, SqlitePool};

/// `target_type`: 0 user group, 1 device group, 2 user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Rule {
    pub target_type: i64,
    pub name: String,
}

#[derive(Debug)]
pub(crate) enum RuleError {
    Unknown(String),
    InvalidType(i64),
    Db(sqlx::Error),
}

impl From<sqlx::Error> for RuleError {
    fn from(e: sqlx::Error) -> Self {
        RuleError::Db(e)
    }
}

fn type_allowed(owner_kind: &str, direction: &str, target_type: i64) -> bool {
    match (owner_kind, direction) {
        ("user_group", "in") | ("device_group", "in") => matches!(target_type, 0 | 2),
        ("user_group", "out") => matches!(target_type, 0 | 1),
        _ => false,
    }
}

fn target_sql(target_type: i64) -> Option<&'static str> {
    match target_type {
        0 => Some("select guid from console_user_group where name = ?"),
        1 => Some("select guid from console_device_group where name = ?"),
        2 => Some("select guid from console_user where name = ?"),
        _ => None,
    }
}

pub(crate) async fn get(
    pool: &SqlitePool,
    owner_kind: &str,
    owner_guid: &[u8],
    direction: &str,
) -> Result<Vec<Rule>, sqlx::Error> {
    let rows = sqlx::query(
        "select r.target_type as target_type,
                case r.target_type
                  when 0 then (select name from console_user_group where guid = r.target_guid)
                  when 1 then (select name from console_device_group where guid = r.target_guid)
                  else (select name from console_user where guid = r.target_guid)
                end as name
         from console_access_rule r
         where r.owner_kind = ? and r.owner_guid = ? and r.direction = ?",
    )
    .bind(owner_kind)
    .bind(owner_guid)
    .bind(direction)
    .fetch_all(pool)
    .await?;
    let mut out = Vec::new();
    for row in &rows {
        let name: Option<String> = row.try_get("name")?;
        // A target that no longer exists is skipped.
        if let Some(name) = name {
            out.push(Rule {
                target_type: row.try_get("target_type")?,
                name,
            });
        }
    }
    out.sort_by_key(|r| r.name.to_lowercase());
    Ok(out)
}

/// Replaces all rules of one direction. Names are validated and resolved
/// before anything is deleted; pass a transaction so the replace is atomic.
pub(crate) async fn replace(
    conn: &mut SqliteConnection,
    owner_kind: &str,
    owner_guid: &[u8],
    direction: &str,
    rules: &[Rule],
) -> Result<(), RuleError> {
    let mut resolved: Vec<(i64, Vec<u8>)> = Vec::with_capacity(rules.len());
    for r in rules {
        if !type_allowed(owner_kind, direction, r.target_type) {
            return Err(RuleError::InvalidType(r.target_type));
        }
        let sql = target_sql(r.target_type).ok_or(RuleError::InvalidType(r.target_type))?;
        let row = sqlx::query(sql)
            .bind(&r.name)
            .fetch_optional(&mut *conn)
            .await?;
        match row {
            Some(row) => resolved.push((r.target_type, row.try_get("guid")?)),
            None => return Err(RuleError::Unknown(r.name.clone())),
        }
    }
    sqlx::query(
        "delete from console_access_rule where owner_kind = ? and owner_guid = ? and direction = ?",
    )
    .bind(owner_kind)
    .bind(owner_guid)
    .bind(direction)
    .execute(&mut *conn)
    .await?;
    for (t, guid) in resolved {
        sqlx::query(
            "insert or ignore into console_access_rule
               (owner_kind, owner_guid, direction, target_type, target_guid)
             values(?, ?, ?, ?, ?)",
        )
        .bind(owner_kind)
        .bind(owner_guid)
        .bind(direction)
        .bind(t)
        .bind(guid)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::console::{
        db::{
            groups::{self, GroupKind},
            migrations::migrate,
        },
        test_util::{insert_user, test_pool},
    };

    fn rule(t: i64, n: &str) -> Rule {
        Rule {
            target_type: t,
            name: n.to_string(),
        }
    }

    #[tokio::test]
    async fn rules_replace_resolves_names_and_rejects_unknown() {
        let pool = test_pool().await;
        migrate(&pool, ":memory:").await.unwrap();
        let owner = groups::create(&pool, GroupKind::User, "Sales", "")
            .await
            .unwrap();
        groups::create(&pool, GroupKind::Device, "Servers", "")
            .await
            .unwrap();
        insert_user(&pool, "carol", "password123", false).await;

        let mut tx = pool.begin().await.unwrap();
        replace(
            &mut tx,
            "user_group",
            &owner,
            "in",
            &[rule(0, "default"), rule(2, "CAROL")],
        )
        .await
        .unwrap();
        replace(&mut tx, "user_group", &owner, "out", &[rule(1, "servers")])
            .await
            .unwrap();
        tx.commit().await.unwrap();

        let got = get(&pool, "user_group", &owner, "in").await.unwrap();
        assert_eq!(got, vec![rule(2, "carol"), rule(0, "Default")]);
        let out = get(&pool, "user_group", &owner, "out").await.unwrap();
        assert_eq!(out, vec![rule(1, "Servers")]);

        let mut tx = pool.begin().await.unwrap();
        let err = replace(&mut tx, "user_group", &owner, "in", &[rule(0, "nope")]).await;
        assert!(matches!(err, Err(RuleError::Unknown(n)) if n == "nope"));
        tx.commit().await.unwrap();
        // The failed replace left the previous rules intact.
        assert_eq!(
            get(&pool, "user_group", &owner, "in").await.unwrap().len(),
            2
        );
    }

    #[tokio::test]
    async fn rules_reject_invalid_type_for_direction() {
        let pool = test_pool().await;
        migrate(&pool, ":memory:").await.unwrap();
        let owner = groups::create(&pool, GroupKind::User, "Sales", "")
            .await
            .unwrap();
        let mut conn = pool.acquire().await.unwrap();
        assert!(matches!(
            replace(&mut conn, "user_group", &owner, "out", &[rule(2, "x")]).await,
            Err(RuleError::InvalidType(2))
        ));
        assert!(matches!(
            replace(&mut conn, "user_group", &owner, "in", &[rule(1, "x")]).await,
            Err(RuleError::InvalidType(1))
        ));
        assert!(matches!(
            replace(&mut conn, "device_group", &owner, "out", &[rule(0, "x")]).await,
            Err(RuleError::InvalidType(0))
        ));
    }
}
