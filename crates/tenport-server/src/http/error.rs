use std::error::Error as StdError;

use axum::Json;
use axum::extract::rejection::{JsonRejection, PathRejection, QueryRejection};
use axum::http::{HeaderValue, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use utoipa::openapi::header::Header;
use utoipa::openapi::schema::{ObjectBuilder, Type};
use utoipa::openapi::{self, Content, Ref, RefOr, ResponseBuilder};
use utoipa::{ToResponse, ToSchema};

#[cfg(test)]
mod tests;

pub const PROBLEM_JSON: &str = "application/problem+json";
const BLANK_TYPE: &str = "about:blank";
const BOX_START_RETRY_SECONDS: u64 = 5;
const LOG_STREAM_RETRY_SECONDS: u64 = 5;
const QUERY_PREFIX: &str = "Failed to deserialize query string: ";

#[derive(Debug, thiserror::Error)]
pub enum HttpError {
    #[error("no route for {method} {path}")]
    NoRoute { method: Method, path: String },
    #[error("{path} has no method {method}")]
    MethodNotAllowed { method: Method, path: String },
    #[error("{0}")]
    PayloadTooLarge(String),
    #[error("too many requests; ask again after {retry_after} seconds")]
    TooManyRequests { retry_after: u64 },
    #[error("the user has {limit} followed logs open, the most there may be; close one first")]
    TooManyLogStreams { limit: usize },
    #[error("the request did not end in {seconds} seconds")]
    Timeout { seconds: u64 },
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
            Self::MethodNotAllowed { .. } => StatusCode::METHOD_NOT_ALLOWED,
            Self::PayloadTooLarge(_) => StatusCode::PAYLOAD_TOO_LARGE,
            Self::TooManyRequests { .. } | Self::TooManyLogStreams { .. } => {
                StatusCode::TOO_MANY_REQUESTS
            }
            Self::DatabaseDown(_) | Self::Timeout { .. } => StatusCode::SERVICE_UNAVAILABLE,
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

    fn retry_after(&self) -> Option<u64> {
        match self {
            Self::BoxNotStarted(_) => Some(BOX_START_RETRY_SECONDS),
            Self::TooManyRequests { retry_after } => Some(*retry_after),
            Self::TooManyLogStreams { .. } => Some(LOG_STREAM_RETRY_SECONDS),
            _ => None,
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::NoRoute { .. } | Self::NotFound(_) => "not-found",
            Self::MethodNotAllowed { .. } => "method-not-allowed",
            Self::PayloadTooLarge(_) => "payload-too-large",
            Self::TooManyRequests { .. } => "too-many-requests",
            Self::TooManyLogStreams { .. } => "too-many-log-streams",
            Self::Timeout { .. } => "timeout",
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
        let text = rejection.body_text();
        Self::BadRequest(text.strip_prefix(QUERY_PREFIX).unwrap_or(&text).to_owned())
    }
}

impl From<PathRejection> for HttpError {
    fn from(rejection: PathRejection) -> Self {
        Self::BadRequest(rejection.body_text())
    }
}

impl From<JsonRejection> for HttpError {
    fn from(rejection: JsonRejection) -> Self {
        if rejection.status() == StatusCode::PAYLOAD_TOO_LARGE {
            return Self::PayloadTooLarge(rejection.body_text());
        }
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
                .insert(header::RETRY_AFTER, HeaderValue::from(seconds));
        }
        response
    }
}

pub struct Unauthorized;

pub struct NotMember;

pub struct NotProjectAdmin;

pub struct RunNotFound;

pub struct TooManyRequests;

pub struct Unavailable;

impl<'s> ToResponse<'s> for Unauthorized {
    fn response() -> (&'s str, RefOr<openapi::Response>) {
        let response = problem_response("No token, or a token Tenport does not know");
        ("Unauthorized", response.build().into())
    }
}

impl<'s> ToResponse<'s> for NotMember {
    fn response() -> (&'s str, RefOr<openapi::Response>) {
        let response = problem_response("The user is not a member of the project's organization");
        ("NotMember", response.build().into())
    }
}

impl<'s> ToResponse<'s> for NotProjectAdmin {
    fn response() -> (&'s str, RefOr<openapi::Response>) {
        let response = problem_response("The user is not an admin of the project's organization");
        ("NotProjectAdmin", response.build().into())
    }
}

impl<'s> ToResponse<'s> for RunNotFound {
    fn response() -> (&'s str, RefOr<openapi::Response>) {
        let response = problem_response("Tenport does not know the run");
        ("RunNotFound", response.build().into())
    }
}

impl<'s> ToResponse<'s> for TooManyRequests {
    fn response() -> (&'s str, RefOr<openapi::Response>) {
        let retry_after = Header::builder()
            .schema(Some(
                ObjectBuilder::new()
                    .schema_type(Type::Integer)
                    .minimum(Some(1)),
            ))
            .description(Some("Seconds to wait before asking again"))
            .build();
        let response = problem_response(TOO_MANY_REQUESTS_TEXT).header("Retry-After", retry_after);
        ("TooManyRequests", response.build().into())
    }
}

impl<'s> ToResponse<'s> for Unavailable {
    fn response() -> (&'s str, RefOr<openapi::Response>) {
        let response = problem_response(UNAVAILABLE_TEXT);
        ("Unavailable", response.build().into())
    }
}

const TOO_MANY_REQUESTS_TEXT: &str = "Over a limit of the user: requests per second (code too-many-requests), or followed logs open at one time (code too-many-log-streams); ask again after Retry-After seconds";
const UNAVAILABLE_TEXT: &str = "The request did not end in time (code timeout), or the database does not answer (code database-down)";

fn problem_response(description: &str) -> ResponseBuilder {
    let problem = Content::new(Some(Ref::from_schema_name("Problem")));
    ResponseBuilder::new()
        .description(description)
        .content(PROBLEM_JSON, problem)
}
