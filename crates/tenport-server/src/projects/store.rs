use sqlx::SqliteConnection;

use super::model::{Project, ProjectAdded, ProjectId, ProjectSecret, SecretChanged, SecretState};
use crate::accounts::{Role, UserId};
use crate::events::Stamp;

pub async fn insert_project(
    conn: &mut SqliteConnection,
    added: &ProjectAdded,
    stamp: &Stamp<'_>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "insert into projects (id, organization_id, name, repo, created_at, created_by)
         values (?, ?, ?, ?, ?, ?)",
        added.id,
        added.organization,
        added.name,
        added.repo,
        stamp.at,
        stamp.actor,
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn list_projects_of_user(
    conn: &mut SqliteConnection,
    user: &UserId,
    after: Option<&ProjectId>,
    limit: i64,
) -> Result<Vec<Project>, sqlx::Error> {
    sqlx::query_as!(
        Project,
        r#"select projects.id as "id: _", projects.organization_id as "organization: _", projects.name as "name: _",
                  projects.repo as "repo: _"
           from projects join memberships on memberships.organization_id = projects.organization_id
           where memberships.user_id = ?
             and (? is null
                  or (projects.name, projects.id) > (select name, id from projects where id = ?))
           order by projects.name, projects.id
           limit ?"#,
        user,
        after,
        after,
        limit,
    )
    .fetch_all(conn)
    .await
}

pub async fn find_project(
    conn: &mut SqliteConnection,
    project: &ProjectId,
) -> Result<Option<Project>, sqlx::Error> {
    sqlx::query_as!(
        Project,
        r#"select id as "id: _", organization_id as "organization: _", name as "name: _", repo as "repo: _"
           from projects where id = ?"#,
        project,
    )
    .fetch_optional(conn)
    .await
}

pub async fn find_project_for_member(
    conn: &mut SqliteConnection,
    project: &ProjectId,
    user: &UserId,
) -> Result<Option<Project>, sqlx::Error> {
    sqlx::query_as!(
        Project,
        r#"select projects.id as "id: _", projects.organization_id as "organization: _", projects.name as "name: _",
                  projects.repo as "repo: _"
           from projects join memberships on memberships.organization_id = projects.organization_id
           where projects.id = ? and memberships.user_id = ?"#,
        project,
        user,
    )
    .fetch_optional(conn)
    .await
}

pub async fn find_role_of_user_in_project(
    conn: &mut SqliteConnection,
    project: &ProjectId,
    user: &UserId,
) -> Result<Option<Role>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"select memberships.role as "role: Role"
           from projects join memberships on memberships.organization_id = projects.organization_id
           where projects.id = ? and memberships.user_id = ?"#,
        project,
        user,
    )
    .fetch_optional(conn)
    .await
}

pub async fn insert_secret(
    conn: &mut SqliteConnection,
    changed: &SecretChanged,
    stamp: &Stamp<'_>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "insert into project_secrets (project_id, secret, changed_at, changed_by)
         values (?, ?, ?, ?)
         on conflict (project_id, secret)
         do update set changed_at = excluded.changed_at, changed_by = excluded.changed_by",
        changed.project,
        changed.secret,
        stamp.at,
        stamp.actor,
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn delete_secret(
    conn: &mut SqliteConnection,
    project: &ProjectId,
    secret: ProjectSecret,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "delete from project_secrets where project_id = ? and secret = ?",
        project,
        secret,
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn find_secret(
    conn: &mut SqliteConnection,
    project: &ProjectId,
    secret: ProjectSecret,
) -> Result<Option<SecretState>, sqlx::Error> {
    sqlx::query_as!(
        SecretState,
        r#"select secret as "secret: _", changed_at as "changed: _"
           from project_secrets where project_id = ? and secret = ?"#,
        project,
        secret,
    )
    .fetch_optional(conn)
    .await
}

pub async fn list_secrets_of_project(
    conn: &mut SqliteConnection,
    project: &ProjectId,
) -> Result<Vec<SecretState>, sqlx::Error> {
    sqlx::query_as!(
        SecretState,
        r#"select secret as "secret: _", changed_at as "changed: _"
           from project_secrets where project_id = ? order by secret"#,
        project,
    )
    .fetch_all(conn)
    .await
}
