use axum::extract::State;
use serde::Serialize;
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use super::AppState;
use super::error::{HttpError, Unavailable};
use super::extract::Json;
use crate::db;

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(health))
}

#[derive(Serialize, ToSchema)]
pub struct Health {
    ok: bool,
}

#[utoipa::path(
    get,
    path = "/health",
    responses(
        (status = OK, description = "The database answers", body = Health),
        (status = SERVICE_UNAVAILABLE, response = Unavailable),
    ),
)]
pub async fn health(State(state): State<AppState>) -> Result<Json<Health>, HttpError> {
    db::ping(&state.db).await.map_err(HttpError::DatabaseDown)?;
    Ok(Json(Health { ok: true }))
}
