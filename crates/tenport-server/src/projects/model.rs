use serde::{Deserialize, Serialize};

use crate::accounts::OrgId;
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
}

impl ProjectsEvent {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::ProjectAdded(_) => "project.added",
        }
    }

    pub fn subject(&self) -> String {
        match self {
            Self::ProjectAdded(added) => format!("project:{}", added.id),
        }
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
