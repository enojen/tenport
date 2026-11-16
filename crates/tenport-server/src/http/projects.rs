use axum::Json;
use axum::extract::State;
use axum::extract::rejection::JsonRejection;
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::AppState;
use super::auth::CurrentUser;
use super::error::{HttpError, PROBLEM_JSON, Problem};
use crate::accounts::OrgId;
use crate::name::Name;
use crate::projects::{self, ProjectsError, Repo};

#[derive(Deserialize, ToSchema)]
pub struct NewProject {
    #[schema(value_type = String)]
    org: OrgId,
    #[schema(value_type = String)]
    name: Name,
    #[schema(value_type = String, example = "enojen/tenport")]
    repo: Repo,
}

#[derive(Serialize, ToSchema)]
pub struct Project {
    id: String,
    org: String,
    name: String,
    repo: String,
}

#[utoipa::path(
    post,
    path = "/projects",
    security(("bearer" = [])),
    request_body = NewProject,
    responses(
        (status = CREATED, description = "The project is added", body = Project),
        (status = BAD_REQUEST, description = "The body is not a valid new project", body = Problem, content_type = PROBLEM_JSON),
        (status = UNAUTHORIZED, description = "No token, or a token Tenport does not know", body = Problem, content_type = PROBLEM_JSON),
        (status = FORBIDDEN, description = "The user is not an admin of the organization", body = Problem, content_type = PROBLEM_JSON),
    ),
)]
pub async fn add_project(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    body: Result<Json<NewProject>, JsonRejection>,
) -> Result<(StatusCode, Json<Project>), HttpError> {
    let Json(body) = body?;
    let new = projects::NewProject {
        org: body.org,
        name: body.name,
        repo: body.repo,
    };
    let project = projects::add(&state.db, &user.id, new)
        .await
        .map_err(add_error)?;
    Ok((StatusCode::CREATED, Json(project.into())))
}

#[utoipa::path(
    get,
    path = "/projects",
    security(("bearer" = [])),
    responses(
        (status = OK, description = "The projects of the user's organizations", body = Vec<Project>),
        (status = UNAUTHORIZED, description = "No token, or a token Tenport does not know", body = Problem, content_type = PROBLEM_JSON),
    ),
)]
pub async fn list_projects(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> Result<Json<Vec<Project>>, HttpError> {
    let projects = projects::list_of_user(&state.db, &user.id)
        .await
        .map_err(HttpError::internal)?;
    Ok(Json(projects.into_iter().map(Into::into).collect()))
}

fn add_error(error: ProjectsError) -> HttpError {
    match error {
        ProjectsError::NotAdmin { .. } => HttpError::Forbidden(error.to_string()),
        other => HttpError::internal(other),
    }
}

impl From<projects::Project> for Project {
    fn from(project: projects::Project) -> Self {
        Self {
            id: project.id.to_string(),
            org: project.org.to_string(),
            name: project.name.into(),
            repo: project.repo.into(),
        }
    }
}
