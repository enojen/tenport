use axum::Json;
use axum::http::{Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Serialize;

const PROBLEM_JSON: &str = "application/problem+json";
const BLANK_TYPE: &str = "about:blank";

#[derive(Debug, thiserror::Error)]
pub enum HttpError {
    #[error("no route for {method} {path}")]
    NoRoute { method: Method, path: String },
    #[error("the database does not answer")]
    DatabaseDown(#[source] sqlx::Error),
}

impl HttpError {
    fn status(&self) -> StatusCode {
        match self {
            Self::NoRoute { .. } => StatusCode::NOT_FOUND,
            Self::DatabaseDown(_) => StatusCode::SERVICE_UNAVAILABLE,
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::NoRoute { .. } => "not-found",
            Self::DatabaseDown(_) => "database-down",
        }
    }
}

#[derive(Serialize)]
struct Problem {
    #[serde(rename = "type")]
    kind: &'static str,
    title: &'static str,
    status: u16,
    detail: String,
    code: &'static str,
}

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        let status = self.status();
        let problem = Problem {
            kind: BLANK_TYPE,
            title: status.canonical_reason().unwrap_or("Error"),
            status: status.as_u16(),
            detail: self.to_string(),
            code: self.code(),
        };
        (
            status,
            [(header::CONTENT_TYPE, PROBLEM_JSON)],
            Json(problem),
        )
            .into_response()
    }
}
