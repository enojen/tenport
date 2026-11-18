use axum::extract::State;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;

use super::AppState;
use super::auth::CurrentUser;
use super::error::{
    HttpError, NotMember, PROBLEM_JSON, Problem, TooManyRequests, Unauthorized, Unavailable,
};
use super::extract::Json;
use crate::events::{EventId, EventsError};
use crate::tasks::{self, Reported, TasksError, TasksEvent};

#[derive(Deserialize, ToSchema)]
pub struct ReportedEvent {
    #[schema(value_type = String, example = "019a8f2e-7c4b-7d21-9a3e-5b8c1d2e3f40")]
    id: EventId,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    at: OffsetDateTime,
    #[serde(flatten)]
    event: TasksEvent,
}

#[derive(Serialize, ToSchema)]
pub struct Recorded {
    stored: usize,
    skipped: usize,
}

#[utoipa::path(
    post,
    path = "/events",
    security(("bearer" = [])),
    request_body = Vec<ReportedEvent>,
    responses(
        (status = OK, description = "The new events are stored; an id Tenport has is skipped", body = Recorded),
        (status = BAD_REQUEST, description = "The body is not a list of valid events", body = Problem, content_type = PROBLEM_JSON),
        (status = UNAUTHORIZED, response = Unauthorized),
        (status = TOO_MANY_REQUESTS, response = TooManyRequests),
        (status = SERVICE_UNAVAILABLE, response = Unavailable),
        (status = FORBIDDEN, response = NotMember),
        (status = CONFLICT, description = "An id came before with other content, or an event does not fit the state; nothing is stored", body = Problem, content_type = PROBLEM_JSON),
    ),
)]
pub async fn record_events(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Json(body): Json<Vec<ReportedEvent>>,
) -> Result<Json<Recorded>, HttpError> {
    let reported = body
        .into_iter()
        .map(|event| Reported {
            id: event.id,
            at: event.at,
            event: event.event,
        })
        .collect();
    let recorded = tasks::record_events(&state.db, &user.id, reported)
        .await
        .map_err(record_error)?;
    Ok(Json(Recorded {
        stored: recorded.stored,
        skipped: recorded.skipped,
    }))
}

fn record_error(error: TasksError) -> HttpError {
    match error {
        TasksError::Projects(error) => error.into(),
        TasksError::EventNotAllowed { .. } => HttpError::BadRequest(error.to_string()),
        TasksError::Events(EventsError::Conflict { .. } | EventsError::Refused { .. }) => {
            HttpError::Conflict(error.to_string())
        }
        other => HttpError::internal(other),
    }
}
