use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

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

pub fn run_token_secret(job: &JobName) -> SecretName {
    SecretName::new(format!("{}-github", job.as_str()))
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
