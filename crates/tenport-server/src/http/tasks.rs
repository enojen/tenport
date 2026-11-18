use axum::extract::State;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use super::AppState;
use super::auth::CurrentUser;
use super::error::{
    HttpError, NotMember, PROBLEM_JSON, Problem, TooManyRequests, Unauthorized, Unavailable,
};
use super::extract::{Json, Path, Query};
use crate::projects::ProjectId;
use crate::runs::RunsError;
use crate::task_archives::TaskArchiveState;
use crate::tasks::{self, TaskId, TasksError};

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
        (status = UNAUTHORIZED, response = Unauthorized),
        (status = TOO_MANY_REQUESTS, response = TooManyRequests),
        (status = SERVICE_UNAVAILABLE, response = Unavailable),
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
    .await
    .map_err(delete_error)?;
    Ok(Json(DeletedTask {
        project: query.project.to_string(),
        task: task.to_string(),
        archive,
    }))
}

fn delete_error(error: TasksError) -> HttpError {
    match error {
        TasksError::Projects(error) => error.into(),
        TasksError::Runs(RunsError::Busy { .. }) => HttpError::MachineBusy(error.to_string()),
        other => HttpError::internal(other),
    }
}
