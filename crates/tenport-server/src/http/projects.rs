use axum::extract::{OriginalUri, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::AppState;
use super::auth::CurrentUser;
use super::error::{
    HttpError, NotMember, PROBLEM_JSON, Problem, TooManyRequests, Unauthorized, Unavailable,
};
use super::extract::{Json, Path, Query};
use super::page::{Page, PageQuery, Paged};
use crate::accounts::OrganizationId;
use crate::name::Name;
use crate::projects::{self, ProjectId, ProjectSecret, Repo, SecretState, SecretValue};
use time::OffsetDateTime;

#[derive(Deserialize, ToSchema)]
pub struct NewProject {
    #[schema(value_type = String)]
    organization: OrganizationId,
    #[schema(value_type = String)]
    name: Name,
    #[schema(value_type = String, example = "enojen/tenport")]
    repo: Repo,
}

#[derive(Serialize, ToSchema)]
pub struct Project {
    id: String,
    organization: String,
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
        (status = UNAUTHORIZED, response = Unauthorized),
        (status = TOO_MANY_REQUESTS, response = TooManyRequests),
        (status = SERVICE_UNAVAILABLE, response = Unavailable),
        (status = FORBIDDEN, description = "The user is not an admin of the organization", body = Problem, content_type = PROBLEM_JSON),
    ),
)]
pub async fn add_project(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Json(body): Json<NewProject>,
) -> Result<(StatusCode, Json<Project>), HttpError> {
    let new = projects::NewProject {
        organization: body.organization,
        name: body.name,
        repo: body.repo,
    };
    let project = projects::add(&state.db, &user.id, new).await?;
    Ok((StatusCode::CREATED, Json(project.into())))
}

#[utoipa::path(
    get,
    path = "/projects",
    security(("bearer" = [])),
    params(PageQuery),
    responses(
        (status = OK, description = "The projects of the user's organizations, by name; a Link header with rel=\"next\" gives the next page", body = Vec<Project>,
            headers(("Link" = String, description = "The next page, when there is one"))),
        (status = BAD_REQUEST, description = "A limit out of 1 to 100, or an after that is not a project id", body = Problem, content_type = PROBLEM_JSON),
        (status = UNAUTHORIZED, response = Unauthorized),
        (status = TOO_MANY_REQUESTS, response = TooManyRequests),
        (status = SERVICE_UNAVAILABLE, response = Unavailable),
    ),
)]
pub async fn list_projects(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    OriginalUri(uri): OriginalUri,
    Query(query): Query<PageQuery>,
) -> Result<Paged<Project>, HttpError> {
    let page = Page::try_from(query)?;
    let after = page.after.as_deref().map(ProjectId::parse).transpose();
    let after = after.map_err(|error| HttpError::BadRequest(format!("after: {error}")))?;
    let found =
        projects::list_of_user(&state.db, &user.id, after.as_ref(), page.rows_to_read()).await?;
    let projects = found.into_iter().map(Project::from).collect();
    Ok(Paged::new(projects, &page, &uri, |project| {
        project.id.clone()
    }))
}

impl From<projects::Project> for Project {
    fn from(project: projects::Project) -> Self {
        Self {
            id: project.id.to_string(),
            organization: project.organization.to_string(),
            name: project.name.into(),
            repo: project.repo.into(),
        }
    }
}

#[derive(Deserialize, ToSchema)]
pub struct NewSecretValue {
    #[schema(value_type = String, min_length = 1, max_length = 4096)]
    value: SecretValue,
}

#[derive(Serialize, ToSchema)]
pub struct Secret {
    name: ProjectSecret,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    changed: OffsetDateTime,
}

impl From<SecretState> for Secret {
    fn from(state: SecretState) -> Self {
        Self {
            name: state.secret,
            changed: state.changed,
        }
    }
}

#[utoipa::path(
    put,
    path = "/projects/{project}/secrets/{name}",
    security(("bearer" = [])),
    params(
        ("project" = String, Path, description = "The id of the project"),
        ("name" = ProjectSecret, Path, description = "The name of the secret"),
    ),
    request_body = NewSecretValue,
    responses(
        (status = OK, description = "The secret is written to the Secret project-<project id>; Tenport keeps no copy and gives back only its name", body = Secret),
        (status = BAD_REQUEST, description = "A name Tenport does not know, or a value that is empty or too long", body = Problem, content_type = PROBLEM_JSON),
        (status = UNAUTHORIZED, response = Unauthorized),
        (status = TOO_MANY_REQUESTS, response = TooManyRequests),
        (status = SERVICE_UNAVAILABLE, response = Unavailable),
        (status = FORBIDDEN, description = "The user is not an admin of the project's organization", body = Problem, content_type = PROBLEM_JSON),
    ),
)]
pub async fn write_secret(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path((project, name)): Path<(ProjectId, ProjectSecret)>,
    Json(body): Json<NewSecretValue>,
) -> Result<Json<Secret>, HttpError> {
    let cluster = state.cluster.as_ref();
    let written =
        projects::write_secret(&state.db, cluster, &user.id, &project, name, &body.value).await?;
    Ok(Json(written.into()))
}

#[utoipa::path(
    delete,
    path = "/projects/{project}/secrets/{name}",
    security(("bearer" = [])),
    params(
        ("project" = String, Path, description = "The id of the project"),
        ("name" = ProjectSecret, Path, description = "The name of the secret"),
    ),
    responses(
        (status = NO_CONTENT, description = "The secret is deleted from the Secret project-<project id>"),
        (status = BAD_REQUEST, description = "A name Tenport does not know", body = Problem, content_type = PROBLEM_JSON),
        (status = UNAUTHORIZED, response = Unauthorized),
        (status = TOO_MANY_REQUESTS, response = TooManyRequests),
        (status = SERVICE_UNAVAILABLE, response = Unavailable),
        (status = FORBIDDEN, description = "The user is not an admin of the project's organization", body = Problem, content_type = PROBLEM_JSON),
        (status = NOT_FOUND, description = "The project has no such secret", body = Problem, content_type = PROBLEM_JSON),
    ),
)]
pub async fn delete_secret(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path((project, name)): Path<(ProjectId, ProjectSecret)>,
) -> Result<StatusCode, HttpError> {
    let cluster = state.cluster.as_ref();
    projects::delete_secret(&state.db, cluster, &user.id, &project, name).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/projects/{project}/secrets",
    security(("bearer" = [])),
    params(("project" = String, Path, description = "The id of the project")),
    responses(
        (status = OK, description = "The secrets that Tenport wrote for the project, by name: never their values", body = Vec<Secret>),
        (status = UNAUTHORIZED, response = Unauthorized),
        (status = TOO_MANY_REQUESTS, response = TooManyRequests),
        (status = SERVICE_UNAVAILABLE, response = Unavailable),
        (status = FORBIDDEN, response = NotMember),
    ),
)]
pub async fn list_secrets(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(project): Path<ProjectId>,
) -> Result<Json<Vec<Secret>>, HttpError> {
    let found = projects::list_secrets(&state.db, &user.id, &project).await?;
    Ok(Json(found.into_iter().map(Secret::from).collect()))
}
