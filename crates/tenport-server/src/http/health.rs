use axum::Json;
use axum::extract::State;
use serde::Serialize;

use super::AppState;
use super::error::HttpError;
use crate::db;

#[derive(Serialize)]
pub struct Health {
    ok: bool,
}

pub async fn health(State(state): State<AppState>) -> Result<Json<Health>, HttpError> {
    db::ping(&state.db).await.map_err(HttpError::DatabaseDown)?;
    Ok(Json(Health { ok: true }))
}
