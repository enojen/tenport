use axum::http::{Method, Uri};
use axum::routing::get;
use axum::{Json, Router};
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use super::AppState;
use super::error::HttpError;
use super::health;
use super::openapi::ApiDoc;

pub fn router(state: AppState) -> Router {
    let (router, api) = OpenApiRouter::with_openapi(ApiDoc::openapi())
        .routes(routes!(health::health))
        .split_for_parts();
    router
        .route("/openapi.json", get(move || async move { Json(api) }))
        .fallback(no_route)
        .with_state(state)
}

async fn no_route(method: Method, uri: Uri) -> HttpError {
    HttpError::NoRoute {
        method,
        path: uri.path().to_owned(),
    }
}
