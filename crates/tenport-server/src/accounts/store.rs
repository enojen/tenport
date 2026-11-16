use sqlx::SqliteConnection;

use super::model::{
    MemberAdded, Membership, OrgId, OrganizationAdded, TokenAdded, TokenHash, User, UserAdded,
    UserId,
};
use crate::events::Stamp;
use crate::name::Name;

pub async fn insert_organization(
    conn: &mut SqliteConnection,
    added: &OrganizationAdded,
    stamp: &Stamp<'_>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "insert into organizations (id, name, created_at, created_by) values (?, ?, ?, ?)",
        added.id,
        added.name,
        stamp.at,
        stamp.actor,
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn insert_user(
    conn: &mut SqliteConnection,
    added: &UserAdded,
    stamp: &Stamp<'_>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "insert into users (id, name, email, github, created_at, created_by)
         values (?, ?, ?, ?, ?, ?)",
        added.id,
        added.name,
        added.email,
        added.github,
        stamp.at,
        stamp.actor,
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn insert_membership(
    conn: &mut SqliteConnection,
    added: &MemberAdded,
    stamp: &Stamp<'_>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "insert into memberships (org_id, user_id, role, created_at, created_by)
         values (?, ?, ?, ?, ?)",
        added.org,
        added.user,
        added.role,
        stamp.at,
        stamp.actor,
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn insert_token(
    conn: &mut SqliteConnection,
    added: &TokenAdded,
    stamp: &Stamp<'_>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "insert into tokens (id, user_id, name, hash, created_at, created_by)
         values (?, ?, ?, ?, ?, ?)",
        added.id,
        added.user,
        added.name,
        added.hash,
        stamp.at,
        stamp.actor,
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn find_organization_id_by_name(
    conn: &mut SqliteConnection,
    name: &Name,
) -> Result<Option<OrgId>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"select id as "id: OrgId" from organizations where name = ?"#,
        name,
    )
    .fetch_optional(conn)
    .await
}

pub async fn find_user_by_token_hash(
    conn: &mut SqliteConnection,
    hash: &TokenHash,
) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as!(
        User,
        r#"select users.id as "id: _", users.name as "name: _", users.email as "email: _",
                  users.github as "github: _"
           from tokens join users on users.id = tokens.user_id
           where tokens.hash = ?"#,
        hash,
    )
    .fetch_optional(conn)
    .await
}

pub async fn list_memberships_of_user(
    conn: &mut SqliteConnection,
    user: &UserId,
) -> Result<Vec<Membership>, sqlx::Error> {
    sqlx::query_as!(
        Membership,
        r#"select organizations.id as "org: _", organizations.name as "org_name: _", memberships.role as "role: _"
           from memberships join organizations on organizations.id = memberships.org_id
           where memberships.user_id = ?
           order by organizations.name"#,
        user,
    )
    .fetch_all(conn)
    .await
}
