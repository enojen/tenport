use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;

use crate::cluster::{JobName, JobPhase};
use crate::name::parsed_text;
use crate::projects::ProjectId;
use crate::tasks::TaskId;

use super::command::CommandName;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[serde(transparent)]
#[sqlx(transparent)]
pub struct RunName(String);

impl RunName {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<JobName> for RunName {
    fn from(job: JobName) -> Self {
        Self(job.as_str().to_owned())
    }
}

impl std::fmt::Display for RunName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

macro_rules! dns_label {
    ($name:ident, $error:ident, $message:literal) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
        #[serde(try_from = "String", into = "String")]
        #[sqlx(transparent)]
        pub struct $name(String);

        #[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
        #[error($message)]
        pub struct $error;

        impl $name {
            pub fn parse(raw: &str) -> Result<Self, $error> {
                if is_dns_label(raw) {
                    Ok(Self(raw.to_owned()))
                } else {
                    Err($error)
                }
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        parsed_text!($name, $error);
    };
}

dns_label!(
    ClassName,
    ClassNameError,
    "a class is lower-case letters, digits and -, as in classes.yaml"
);
dns_label!(
    ServiceName,
    ServiceNameError,
    "a service is lower-case letters, digits and -, as its file in services/"
);

fn is_dns_label(raw: &str) -> bool {
    let starts_well = raw.chars().next().is_some_and(|c| c.is_ascii_lowercase());
    starts_well
        && raw
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Branch(String);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("a branch is letters, digits, and . _ / - only, and does not start with -")]
pub struct BranchError;

impl Branch {
    pub fn parse(raw: &str) -> Result<Self, BranchError> {
        let starts_well = raw.chars().next().is_some_and(|c| c != '-');
        let is_safe = raw
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '/' | '-'));
        if starts_well && is_safe {
            Ok(Self(raw.to_owned()))
        } else {
            Err(BranchError)
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

parsed_text!(Branch, BranchError);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum RunsEvent {
    #[serde(rename = "run.started")]
    RunStarted(RunStarted),
}

impl RunsEvent {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::RunStarted(_) => "run.started",
        }
    }

    pub fn subject(&self) -> String {
        match self {
            Self::RunStarted(started) => format!("run:{}", started.run),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunStarted {
    pub project: ProjectId,
    pub task: TaskId,
    pub run: RunName,
    pub command: CommandName,
    pub class: ClassName,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredRun {
    pub name: RunName,
    pub project: ProjectId,
    pub task: TaskId,
    pub command: CommandName,
    pub class: ClassName,
    pub created: OffsetDateTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
pub enum RunPhase {
    Running,
    Succeeded,
    Failed,
    Unknown,
}

impl From<JobPhase> for RunPhase {
    fn from(phase: JobPhase) -> Self {
        match phase {
            JobPhase::Running => Self::Running,
            JobPhase::Succeeded => Self::Succeeded,
            JobPhase::Failed => Self::Failed,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunState {
    pub run: StoredRun,
    pub phase: RunPhase,
    pub reason: Option<String>,
}
