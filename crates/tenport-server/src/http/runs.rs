use std::collections::BTreeMap;

use axum::body::{Body, Bytes};
use axum::extract::{OriginalUri, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use futures::{AsyncBufReadExt, Stream, stream};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use super::AppState;
use super::auth::CurrentUser;
use super::error::{HttpError, NotMember, PROBLEM_JSON, Problem, RunNotFound};
use super::extract::{Json, Path, Query};
use super::limits::LogStreamPermit;
use super::page::{Page, PageQuery, Paged};
use crate::accounts::Email;
use crate::agents::AgentId;
use crate::cluster::LogReader;
use crate::name::Name;
use crate::projects::ProjectId;
use crate::runs::{self, Branch, ClassName, Command, RunName, RunPhase, RunsError, ServiceName};
use crate::tasks::TaskId;

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(start_run, list_runs))
        .routes(routes!(find_run, stop_run))
        .routes(routes!(read_log))
}

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
    #[schema(value_type = Option<String>, example = "a-k3x9qa")]
    agent: Option<AgentId>,
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
        (status = BAD_REQUEST, description = "The body is not a valid run: an unknown command, class or service; or the user has no agent (code no-agent)", body = Problem, content_type = PROBLEM_JSON),
        (status = FORBIDDEN, response = NotMember),
        (status = NOT_FOUND, description = "The run names an agent the user does not have", body = Problem, content_type = PROBLEM_JSON),
        (status = CONFLICT, description = "A Job of the task still runs (code machine-busy)", body = Problem, content_type = PROBLEM_JSON),
    ),
)]
pub async fn start_run(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Json(body): Json<NewRun>,
) -> Result<(StatusCode, Json<Run>), HttpError> {
    let new = new_run(body)?;
    let start = async move { state.runs.start(&state.db, &user.id, new).await };
    let run = tokio::spawn(start).await.map_err(HttpError::internal)??;
    Ok((StatusCode::CREATED, Json(run.into())))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct TaskRuns {
    #[param(value_type = String)]
    project: ProjectId,
    #[param(value_type = String, example = "TEN-301")]
    task: TaskId,
}

#[derive(Serialize, ToSchema)]
pub struct RunState {
    #[schema(example = "task-ten-301-1117-101500")]
    name: String,
    project: String,
    #[schema(example = "TEN-301")]
    task: String,
    #[schema(example = "build")]
    command: String,
    #[schema(example = "build")]
    class: String,
    #[schema(example = "a-k3x9qa")]
    agent: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    created: OffsetDateTime,
    phase: RunPhase,
    #[schema(example = "OOMKilled")]
    reason: Option<String>,
    #[schema(example = 0)]
    exit_code: Option<i32>,
    #[schema(example = 0)]
    oom_kill: Option<u64>,
    #[schema(value_type = Option<Object>, example = json!({ "ok": true, "task": "TEN-301" }))]
    output: Option<serde_json::Value>,
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct LogQuery {
    #[serde(default)]
    follow: bool,
}

#[utoipa::path(
    get,
    path = "/runs",
    security(("bearer" = [])),
    params(TaskRuns, PageQuery),
    responses(
        (status = OK, description = "The runs of the task, the oldest first; a Link header with rel=\"next\" gives the next page", body = Vec<RunState>,
            headers(("Link" = String, description = "The next page, when there is one"))),
        (status = BAD_REQUEST, description = "No project or task, an id that is not valid, or a limit out of 1 to 100", body = Problem, content_type = PROBLEM_JSON),
        (status = FORBIDDEN, response = NotMember),
    ),
)]
pub async fn list_runs(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    OriginalUri(uri): OriginalUri,
    Query(query): Query<TaskRuns>,
    Query(page): Query<PageQuery>,
) -> Result<Paged<RunState>, HttpError> {
    let page = Page::try_from(page)?;
    let after = page.parse_after(RunName::parse)?;
    let limit = page.rows_to_read();
    let found = state
        .runs
        .list_of_task(
            &state.db,
            &user.id,
            &query.project,
            &query.task,
            after.as_ref(),
            limit,
        )
        .await?;
    let runs = found.into_iter().map(RunState::from).collect();
    Ok(Paged::new(runs, &page, &uri, |run| run.name.clone()))
}

