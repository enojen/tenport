use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::name::{Name, parsed_text};
use crate::projects::ProjectId;

const URL_SCHEMES: [&str; 2] = ["https://", "http://"];

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[serde(try_from = "String", into = "String")]
#[sqlx(transparent)]
pub struct TaskId(String);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("a task id starts with a letter or a digit, then has letters, digits, _ and - only")]
pub struct TaskIdError;

impl TaskId {
    pub fn parse(raw: &str) -> Result<Self, TaskIdError> {
        let mut chars = raw.chars();
        let starts_well = chars.next().is_some_and(|c| c.is_ascii_alphanumeric());
        let rest_is_safe = chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
        if starts_well && rest_is_safe {
            Ok(Self(raw.to_owned()))
        } else {
            Err(TaskIdError)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[serde(try_from = "String", into = "String")]
#[sqlx(transparent)]
pub struct TrackerUrl(String);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("a tracker link starts with https:// or http:// and has no spaces")]
pub struct TrackerUrlError;

impl TrackerUrl {
    pub fn parse(raw: &str) -> Result<Self, TrackerUrlError> {
        let url = raw.trim();
        let has_scheme = URL_SCHEMES
            .iter()
            .any(|scheme| url.len() > scheme.len() && url.starts_with(scheme));
        if has_scheme && !url.contains(char::is_whitespace) {
            Ok(Self(url.to_owned()))
        } else {
            Err(TrackerUrlError)
        }
    }
}

parsed_text!(TaskId, TaskIdError);
parsed_text!(TrackerUrl, TrackerUrlError);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type")]
pub enum LanesEvent {
    #[serde(rename = "task.started")]
    TaskStarted(TaskStarted),
    #[serde(rename = "task.closed")]
    TaskClosed(TaskClosed),
}

impl LanesEvent {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::TaskStarted(_) => "task.started",
            Self::TaskClosed(_) => "task.closed",
        }
    }

    pub fn subject(&self) -> String {
        format!("task:{}/{}", self.project(), self.task())
    }

    pub fn project(&self) -> &ProjectId {
        match self {
            Self::TaskStarted(started) => &started.project,
            Self::TaskClosed(closed) => &closed.project,
        }
    }

    fn task(&self) -> &TaskId {
        match self {
            Self::TaskStarted(started) => &started.task,
            Self::TaskClosed(closed) => &closed.task,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct TaskStarted {
    #[schema(value_type = String)]
    pub project: ProjectId,
    #[schema(value_type = String, example = "TEN-301")]
    pub task: TaskId,
    #[serde(default)]
    #[schema(value_type = Option<String>)]
    pub title: Option<Name>,
    #[serde(default)]
    #[schema(value_type = Option<String>)]
    pub url: Option<TrackerUrl>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct TaskClosed {
    #[schema(value_type = String)]
    pub project: ProjectId,
    #[schema(value_type = String, example = "TEN-301")]
    pub task: TaskId,
}
