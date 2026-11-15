//! Who may see whom. One place for the "accessible" computation, shared by
//! `GET /api/users?accessible` and (later) `GET /api/peers`.

use super::users::User;
use sqlx::SqlitePool;

/// The users `viewer` may access: `None` for admins (everyone), otherwise the
/// explicit set, which always contains the viewer. A non-admin sees:
/// - users in the viewer's own group;
/// - users in user groups G where the viewer's group has an `out` rule of type 0 -> G;
/// - users in user groups G where G has an `in` rule of type 0 -> the viewer's group,
///   or of type 2 -> the viewer;
/// - the viewer.
pub(crate) async fn accessible_user_guids(
    pool: &SqlitePool,
    viewer: &User,
) -> Result<Option<Vec<Vec<u8>>>, sqlx::Error> {
    if viewer.is_admin {
        return Ok(None);
    }
    let rows: Vec<(Vec<u8>,)> = sqlx::query_as(
        "with mine(g) as (
           select group_guid from console_user where guid = ?1 and group_guid is not null
         ),
         visible(g) as (
           select g from mine
           union
           select r.target_guid from console_access_rule r
            where r.owner_kind = 'user_group' and r.direction = 'out' and r.target_type = 0
              and r.owner_guid in (select g from mine)
           union
           select r.owner_guid from console_access_rule r
            where r.owner_kind = 'user_group' and r.direction = 'in'
              and ((r.target_type = 0 and r.target_guid in (select g from mine))
                   or (r.target_type = 2 and r.target_guid = ?1))
         )
         select guid from console_user
          where guid = ?1 or group_guid in (select g from visible)
          order by guid",
    )
    .bind(&viewer.guid)
    .fetch_all(pool)
    .await?;
    let mut out: Vec<Vec<u8>> = rows.into_iter().map(|(g,)| g).collect();
    if !out.contains(&viewer.guid) {
        out.push(viewer.guid.clone());
    }
    Ok(Some(out))
}
