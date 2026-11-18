use axum::extract::State;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::AppState;
use super::auth::Injector;
use super::error::{HttpError, PROBLEM_JSON, Problem, TooManyRequests, Unavailable};
use super::extract::Json;
use crate::github::GithubError;
use crate::runs::RunsError;

#[derive(Deserialize, ToSchema)]
pub struct RunCredentialRequest {
    #[schema(example = "tbxph_ETORSF6tK5Aq-PPCa0hncaILEOC-43LTqxG9R15MW38")]
    placeholder: String,
    #[schema(example = "api.github.com")]
    host: String,
}

#[derive(Serialize, ToSchema)]
pub struct RunCredential {
    value: String,
}

#[utoipa::path(
    post,
    path = "/run-credentials",
    security(("injector" = [])),
    request_body(content = RunCredentialRequest, description = "A placeholder that a box sent, and the host it sent it to"),
    responses(
        (status = OK, description = "The one-hour GitHub App token for the repository of the open Job that has this placeholder", body = RunCredential),
        (status = BAD_REQUEST, description = "The body is not a placeholder and a host", body = Problem, content_type = PROBLEM_JSON),
        (status = UNAUTHORIZED, description = "No injector token, or another one", body = Problem, content_type = PROBLEM_JSON),
        (status = TOO_MANY_REQUESTS, response = TooManyRequests),
        (status = SERVICE_UNAVAILABLE, response = Unavailable),
        (status = NOT_FOUND, description = "No open Job of Tenport has this placeholder, the host is not GitHub, or the GitHub App is not installed on the repository", body = Problem, content_type = PROBLEM_JSON),
    ),
)]
pub async fn run_credential(
    State(state): State<AppState>,
    _injector: Injector,
    Json(body): Json<RunCredentialRequest>,
) -> Result<Json<RunCredential>, HttpError> {
    let value = state
        .runs
        .run_credential(&state.db, &body.placeholder, &body.host)
        .await
        .map_err(run_credential_error)?;
    Ok(Json(RunCredential { value }))
}

fn run_credential_error(error: RunsError) -> HttpError {
    match error {
        RunsError::NoRunCredential { .. } | RunsError::Github(GithubError::NotInstalled { .. }) => {
            HttpError::NotFound(error.to_string())
        }
        other => HttpError::internal(other),
    }
}
