use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use crate::cluster::{JobName, JobUid, SecretName};

#[cfg(test)]
mod tests;

pub const RUN_TOKEN_KEY: &str = "token";

#[derive(Debug, thiserror::Error)]
pub enum RunCredentialsError {
    #[error("cannot read {}: {source}", path.display())]
    Read { path: PathBuf, source: io::Error },
    #[error("{} is empty", path.display())]
    Empty { path: PathBuf },
}

pub struct RunCredentialKey(String);

impl RunCredentialKey {
    pub fn new(key: impl Into<String>) -> Self {
        Self(key.into())
    }

    pub fn read(path: &Path) -> Result<Self, RunCredentialsError> {
        let text = std::fs::read_to_string(path).map_err(|source| RunCredentialsError::Read {
            path: path.to_owned(),
            source,
        })?;
        let key = text.trim();
        if key.is_empty() {
            return Err(RunCredentialsError::Empty {
                path: path.to_owned(),
            });
        }
        Ok(Self::new(key))
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

pub fn run_token_secret(job: &JobName) -> SecretName {
    SecretName::new(format!("{}-github", job.as_str()))
}
