use axum::Router;
use axum::http::{Method, Uri};
use axum::routing::get;

use super::AppState;
use super::error::HttpError;
use super::health::health;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .fallback(no_route)
        .with_state(state)
}

async fn no_route(method: Method, uri: Uri) -> HttpError {
    HttpError::NoRoute {
        method,
        path: uri.path().to_owned(),
    }
}
