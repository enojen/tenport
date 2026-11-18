use axum::extract::{FromRequest, FromRequestParts};
use axum::response::{IntoResponse, Response};
use serde::Serialize;

use super::error::HttpError;

#[derive(FromRequest)]
#[from_request(via(axum::Json), rejection(HttpError))]
pub struct Json<T>(pub T);

#[derive(FromRequestParts)]
#[from_request(via(axum::extract::Query), rejection(HttpError))]
pub struct Query<T>(pub T);

#[derive(FromRequestParts)]
#[from_request(via(axum::extract::Path), rejection(HttpError))]
pub struct Path<T>(pub T);

impl<T: Serialize> IntoResponse for Json<T> {
    fn into_response(self) -> Response {
        axum::Json(self.0).into_response()
    }
}
