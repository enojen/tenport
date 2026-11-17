use std::collections::BTreeMap;
use std::pin::Pin;

use futures::AsyncBufRead;
use time::OffsetDateTime;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobPhase {
    Running,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobState {
    pub name: JobName,
    pub labels: BTreeMap<String, String>,
    pub created: OffsetDateTime,
    pub phase: JobPhase,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PodState {
    pub name: PodName,
    pub started: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LogRead {
    pub follow: bool,
    pub tail_lines: Option<i64>,
}

pub type LogReader = Pin<Box<dyn AsyncBufRead + Send>>;

#[derive(Debug, thiserror::Error)]
pub enum ClusterError {
    #[error("the Job has no name")]
    NoName,
    #[error("the Job {} exists", .0.as_str())]
    Exists(JobName),
    #[error("the pod {} is gone", .0.as_str())]
    NoPod(PodName),
    #[error("kubernetes: {0}")]
    Kube(#[source] kube::Error),
}
