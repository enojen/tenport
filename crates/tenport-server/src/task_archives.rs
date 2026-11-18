use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use reqwest::{Client, Method, StatusCode};
use rusty_s3::{Bucket, Credentials, S3Action, UrlStyle};
use serde::Serialize;
use url::Url;
use utoipa::ToSchema;

use crate::config::TaskArchiveBucketConfig;
use crate::https_client::{HttpsError, https_client};
use crate::projects::ProjectId;
use crate::tasks::TaskId;

#[cfg(test)]
mod tests;

const URL_LIFETIME: Duration = Duration::from_secs(24 * 60 * 60);
const REQUEST_URL_LIFETIME: Duration = Duration::from_secs(60);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, thiserror::Error)]
pub enum TaskArchivesError {
    #[error("cannot read {}: {source}", path.display())]
    Read { path: PathBuf, source: io::Error },
    #[error("{} is empty", path.display())]
    Empty { path: PathBuf },
    #[error("the bucket {url} is not an http(s) URL with the bucket name as its path: {detail}")]
    BadUrl { url: String, detail: String },
    #[error(transparent)]
    Client(#[from] HttpsError),
    #[error("the bucket did not answer {method} {key}: {source}")]
    Request {
        method: Method,
        key: String,
        source: reqwest::Error,
    },
    #[error("the bucket answered {method} {key} with {status}")]
    Answer {
        method: Method,
        key: String,
        status: StatusCode,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TaskArchiveState {
    Deleted,
    Missing,
}

pub struct TaskArchiveBucket {
    bucket: Bucket,
    credentials: Credentials,
    client: Client,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskArchiveUrls {
    pub get: String,
    pub put: String,
}

impl TaskArchiveBucket {
    pub fn new(
        url: &str,
        region: &str,
        key: String,
        secret: String,
    ) -> Result<Self, TaskArchivesError> {
        Ok(Self {
            bucket: bucket(url, region)?,
            credentials: Credentials::new(key, secret),
            client: https_client(REQUEST_TIMEOUT)?,
        })
    }

    pub fn open(config: &TaskArchiveBucketConfig) -> Result<Self, TaskArchivesError> {
        Self::new(
            &config.url,
            &config.region,
            read_secret(&config.key_file)?,
            read_secret(&config.secret_file)?,
        )
    }

    pub fn presign(&self, project: &ProjectId, task: &TaskId) -> TaskArchiveUrls {
        let key = task_archive_key(project, task);
        let credentials = Some(&self.credentials);
        TaskArchiveUrls {
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

impl TaskArchiveBucket {
    pub async fn delete(
        &self,
        project: &ProjectId,
        task: &TaskId,
    ) -> Result<TaskArchiveState, TaskArchivesError> {
        let key = task_archive_key(project, task);
        let credentials = Some(&self.credentials);
        let head = self.bucket.head_object(credentials, &key);
        let found = self
            .send(Method::HEAD, head.sign(REQUEST_URL_LIFETIME), &key)
            .await?;
        if found == StatusCode::NOT_FOUND {
            return Ok(TaskArchiveState::Missing);
        }
        let delete = self.bucket.delete_object(credentials, &key);
        self.send(Method::DELETE, delete.sign(REQUEST_URL_LIFETIME), &key)
            .await?;
        Ok(TaskArchiveState::Deleted)
    }

    async fn send(
        &self,
        method: Method,
        url: Url,
        key: &str,
    ) -> Result<StatusCode, TaskArchivesError> {
        let response = self
            .client
            .request(method.clone(), url)
            .send()
            .await
            .map_err(|source| TaskArchivesError::Request {
                method: method.clone(),
                key: key.to_owned(),
                source,
            })?;
        let status = response.status();
        if status.is_success() || (method == Method::HEAD && status == StatusCode::NOT_FOUND) {
            Ok(status)
        } else {
            Err(TaskArchivesError::Answer {
                method,
                key: key.to_owned(),
                status,
            })
        }
    }
}

impl fmt::Debug for TaskArchiveBucket {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TaskArchiveBucket")
            .field("url", &self.bucket.base_url().as_str())
            .field("region", &self.bucket.region())
            .finish_non_exhaustive()
    }
}

fn task_archive_key(project: &ProjectId, task: &TaskId) -> String {
    format!("tasks/{project}/{task}.tar.zst")
}

fn bucket(url: &str, region: &str) -> Result<Bucket, TaskArchivesError> {
    let bad_url = |detail: String| TaskArchivesError::BadUrl {
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

fn read_secret(path: &Path) -> Result<String, TaskArchivesError> {
    let text = std::fs::read_to_string(path).map_err(|source| TaskArchivesError::Read {
        path: path.to_owned(),
        source,
    })?;
    let secret = text.trim();
    if secret.is_empty() {
        return Err(TaskArchivesError::Empty {
            path: path.to_owned(),
        });
    }
    Ok(secret.to_owned())
}
