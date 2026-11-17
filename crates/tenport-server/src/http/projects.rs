use axum::extract::{OriginalUri, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::AppState;
use super::auth::CurrentUser;
use super::error::{HttpError, PROBLEM_JSON, Problem, TooManyRequests, Unauthorized, Unavailable};
use super::extract::{Json, Query};
use super::page::{Page, PageQuery, Paged};
use crate::accounts::OrgId;
use crate::name::Name;
use crate::projects::{self, ProjectId, Repo};

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
        org: body.org,
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
            org: project.org.to_string(),
            name: project.name.into(),
            repo: project.repo.into(),
        }
    }
}
