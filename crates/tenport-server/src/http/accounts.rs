use axum::extract::State;
use serde::Serialize;
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use super::AppState;
use super::auth::CurrentUser;
use super::error::HttpError;
use super::extract::Json;
use crate::accounts::{self, Role};

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(me))
}

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
            id: membership.organization.to_string(),
            name: membership.organization_name.into(),
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
