use axum::Json;
use axum::extract::State;
use serde::Serialize;
use utoipa::ToSchema;

use super::AppState;
use super::auth::CurrentUser;
use super::error::{HttpError, PROBLEM_JSON, Problem};
use crate::accounts::{self, Role};

#[derive(Serialize, ToSchema)]
pub struct Me {
    id: String,
    name: String,
    email: Option<String>,
    github: String,
    organizations: Vec<MyOrganization>,
}

#[derive(Serialize, ToSchema)]
pub struct MyOrganization {
    id: String,
    name: String,
    role: Role,
}

#[utoipa::path(
    get,
    path = "/me",
    security(("bearer" = [])),
    responses(
        (status = OK, description = "The user of the token and their organizations", body = Me),
        (status = UNAUTHORIZED, description = "No token, or a token Tenport does not know", body = Problem, content_type = PROBLEM_JSON),
    ),
)]
pub async fn me(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> Result<Json<Me>, HttpError> {
    let memberships = accounts::memberships(&state.db, &user.id)
        .await
        .map_err(HttpError::internal)?;
    let organizations = memberships
        .into_iter()
        .map(|membership| MyOrganization {
            id: membership.org.to_string(),
            name: membership.org_name.into(),
            role: membership.role,
        })
        .collect();
    Ok(Json(Me {
        id: user.id.to_string(),
        name: user.name.into(),
        email: user.email.map(Into::into),
        github: user.github.into(),
        organizations,
    }))
}
