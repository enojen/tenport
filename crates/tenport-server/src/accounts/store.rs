use sqlx::SqliteConnection;

use super::model::{
    MemberAdded, Membership, OrganizationAdded, OrganizationId, Role, TokenAdded, TokenHash, User,
    UserAdded, UserId,
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
        "insert into memberships (organization_id, user_id, role, created_at, created_by)
         values (?, ?, ?, ?, ?)",
        added.organization,
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
) -> Result<Option<OrganizationId>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"select id as "id: OrganizationId" from organizations where name = ?"#,
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
        r#"select organizations.id as "organization: _", organizations.name as "organization_name: _", memberships.role as "role: _"
           from memberships join organizations on organizations.id = memberships.organization_id
           where memberships.user_id = ?
           order by organizations.name"#,
        user,
    )
    .fetch_all(conn)
    .await
}

pub async fn find_role_of_user(
    conn: &mut SqliteConnection,
    organization: &OrganizationId,
    user: &UserId,
) -> Result<Option<Role>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"select role as "role: Role" from memberships where organization_id = ? and user_id = ?"#,
        organization,
        user,
    )
    .fetch_optional(conn)
    .await
}
