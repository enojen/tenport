use std::collections::HashMap;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use sha2::{Digest, Sha256};
use time::OffsetDateTime;

use crate::cluster::{
    Cluster, ClusterError, JobName, JobState, JobUid, PROJECT_ID_LABEL, SecretName,
    TENPORT_JOB_LABELS,
};
use crate::db::Db;
use crate::github::{GITHUB_HOSTS, GithubApp, GithubError, RepoToken};
use crate::projects::{self, ProjectId, ProjectsError};

#[cfg(test)]
mod tests;

pub const RUN_PLACEHOLDER_KEY: &str = "placeholder";
const TOKEN_REUSE_MARGIN: time::Duration = time::Duration::minutes(5);

#[derive(Debug, thiserror::Error)]
pub enum RunCredentialsError {
    #[error("cannot read {}: {source}", path.display())]
    Read { path: PathBuf, source: io::Error },
    #[error("{} is empty", path.display())]
    Empty { path: PathBuf },
    #[error("no open Job of Tenport has this placeholder for {host}")]
    NoRunCredential { host: String },
    #[error(transparent)]
    Github(#[from] GithubError),
    #[error(transparent)]
    Cluster(#[from] ClusterError),
    #[error(transparent)]
    Projects(#[from] ProjectsError),
}

pub struct RunCredentials {
    cluster: Arc<dyn Cluster>,
    key: RunCredentialKey,
    github: GithubApp,
    repo_tokens: Mutex<HashMap<JobUid, RepoToken>>,
}

impl RunCredentials {
    pub fn new(cluster: Arc<dyn Cluster>, key: RunCredentialKey, github: GithubApp) -> Self {
        Self {
            cluster,
            key,
            github,
            repo_tokens: Mutex::default(),
        }
    }

    pub fn placeholder(&self, job: &JobUid) -> String {
        self.key.placeholder(job)
    }

    pub async fn repo_token(
        &self,
        db: &Db,
        placeholder: &str,
        host: &str,
    ) -> Result<String, RunCredentialsError> {
        let none = || RunCredentialsError::NoRunCredential {
            host: host.to_owned(),
        };
        if !GITHUB_HOSTS.contains(&host) {
            return Err(none());
        }
        let open = self.list_open_jobs().await?;
        self.forget_tokens_of_ended_jobs(&open);
        let job = open
            .iter()
            .find(|job| self.placeholder(&job.uid) == placeholder)
            .ok_or_else(none)?;
        let now = OffsetDateTime::now_utc();
        if let Some(token) = self.reusable_token(&job.uid, now) {
            return Ok(token);
        }
        let project = job
            .labels
            .get(PROJECT_ID_LABEL)
            .and_then(|id| ProjectId::parse(id).ok())
            .ok_or_else(none)?;
        let project = projects::find(db, &project).await?.ok_or_else(none)?;
        let token = self.github.repo_token(&project.repo, now).await?;
        let value = token.token.clone();
        self.lock_repo_tokens().insert(job.uid.clone(), token);
        Ok(value)
    }

    async fn list_open_jobs(&self) -> Result<Vec<JobState>, ClusterError> {
        let jobs = self.cluster.list_jobs(TENPORT_JOB_LABELS).await?;
        Ok(jobs.into_iter().filter(JobState::is_running).collect())
    }

    fn reusable_token(&self, job: &JobUid, now: OffsetDateTime) -> Option<String> {
        self.lock_repo_tokens()
            .get(job)
            .filter(|token| token.expires - now > TOKEN_REUSE_MARGIN)
            .map(|token| token.token.clone())
    }

    fn forget_tokens_of_ended_jobs(&self, open: &[JobState]) {
        self.lock_repo_tokens()
            .retain(|uid, _| open.iter().any(|job| job.uid == *uid));
    }

    fn lock_repo_tokens(&self) -> MutexGuard<'_, HashMap<JobUid, RepoToken>> {
        self.repo_tokens
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

pub struct RunCredentialKey(String);

impl RunCredentialKey {
    pub fn new(key: impl Into<String>) -> Self {
        Self(key.into())
    }

    pub fn read(path: &Path) -> Result<Self, RunCredentialsError> {
        Ok(Self::new(read_secret(path)?))
    }

    pub fn placeholder(&self, job: &JobUid) -> String {
        tenport_run_credentials::run_placeholder(&self.0, job.as_str())
    }
}

impl fmt::Debug for RunCredentialKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RunCredentialKey(hidden)")
    }
}

pub struct InjectorToken([u8; 32]);

impl InjectorToken {
    pub fn new(token: &str) -> Self {
        Self(Sha256::digest(token.as_bytes()).into())
    }

    pub fn read(path: &Path) -> Result<Self, RunCredentialsError> {
        let text = read_secret(path)?;
        Ok(Self::new(&text))
    }

    pub fn matches(&self, given: &str) -> bool {
        let given: [u8; 32] = Sha256::digest(given.as_bytes()).into();
        given == self.0
    }
}

impl fmt::Debug for InjectorToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("InjectorToken(hidden)")
    }
}

pub fn run_placeholder_secret(job: &JobName) -> SecretName {
    SecretName::new(format!("{}-placeholder", job.as_str()))
}

fn read_secret(path: &Path) -> Result<String, RunCredentialsError> {
    let text = std::fs::read_to_string(path).map_err(|source| RunCredentialsError::Read {
        path: path.to_owned(),
        source,
    })?;
    let secret = text.trim();
    if secret.is_empty() {
        return Err(RunCredentialsError::Empty {
            path: path.to_owned(),
        });
    }
    Ok(secret.to_owned())
}
