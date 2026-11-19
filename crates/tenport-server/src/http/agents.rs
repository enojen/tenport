use axum::extract::State;
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use super::AppState;
use super::auth::CurrentUser;
use super::error::{HttpError, PROBLEM_JSON, Problem};
use super::extract::{Json, Path};
use crate::agents::{self, AgentId, AgentKind, AgentLogin, AgentsError, LoginState};
use crate::name::Name;

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(add_agent, list_agents))
        .routes(routes!(write_login))
}

#[derive(Deserialize, ToSchema)]
pub struct NewAgent {
    kind: AgentKind,
    #[schema(value_type = String, example = "personal")]
    name: Name,
}

#[derive(Serialize, ToSchema)]
pub struct Agent {
    #[schema(example = "a-k3x9qa")]
    id: String,
    kind: AgentKind,
    #[schema(example = "personal")]
    name: String,
    default: bool,
    login: Option<Login>,
}

#[derive(Serialize, ToSchema)]
pub struct Login {
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    changed: OffsetDateTime,
}

impl From<agents::Agent> for Agent {
    fn from(agent: agents::Agent) -> Self {
        Self {
            id: agent.id.to_string(),
            kind: agent.kind,
            name: agent.name.into(),
            default: agent.default,
            login: agent.login_changed.map(|changed| Login { changed }),
        }
    }
}

#[utoipa::path(
    post,
    path = "/agents",
    security(("bearer" = [])),
    request_body = NewAgent,
    responses(
        (status = CREATED, description = "The agent is added; the user's first agent is their default", body = Agent),
        (status = BAD_REQUEST, description = "A kind Tenport does not know, or a name that is not valid", body = Problem, content_type = PROBLEM_JSON),
        (status = CONFLICT, description = "The user has an agent with this name already", body = Problem, content_type = PROBLEM_JSON),
    ),
)]
pub async fn add_agent(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Json(body): Json<NewAgent>,
) -> Result<(StatusCode, Json<Agent>), HttpError> {
    let agent = agents::add(&state.db, &user.id, body.kind, body.name).await?;
    Ok((StatusCode::CREATED, Json(agent.into())))
}

#[utoipa::path(
    get,
    path = "/agents",
    security(("bearer" = [])),
    responses(
        (status = OK, description = "The user's own agents, the oldest first: never their logins", body = Vec<Agent>),
    ),
)]
pub async fn list_agents(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> Result<Json<Vec<Agent>>, HttpError> {
    let found = agents::list_of_user(&state.db, &user.id).await?;
    Ok(Json(found.into_iter().map(Agent::from).collect()))
}

#[derive(Deserialize, ToSchema)]
pub struct NewLogin {
    #[schema(value_type = String, min_length = 1, max_length = 16384, example = "{\"claudeAiOauth\":{}}")]
    value: AgentLogin,
}

#[derive(Serialize, ToSchema)]
pub struct WrittenLogin {
    #[schema(example = "a-k3x9qa")]
    agent: String,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    changed: OffsetDateTime,
}

impl From<LoginState> for WrittenLogin {
    fn from(state: LoginState) -> Self {
        Self {
            agent: state.agent.to_string(),
            changed: state.changed,
        }
    }
}

#[utoipa::path(
    put,
    path = "/agents/{agent}/login",
    security(("bearer" = [])),
    params(("agent" = String, Path, description = "The id of the agent")),
    request_body(content = NewLogin, description = "The text of the agent's credentials file, such as ~/.claude/.credentials.json"),
    responses(
        (status = OK, description = "The login is written to the Secret agent-<agent id>; Tenport keeps no copy and never gives it back", body = WrittenLogin),
        (status = BAD_REQUEST, description = "A value that is not a JSON object, or is too long", body = Problem, content_type = PROBLEM_JSON),
        (status = NOT_FOUND, description = "The user has no such agent", body = Problem, content_type = PROBLEM_JSON),
    ),
)]
pub async fn write_login(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(agent): Path<AgentId>,
    Json(body): Json<NewLogin>,
) -> Result<Json<WrittenLogin>, HttpError> {
    let cluster = state.cluster.as_ref();
    let written = agents::write_login(&state.db, cluster, &user.id, &agent, &body.value).await?;
    Ok(Json(written.into()))
}

impl From<AgentsError> for HttpError {
    fn from(error: AgentsError) -> Self {
        match error {
            AgentsError::NameTaken { .. } => Self::Conflict(error.to_string()),
            AgentsError::NotFound { .. } => Self::NotFound(error.to_string()),
            AgentsError::NoAgent => Self::NoAgent(error.to_string()),
            other => Self::internal(other),
        }
    }
}
