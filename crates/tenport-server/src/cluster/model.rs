use std::collections::BTreeMap;
use std::pin::Pin;

use futures::AsyncBufRead;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct JobName(String);

impl JobName {
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PodName(String);

impl PodName {
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SecretName(String);

impl SecretName {
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct JobUid(String);

impl JobUid {
    pub fn new(uid: impl Into<String>) -> Self {
        Self(uid.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatedJob {
    pub name: JobName,
    pub uid: JobUid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobPhase {
    Running,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobState {
    pub name: JobName,
    pub uid: JobUid,
    pub labels: BTreeMap<String, String>,
    pub phase: JobPhase,
    pub failed_reason: Option<String>,
}

impl JobState {
    pub fn is_running(&self) -> bool {
        self.phase == JobPhase::Running
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PodState {
    pub name: PodName,
    pub started: bool,
    pub reason: Option<String>,
    pub exit_code: Option<i32>,
    pub pod_reason: Option<String>,
}

pub type LogReader = Pin<Box<dyn AsyncBufRead + Send>>;

#[derive(Debug, thiserror::Error)]
pub enum ClusterError {
    #[error("the Job has no name")]
    NoName,
    #[error("the Job {} came back without a uid", .0.as_str())]
    NoUid(JobName),
    #[error("the Job {} exists", .0.as_str())]
    Exists(JobName),
    #[error("the pod {} is gone", .0.as_str())]
    NoPod(PodName),
    #[error("kubernetes: {0}")]
    Kube(#[source] kube::Error),
}
