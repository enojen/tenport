use axum::extract::State;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use super::AppState;
use super::auth::CurrentUser;
use super::error::{HttpError, NotMember, PROBLEM_JSON, Problem};
use super::extract::{Json, Path, Query};
use crate::events::{EventId, EventsError};
use crate::projects::ProjectId;
use crate::task_archives::TaskArchiveState;
use crate::tasks::{self, Reported, TaskId, TasksError, TasksEvent};

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(delete_task))
        .routes(routes!(record_events))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct TaskProject {
    #[param(value_type = String)]
    project: ProjectId,
}

#[derive(Serialize, ToSchema)]
pub struct DeletedTask {
    project: String,
    #[schema(example = "TEN-301")]
    task: String,
    archive: TaskArchiveState,
}

#[utoipa::path(
    delete,
    path = "/tasks/{task}",
    security(("bearer" = [])),
    params(("task" = String, Path, description = "The task id, such as TEN-301"), TaskProject),
    responses(
        (status = OK, description = "The task is deleted. `archive` is `deleted`, or `missing` when the bucket had no task archive, so a second delete is safe", body = DeletedTask),
        (status = BAD_REQUEST, description = "Not a task id or a project id", body = Problem, content_type = PROBLEM_JSON),
        (status = FORBIDDEN, response = NotMember),
        (status = CONFLICT, description = "A Job of the task still runs (code machine-busy); nothing is deleted", body = Problem, content_type = PROBLEM_JSON),
    ),
)]
pub async fn delete_task(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(task): Path<TaskId>,
    Query(query): Query<TaskProject>,
) -> Result<Json<DeletedTask>, HttpError> {
    let archive = tasks::delete_task(
        &state.db,
        &state.runs,
        &state.task_archives,
        &user.id,
        &query.project,
        &task,
    )
    .await?;
    Ok(Json(DeletedTask {
        project: query.project.to_string(),
        task: task.to_string(),
        archive,
    }))
}

#[derive(Deserialize, ToSchema)]
pub struct ReportedEvent {
    #[schema(value_type = String, example = "019a8f2e-7c4b-7d21-9a3e-5b8c1d2e3f40")]
    id: EventId,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    at: OffsetDateTime,
    #[serde(flatten)]
    event: TasksEvent,
}

#[derive(Serialize, ToSchema)]
pub struct Recorded {
    stored: usize,
    skipped: usize,
}

#[utoipa::path(
    post,
    path = "/events",
    security(("bearer" = [])),
    request_body = Vec<ReportedEvent>,
    responses(
        (status = OK, description = "The new events are stored; an id Tenport has is skipped", body = Recorded),
        (status = BAD_REQUEST, description = "The body is not a list of valid events", body = Problem, content_type = PROBLEM_JSON),
        (status = FORBIDDEN, response = NotMember),
        (status = CONFLICT, description = "An id came before with other content, or an event does not fit the state; nothing is stored", body = Problem, content_type = PROBLEM_JSON),
    ),
)]
pub async fn record_events(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Json(body): Json<Vec<ReportedEvent>>,
) -> Result<Json<Recorded>, HttpError> {
    let reported = body
        .into_iter()
        .map(|event| Reported {
            id: event.id,
            at: event.at,
            event: event.event,
        })
        .collect();
    let recorded = tasks::record_events(&state.db, &user.id, reported).await?;
    Ok(Json(Recorded {
        stored: recorded.stored,
        skipped: recorded.skipped,
    }))
}

impl From<TasksError> for HttpError {
    fn from(error: TasksError) -> Self {
        match error {
            TasksError::Projects(error) => error.into(),
            TasksError::Runs(error) => error.into(),
            TasksError::EventNotAllowed { .. } => Self::BadRequest(error.to_string()),
            TasksError::Events(EventsError::Conflict { .. } | EventsError::Refused { .. }) => {
                Self::Conflict(error.to_string())
            }
            other => Self::internal(other),
        }
    }
}
