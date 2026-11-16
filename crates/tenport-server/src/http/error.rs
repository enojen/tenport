use axum::Json;
use axum::http::{Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use utoipa::ToSchema;

#[cfg(test)]
mod tests;

pub const PROBLEM_JSON: &str = "application/problem+json";
const BLANK_TYPE: &str = "about:blank";

#[derive(Debug, thiserror::Error)]
pub enum HttpError {
    #[error("no route for {method} {path}")]
    NoRoute { method: Method, path: String },
    #[error("the database does not answer")]
    DatabaseDown(#[source] sqlx::Error),
    #[error("a valid bearer token is needed")]
    Unauthorized,
    #[error("the server failed; see its log")]
    Internal(#[source] Box<dyn std::error::Error + Send + Sync>),
}

impl HttpError {
    pub fn internal(error: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::Internal(Box::new(error))
    }

    fn status(&self) -> StatusCode {
        match self {
            Self::NoRoute { .. } => StatusCode::NOT_FOUND,
            Self::DatabaseDown(_) => StatusCode::SERVICE_UNAVAILABLE,
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::NoRoute { .. } => "not-found",
            Self::DatabaseDown(_) => "database-down",
            Self::Unauthorized => "unauthorized",
            Self::Internal(_) => "internal",
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct Problem {
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
        if status.is_server_error() {
            tracing::error!(error = %self, source = ?std::error::Error::source(&self), "request failed");
        }
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
