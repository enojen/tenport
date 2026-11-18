use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::name::{Name, parsed_text};
use crate::projects::ProjectId;
use crate::tracker::TrackerUrl;

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

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

parsed_text!(TaskId, TaskIdError);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type")]
pub enum TasksEvent {
    #[serde(rename = "task.started")]
    TaskStarted(TaskStarted),
    #[serde(rename = "task.closed")]
    TaskClosed(TaskClosed),
    #[serde(rename = "task.deleted")]
    TaskDeleted(TaskDeleted),
}

impl TasksEvent {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::TaskStarted(_) => "task.started",
            Self::TaskClosed(_) => "task.closed",
            Self::TaskDeleted(_) => "task.deleted",
        }
    }

    pub fn subject(&self) -> String {
        format!("task:{}/{}", self.project(), self.task())
    }

    pub fn project(&self) -> &ProjectId {
        match self {
            Self::TaskStarted(started) => &started.project,
            Self::TaskClosed(closed) => &closed.project,
            Self::TaskDeleted(deleted) => &deleted.project,
        }
    }

    fn task(&self) -> &TaskId {
        match self {
            Self::TaskStarted(started) => &started.task,
            Self::TaskClosed(closed) => &closed.task,
            Self::TaskDeleted(deleted) => &deleted.task,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct TaskDeleted {
    #[schema(value_type = String)]
    pub project: ProjectId,
    #[schema(value_type = String, example = "TEN-301")]
    pub task: TaskId,
}
