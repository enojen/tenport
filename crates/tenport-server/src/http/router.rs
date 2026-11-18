use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::http::{Method, Uri};
use axum::middleware;
use axum::routing::get;
use tower_http::trace::TraceLayer;
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;

use super::AppState;
use super::error::HttpError;
use super::extract::Json;
use super::limits::limit_requests;
use super::openapi::{ApiDoc, document};
use super::{accounts, events, health, projects, run_credentials, runs, tasks};

pub fn router(state: AppState) -> Router {
    let (router, api) = OpenApiRouter::with_openapi(ApiDoc::openapi())
        .merge(health::routes())
        .merge(accounts::routes())
        .merge(projects::routes())
        .merge(events::routes())
        .merge(runs::routes())
        .merge(tasks::routes())
        .merge(run_credentials::routes())
        .split_for_parts();
    let document = document(api);
    router
        .route("/openapi.json", get(move || async move { Json(document) }))
        .fallback(no_route)
        .method_not_allowed_fallback(no_method)
        .layer(DefaultBodyLimit::max(state.limiter.limits().body_bytes))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            limit_requests,
        ))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn no_method(method: Method, uri: Uri) -> HttpError {
    HttpError::MethodNotAllowed {
        method,
        path: uri.path().to_owned(),
    }
}

async fn no_route(method: Method, uri: Uri) -> HttpError {
    HttpError::NoRoute {
        method,
        path: uri.path().to_owned(),
    }
}
