use std::collections::BTreeMap;

use axum::Json;
use axum::extract::State;
use axum::extract::rejection::JsonRejection;
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::AppState;
use super::auth::CurrentUser;
use super::error::{HttpError, PROBLEM_JSON, Problem};
use crate::accounts::Email;
use crate::name::Name;
use crate::projects::ProjectId;
use crate::runs::{self, Branch, ClassName, Command, RunsError, ServiceName};
use crate::tasks::TaskId;

#[derive(Deserialize, ToSchema)]
pub struct NewRun {
    #[schema(value_type = String)]
    project: ProjectId,
    #[schema(value_type = String, example = "TEN-301")]
    task: TaskId,
    #[schema(example = json!(["tenbox", "build", "TEN-301"]))]
    command: Vec<String>,
    #[schema(value_type = String, example = "main")]
    base: Branch,
    #[schema(value_type = String, example = "build")]
    class: ClassName,
    #[serde(default)]
    env: BTreeMap<String, String>,
    #[serde(default)]
    setup: String,
    #[serde(default)]
    #[schema(value_type = Vec<String>, example = json!(["postgres"]))]
    services: Vec<ServiceName>,
    author: Author,
}

#[derive(Deserialize, ToSchema)]
pub struct Author {
    #[schema(value_type = String)]
    name: Name,
    #[schema(value_type = String, example = "1+enojen@users.noreply.github.com")]
    email: Email,
}

#[derive(Serialize, ToSchema)]
pub struct Run {
    #[schema(example = "task-ten-301-1117-101500")]
    name: String,
    project: String,
    #[schema(example = "TEN-301")]
    task: String,
}

#[utoipa::path(
    post,
    path = "/runs",
    security(("bearer" = [])),
    request_body(content = NewRun, description = "One tenbox command to run in a box"),
    responses(
        (status = CREATED, description = "The Job of the run is created", body = Run),
        (status = BAD_REQUEST, description = "The body is not a valid run: an unknown command, class or service", body = Problem, content_type = PROBLEM_JSON),
        (status = UNAUTHORIZED, description = "No token, or a token Tenport does not know", body = Problem, content_type = PROBLEM_JSON),
        (status = FORBIDDEN, description = "The user is not a member of the project's organization", body = Problem, content_type = PROBLEM_JSON),
        (status = CONFLICT, description = "A Job of the task still runs (code machine-busy)", body = Problem, content_type = PROBLEM_JSON),
    ),
)]
pub async fn start_run(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    body: Result<Json<NewRun>, JsonRejection>,
) -> Result<(StatusCode, Json<Run>), HttpError> {
    let Json(body) = body?;
    let new = new_run(body)?;
    let run = state
        .runs
        .start(&state.db, &user.id, new)
        .await
        .map_err(start_error)?;
    Ok((StatusCode::CREATED, Json(run.into())))
}

fn new_run(body: NewRun) -> Result<runs::NewRun, HttpError> {
    let command = Command::parse(body.command, &body.task)
        .map_err(|error| HttpError::BadRequest(format!("command: {error}")))?;
    Ok(runs::NewRun {
        project: body.project,
        task: body.task,
        command,
        base: body.base,
        class: body.class,
        env: body.env,
        setup: body.setup,
        services: body.services,
        author: runs::Author {
            name: body.author.name,
            email: body.author.email,
        },
    })
}

fn start_error(error: RunsError) -> HttpError {
    match error {
        RunsError::NotMember { .. } => HttpError::Forbidden(error.to_string()),
        RunsError::Busy { .. } => HttpError::MachineBusy(error.to_string()),
        RunsError::Template(
            runs::TemplateError::NoClass { .. } | runs::TemplateError::NoService { .. },
        ) => HttpError::BadRequest(error.to_string()),
        other => HttpError::internal(other),
    }
}

impl From<runs::Run> for Run {
    fn from(run: runs::Run) -> Self {
        Self {
            name: run.name.to_string(),
            project: run.project.to_string(),
            task: run.task.to_string(),
        }
    }
}
