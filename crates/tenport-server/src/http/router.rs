use axum::http::{Method, Uri};
use axum::routing::get;
use axum::{Json, Router};
use tower_http::trace::TraceLayer;
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use super::AppState;
use super::error::HttpError;
use super::openapi::ApiDoc;
use super::{accounts, health, projects};

pub fn router(state: AppState) -> Router {
    let (router, api) = OpenApiRouter::with_openapi(ApiDoc::openapi())
        .routes(routes!(health::health))
        .routes(routes!(accounts::me))
        .routes(routes!(projects::add_project, projects::list_projects))
        .split_for_parts();
    router
        .route("/openapi.json", get(move || async move { Json(api) }))
        .fallback(no_route)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn no_route(method: Method, uri: Uri) -> HttpError {
    HttpError::NoRoute {
        method,
        path: uri.path().to_owned(),
    }
}
