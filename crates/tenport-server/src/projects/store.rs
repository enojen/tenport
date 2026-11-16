use sqlx::SqliteConnection;

use super::model::{Project, ProjectAdded};
use crate::accounts::UserId;
use crate::events::Stamp;

pub async fn insert_project(
    conn: &mut SqliteConnection,
    added: &ProjectAdded,
    stamp: &Stamp<'_>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "insert into projects (id, org_id, name, repo, created_at, created_by)
         values (?, ?, ?, ?, ?, ?)",
        added.id,
        added.org,
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
) -> Result<Vec<Project>, sqlx::Error> {
    sqlx::query_as!(
        Project,
        r#"select projects.id as "id: _", projects.org_id as "org: _", projects.name as "name: _",
                  projects.repo as "repo: _"
           from projects join memberships on memberships.org_id = projects.org_id
           where memberships.user_id = ?
           order by projects.name, projects.id"#,
        user,
    )
    .fetch_all(conn)
    .await
}
