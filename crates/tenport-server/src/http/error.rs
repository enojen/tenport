use std::error::Error as StdError;

use axum::Json;
use axum::extract::rejection::{JsonRejection, QueryRejection};
use axum::http::{HeaderValue, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use utoipa::ToSchema;

#[cfg(test)]
mod tests;

pub const PROBLEM_JSON: &str = "application/problem+json";
const BLANK_TYPE: &str = "about:blank";
const BOX_START_RETRY_SECONDS: &str = "5";

#[derive(Debug, thiserror::Error)]
pub enum HttpError {
    #[error("no route for {method} {path}")]
    NoRoute { method: Method, path: String },
    #[error("the database does not answer")]
    DatabaseDown(#[source] sqlx::Error),
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    NotFound(String),
    #[error("a valid bearer token is needed")]
    Unauthorized,
    #[error("{0}")]
    Forbidden(String),
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    MachineBusy(String),
    #[error("{0}")]
    BoxNotStarted(String),
    #[error("{0}")]
    JobGone(String),
    #[error("the server failed; see its log")]
    Internal(#[source] Box<dyn std::error::Error + Send + Sync>),
}

impl HttpError {
    pub fn internal(error: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::Internal(Box::new(error))
    }

    fn status(&self) -> StatusCode {
        match self {
            Self::NoRoute { .. } | Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::DatabaseDown(_) => StatusCode::SERVICE_UNAVAILABLE,
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::Forbidden(_) => StatusCode::FORBIDDEN,
            Self::Conflict(_)
            | Self::MachineBusy(_)
            | Self::BoxNotStarted(_)
            | Self::JobGone(_) => StatusCode::CONFLICT,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn retry_after(&self) -> Option<&'static str> {
        match self {
            Self::BoxNotStarted(_) => Some(BOX_START_RETRY_SECONDS),
            _ => None,
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::NoRoute { .. } | Self::NotFound(_) => "not-found",
            Self::DatabaseDown(_) => "database-down",
            Self::BadRequest(_) => "bad-request",
            Self::Unauthorized => "unauthorized",
            Self::Forbidden(_) => "forbidden",
            Self::Conflict(_) => "conflict",
            Self::MachineBusy(_) => "machine-busy",
            Self::BoxNotStarted(_) => "box-not-started",
            Self::JobGone(_) => "job-gone",
            Self::Internal(_) => "internal",
        }
    }
}

impl From<QueryRejection> for HttpError {
    fn from(rejection: QueryRejection) -> Self {
        Self::BadRequest(rejection.body_text())
    }
}

impl From<JsonRejection> for HttpError {
    fn from(rejection: JsonRejection) -> Self {
        let detail = match (&rejection, serde_error(&rejection)) {
            (JsonRejection::JsonSyntaxError(_), Some(error)) => {
                format!("the body is not valid JSON: {}", describe(error))
            }
            (_, Some(error)) => describe(error),
            (_, None) => rejection.body_text(),
        };
        Self::BadRequest(detail)
    }
}

type SerdeError = serde_path_to_error::Error<serde_json::Error>;

fn serde_error(rejection: &JsonRejection) -> Option<&SerdeError> {
    let mut source = StdError::source(rejection);
    while let Some(error) = source {
        if let Some(found) = error.downcast_ref::<SerdeError>() {
            return Some(found);
        }
        source = error.source();
    }
    None
}

fn describe(error: &SerdeError) -> String {
    let inner = error.inner();
    let text = inner.to_string();
    let position = format!(" at line {} column {}", inner.line(), inner.column());
    let message = text.strip_suffix(&position).unwrap_or(&text);
    let path = error.path().to_string();
    if path == "." {
        message.to_owned()
    } else {
        format!("{path}: {message}")
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
        let retry_after = self.retry_after();
        let mut response = (
            status,
            [(header::CONTENT_TYPE, PROBLEM_JSON)],
            Json(problem),
        )
            .into_response();
        if let Some(seconds) = retry_after {
            response
                .headers_mut()
                .insert(header::RETRY_AFTER, HeaderValue::from_static(seconds));
        }
        response
    }
}
