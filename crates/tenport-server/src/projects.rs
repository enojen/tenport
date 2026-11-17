mod model;
mod store;

#[cfg(test)]
mod tests;

use sqlx::SqliteConnection;

use crate::accounts::{self, AccountsError, OrgId, Role, UserId};
use crate::db::Db;
use crate::events::{self, Actor, ApplyError, Event, EventsError, Stamp};
use crate::name::Name;

pub use model::{Project, ProjectAdded, ProjectId, ProjectsEvent, Repo, RepoError};

#[derive(Debug, thiserror::Error)]
pub enum ProjectsError {
    #[error("only an admin of the organization {org} adds a project to it")]
    NotAdmin { org: OrgId },
    #[error("only a member of the organization of the project {project} uses it")]
    NotMember { project: ProjectId },
    #[error(transparent)]
    Accounts(#[from] AccountsError),
    #[error(transparent)]
    Events(#[from] EventsError),
    #[error("cannot read projects: {0}")]
    Database(#[from] sqlx::Error),
}

#[derive(Debug, Clone)]
pub struct NewProject {
    pub org: OrgId,
    pub name: Name,
    pub repo: Repo,
}

pub async fn add(db: &Db, user: &UserId, project: NewProject) -> Result<Project, ProjectsError> {
    require_admin(db, user, &project.org).await?;
    let added = ProjectAdded {
        id: ProjectId::random(),
        org: project.org,
        name: project.name,
        repo: project.repo,
    };
    let event = Event::Projects(ProjectsEvent::ProjectAdded(added.clone()));
    events::append(db, Actor::User(user.clone()), event).await?;
    Ok(added.into())
}

pub async fn list_of_user(
    db: &Db,
    user: &UserId,
    after: Option<&ProjectId>,
    limit: i64,
) -> Result<Vec<Project>, ProjectsError> {
    let mut conn = db.acquire().await?;
    Ok(store::list_projects_of_user(&mut conn, user, after, limit).await?)
}

pub(crate) async fn apply(
    conn: &mut SqliteConnection,
    event: &ProjectsEvent,
    stamp: &Stamp<'_>,
) -> Result<(), ApplyError> {
    match event {
        ProjectsEvent::ProjectAdded(added) => store::insert_project(conn, added, stamp).await?,
    }
    Ok(())
}

pub async fn require_member(
    db: &Db,
    user: &UserId,
    project: &ProjectId,
) -> Result<Project, ProjectsError> {
    find_project_for_member(db, user, project)
        .await?
        .ok_or_else(|| ProjectsError::NotMember {
            project: project.clone(),
        })
}

async fn find_project_for_member(
    db: &Db,
    user: &UserId,
    project: &ProjectId,
) -> Result<Option<Project>, ProjectsError> {
    let mut conn = db.acquire().await?;
    Ok(store::find_project_for_member(&mut conn, project, user).await?)
}

async fn require_admin(db: &Db, user: &UserId, org: &OrgId) -> Result<(), ProjectsError> {
    let memberships = accounts::memberships(db, user).await?;
    let is_admin = memberships
        .iter()
        .any(|membership| membership.org == *org && membership.role == Role::Admin);
    if is_admin {
        Ok(())
    } else {
        Err(ProjectsError::NotAdmin { org: org.clone() })
    }
}
