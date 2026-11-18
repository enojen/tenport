mod model;
mod store;

#[cfg(test)]
mod tests;

use sqlx::SqliteConnection;

use crate::accounts::{self, AccountsError, OrganizationId, Role, UserId};
use crate::cluster::{Cluster, ClusterError};
use crate::db::Db;
use crate::events::{self, Actor, ApplyError, Event, EventsError, Stamp};
use crate::name::Name;

pub use model::{
    Project, ProjectAdded, ProjectId, ProjectSecret, ProjectsEvent, Repo, RepoError, SecretChanged,
    SecretDeleted, SecretState, SecretValue, SecretValueError, kube_secret_of,
};

#[derive(Debug, thiserror::Error)]
pub enum ProjectsError {
    #[error("only an admin of the organization {organization} adds a project to it")]
    NotAdmin { organization: OrganizationId },
    #[error("only a member of the organization of the project {project} uses it")]
    NotMember { project: ProjectId },
    #[error("only an admin of the organization of the project {project} writes its secrets")]
    NotProjectAdmin { project: ProjectId },
    #[error("the project {project} has no secret {secret}")]
    SecretNotSet {
        project: ProjectId,
        secret: ProjectSecret,
    },
    #[error(transparent)]
    Cluster(#[from] ClusterError),
    #[error(transparent)]
    Accounts(#[from] AccountsError),
    #[error(transparent)]
    Events(#[from] EventsError),
    #[error("cannot read projects: {0}")]
    Database(#[from] sqlx::Error),
}

#[derive(Debug, Clone)]
pub struct NewProject {
    pub organization: OrganizationId,
    pub name: Name,
    pub repo: Repo,
}

pub async fn add(db: &Db, user: &UserId, project: NewProject) -> Result<Project, ProjectsError> {
    require_admin(db, user, &project.organization).await?;
    let added = ProjectAdded {
        id: ProjectId::random(),
        organization: project.organization,
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

pub async fn write_secret(
    db: &Db,
    cluster: &dyn Cluster,
    user: &UserId,
    project: &ProjectId,
    secret: ProjectSecret,
    value: &SecretValue,
) -> Result<SecretState, ProjectsError> {
    require_project_admin(db, user, project).await?;
    cluster
        .write_secret_key(&kube_secret_of(project), secret.name(), value.expose())
        .await?;
    let changed = SecretChanged {
        project: project.clone(),
        secret,
    };
    let event = Event::Projects(ProjectsEvent::SecretChanged(changed));
    events::append(db, Actor::User(user.clone()), event).await?;
    find_secret(db, project, secret)
        .await?
        .ok_or_else(|| ProjectsError::SecretNotSet {
            project: project.clone(),
            secret,
        })
}

pub async fn delete_secret(
    db: &Db,
    cluster: &dyn Cluster,
    user: &UserId,
    project: &ProjectId,
    secret: ProjectSecret,
) -> Result<(), ProjectsError> {
    require_project_admin(db, user, project).await?;
    let in_cluster = cluster
        .delete_secret_key(&kube_secret_of(project), secret.name())
        .await?;
    let recorded = find_secret(db, project, secret).await?.is_some();
    if !in_cluster && !recorded {
        return Err(ProjectsError::SecretNotSet {
            project: project.clone(),
            secret,
        });
    }
    let deleted = SecretDeleted {
        project: project.clone(),
        secret,
    };
    let event = Event::Projects(ProjectsEvent::SecretDeleted(deleted));
    events::append(db, Actor::User(user.clone()), event).await?;
    Ok(())
}

pub async fn list_secrets(
    db: &Db,
    user: &UserId,
    project: &ProjectId,
) -> Result<Vec<SecretState>, ProjectsError> {
    require_member(db, user, project).await?;
    let mut conn = db.acquire().await?;
    Ok(store::list_secrets_of_project(&mut conn, project).await?)
}

async fn find_secret(
    db: &Db,
    project: &ProjectId,
    secret: ProjectSecret,
) -> Result<Option<SecretState>, ProjectsError> {
    let mut conn = db.acquire().await?;
    Ok(store::find_secret(&mut conn, project, secret).await?)
}

pub(crate) async fn apply(
    conn: &mut SqliteConnection,
    event: &ProjectsEvent,
    stamp: &Stamp<'_>,
) -> Result<(), ApplyError> {
    match event {
        ProjectsEvent::ProjectAdded(added) => store::insert_project(conn, added, stamp).await?,
        ProjectsEvent::SecretChanged(changed) => store::insert_secret(conn, changed, stamp).await?,
        ProjectsEvent::SecretDeleted(deleted) => {
            store::delete_secret(conn, &deleted.project, deleted.secret).await?
        }
    }
    Ok(())
}

pub async fn find(db: &Db, project: &ProjectId) -> Result<Option<Project>, ProjectsError> {
    let mut conn = db.acquire().await?;
    Ok(store::find_project(&mut conn, project).await?)
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

async fn require_project_admin(
    db: &Db,
    user: &UserId,
    project: &ProjectId,
) -> Result<(), ProjectsError> {
    let found = require_member(db, user, project).await?;
    match require_admin(db, user, &found.organization).await {
        Err(ProjectsError::NotAdmin { .. }) => Err(ProjectsError::NotProjectAdmin {
            project: project.clone(),
        }),
        other => other,
    }
}

async fn require_admin(
    db: &Db,
    user: &UserId,
    organization: &OrganizationId,
) -> Result<(), ProjectsError> {
    let memberships = accounts::memberships(db, user).await?;
    let is_admin = memberships.iter().any(|membership| {
        membership.organization == *organization && membership.role == Role::Admin
    });
    if is_admin {
        Ok(())
    } else {
        Err(ProjectsError::NotAdmin {
            organization: organization.clone(),
        })
    }
}
