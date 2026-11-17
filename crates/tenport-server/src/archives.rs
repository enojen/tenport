use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusty_s3::{Bucket, Credentials, S3Action, UrlStyle};
use url::Url;

use crate::config::ArchiveBucketConfig;
use crate::projects::ProjectId;
use crate::tasks::TaskId;

#[cfg(test)]
mod tests;

const URL_LIFETIME: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Debug, thiserror::Error)]
pub enum ArchivesError {
    #[error("cannot read {}: {source}", path.display())]
    Read { path: PathBuf, source: io::Error },
    #[error("{} is empty", path.display())]
    Empty { path: PathBuf },
    #[error("the bucket {url} is not an http(s) URL with the bucket name as its path: {detail}")]
    BadUrl { url: String, detail: String },
}

pub struct ArchiveBucket {
    bucket: Bucket,
    credentials: Credentials,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveUrls {
    pub get: String,
    pub put: String,
}

impl ArchiveBucket {
    pub fn new(
        url: &str,
        region: &str,
        key: String,
        secret: String,
    ) -> Result<Self, ArchivesError> {
        Ok(Self {
            bucket: bucket(url, region)?,
            credentials: Credentials::new(key, secret),
        })
    }

    pub fn open(config: &ArchiveBucketConfig) -> Result<Self, ArchivesError> {
        Self::new(
            &config.url,
            &config.region,
            read_secret(&config.key_file)?,
            read_secret(&config.secret_file)?,
        )
    }

    pub fn presign(&self, project: &ProjectId, task: &TaskId) -> ArchiveUrls {
        let key = task_archive_key(project, task);
        let credentials = Some(&self.credentials);
        ArchiveUrls {
            get: self
                .bucket
                .get_object(credentials, &key)
                .sign(URL_LIFETIME)
                .into(),
            put: self
                .bucket
                .put_object(credentials, &key)
                .sign(URL_LIFETIME)
                .into(),
        }
    }
}

impl fmt::Debug for ArchiveBucket {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ArchiveBucket")
            .field("url", &self.bucket.base_url().as_str())
            .field("region", &self.bucket.region())
            .finish_non_exhaustive()
    }
}

fn task_archive_key(project: &ProjectId, task: &TaskId) -> String {
    format!("tasks/{project}/{task}.tar.zst")
}

fn bucket(url: &str, region: &str) -> Result<Bucket, ArchivesError> {
    let bad_url = |detail: String| ArchivesError::BadUrl {
        url: url.to_owned(),
        detail,
    };
    let mut endpoint = Url::parse(url).map_err(|error| bad_url(error.to_string()))?;
    let name = endpoint.path().trim_matches('/').to_owned();
    if name.is_empty() || name.contains('/') {
        return Err(bad_url(format!(
            "the path {:?} is not one name",
            endpoint.path()
        )));
    }
    endpoint.set_path("");
    Bucket::new(endpoint, UrlStyle::Path, name, region.to_owned())
        .map_err(|error| bad_url(error.to_string()))
}

fn read_secret(path: &Path) -> Result<String, ArchivesError> {
    let text = std::fs::read_to_string(path).map_err(|source| ArchivesError::Read {
        path: path.to_owned(),
        source,
    })?;
    let secret = text.trim();
    if secret.is_empty() {
        return Err(ArchivesError::Empty {
            path: path.to_owned(),
        });
    }
    Ok(secret.to_owned())
}