#[utoipa::path(
    get,
    path = "/runs/{name}",
    security(("bearer" = [])),
    params(("name" = String, Path, description = "The name of the run, the name of its Job")),
    responses(
        (status = OK, description = "The run, with the phase of its Job", body = RunState),
        (status = FORBIDDEN, response = NotMember),
        (status = NOT_FOUND, response = RunNotFound),
    ),
)]
pub async fn find_run(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(name): Path<RunName>,
) -> Result<Json<RunState>, HttpError> {
    let run = state.runs.find(&state.db, &user.id, &name).await?;
    Ok(Json(run.into()))
}

#[utoipa::path(
    get,
    path = "/runs/{name}/log",
    security(("bearer" = [])),
    params(
        ("name" = String, Path, description = "The name of the run, the name of its Job"),
        LogQuery,
    ),
    responses(
        (status = OK, description = "The log of the box container; with follow=true it stays open until the container ends", body = String, content_type = "text/plain"),
        (status = FORBIDDEN, response = NotMember),
        (status = NOT_FOUND, response = RunNotFound),
        (status = CONFLICT, description = "The box container has not started (code box-not-started; ask again after Retry-After seconds), or the Job is gone (code job-gone)", body = Problem, content_type = PROBLEM_JSON,
            headers(("Retry-After" = u32, description = "Seconds to wait before asking again; only with box-not-started"))),
    ),
)]
pub async fn read_log(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(name): Path<RunName>,
    Query(query): Query<LogQuery>,
) -> Result<Response, HttpError> {
    let permit = state.limiter.open_log_stream(&user.id)?;
    let reader = state
        .runs
        .open_log(&state.db, &user.id, &name, query.follow)
        .await?;
    let headers = [(header::CONTENT_TYPE, TEXT_PLAIN)];
    Ok((headers, Body::from_stream(log_chunks(reader, permit))).into_response())
}

#[utoipa::path(
    delete,
    path = "/runs/{name}",
    security(("bearer" = [])),
    params(("name" = String, Path, description = "The name of the run, the name of its Job")),
    responses(
        (status = NO_CONTENT, description = "The run is stopped: its Job and pod are deleted, or were gone"),
        (status = FORBIDDEN, response = NotMember),
        (status = NOT_FOUND, response = RunNotFound),
    ),
)]
pub async fn stop_run(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(name): Path<RunName>,
) -> Result<StatusCode, HttpError> {
    state.runs.stop(&state.db, &user.id, &name).await?;
    Ok(StatusCode::NO_CONTENT)
}

const TEXT_PLAIN: &str = "text/plain; charset=utf-8";

fn log_chunks(
    reader: LogReader,
    permit: LogStreamPermit,
) -> impl Stream<Item = std::io::Result<Bytes>> {
    stream::try_unfold((reader, permit), |(mut reader, permit)| async move {
        let chunk = Bytes::copy_from_slice(reader.fill_buf().await?);
        reader.as_mut().consume(chunk.len());
        Ok((!chunk.is_empty()).then_some((chunk, (reader, permit))))
    })
}

impl From<RunsError> for HttpError {
    fn from(error: RunsError) -> Self {
        match error {
            RunsError::Projects(error) => error.into(),
            RunsError::Agents(error) => error.into(),
            RunsError::NotFound { .. } => Self::NotFound(error.to_string()),
            RunsError::NotStarted { .. } => Self::BoxNotStarted(error.to_string()),
            RunsError::JobGone { .. } => Self::JobGone(error.to_string()),
            RunsError::Busy { .. } => Self::MachineBusy(error.to_string()),
            RunsError::Template(
                runs::TemplateError::NoClass { .. } | runs::TemplateError::NoService { .. },
            ) => Self::BadRequest(error.to_string()),
            other => Self::internal(other),
        }
    }
}

impl From<runs::RunState> for RunState {
    fn from(state: runs::RunState) -> Self {
        let run = state.run;
        Self {
            name: run.name.to_string(),
            project: run.project.to_string(),
            task: run.task.to_string(),
            command: run.command.as_str().to_owned(),
            class: run.class.as_str().to_owned(),
            agent: run.agent.map(|agent| agent.to_string()),
            created: run.created,
            phase: state.phase,
            reason: state.reason,
            exit_code: state.exit_code,
            oom_kill: state.oom_kill,
            output: state.output,
        }
    }
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
        agent: body.agent,
    })
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
