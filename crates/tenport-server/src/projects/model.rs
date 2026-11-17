use std::fmt;

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;

use crate::accounts::OrgId;
use crate::cluster::SecretName;
use crate::id::short_id;
use crate::name::{Name, parsed_text};

short_id!(ProjectId, "", "a project id");

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[serde(try_from = "String", into = "String")]
#[sqlx(transparent)]
pub struct Repo(String);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("a repository is owner/name, as on GitHub")]
pub struct RepoError;

impl Repo {
    pub fn parse(raw: &str) -> Result<Self, RepoError> {
        let repo = raw.trim();
        match repo.split_once('/') {
            Some((owner, name)) if is_owner(owner) && is_repo_name(name) => {
                Ok(Self(repo.to_owned()))
            }
            _ => Err(RepoError),
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn is_owner(owner: &str) -> bool {
    !owner.is_empty() && owner.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

fn is_repo_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

parsed_text!(Repo, RepoError);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    pub id: ProjectId,
    pub org: OrgId,
    pub name: Name,
    pub repo: Repo,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ProjectsEvent {
    #[serde(rename = "project.added")]
    ProjectAdded(ProjectAdded),
    #[serde(rename = "project.secret_changed")]
    SecretChanged(SecretChanged),
    #[serde(rename = "project.secret_deleted")]
    SecretDeleted(SecretDeleted),
}

impl ProjectsEvent {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::ProjectAdded(_) => "project.added",
            Self::SecretChanged(_) => "project.secret_changed",
            Self::SecretDeleted(_) => "project.secret_deleted",
        }
    }

    pub fn subject(&self) -> String {
        let project = match self {
            Self::ProjectAdded(added) => &added.id,
            Self::SecretChanged(changed) => &changed.project,
            Self::SecretDeleted(deleted) => &deleted.project,
        };
        format!("project:{project}")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectAdded {
    pub id: ProjectId,
    pub org: OrgId,
    pub name: Name,
    pub repo: Repo,
}

impl From<ProjectAdded> for Project {
    fn from(added: ProjectAdded) -> Self {
        Self {
            id: added.id,
            org: added.org,
            name: added.name,
            repo: added.repo,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "text", rename_all = "snake_case")]
pub enum ProjectSecret {
    TrackerApiKey,
    NtfyUrl,
}

impl ProjectSecret {
    pub fn name(self) -> &'static str {
        match self {
            Self::TrackerApiKey => "tracker_api_key",
            Self::NtfyUrl => "ntfy_url",
        }
    }
}

impl fmt::Display for ProjectSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

pub fn kube_secret_of(project: &ProjectId) -> SecretName {
    SecretName::new(format!("project-{project}"))
}

const MAX_SECRET_VALUE_LENGTH: usize = 4096;

#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(try_from = "String")]
pub struct SecretValue(String);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("a secret value is 1 to {MAX_SECRET_VALUE_LENGTH} characters")]
pub struct SecretValueError;

impl SecretValue {
    pub fn parse(raw: &str) -> Result<Self, SecretValueError> {
        let length = raw.chars().count();
        if (1..=MAX_SECRET_VALUE_LENGTH).contains(&length) {
            Ok(Self(raw.to_owned()))
        } else {
            Err(SecretValueError)
        }
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for SecretValue {
    type Error = SecretValueError;

    fn try_from(raw: String) -> Result<Self, SecretValueError> {
        Self::parse(&raw)
    }
}

impl fmt::Debug for SecretValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretValue(hidden)")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecretChanged {
    pub project: ProjectId,
    pub secret: ProjectSecret,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecretDeleted {
    pub project: ProjectId,
    pub secret: ProjectSecret,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretState {
    pub secret: ProjectSecret,
    pub changed: OffsetDateTime,
}
