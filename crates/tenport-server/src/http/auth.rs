use axum::extract::FromRequestParts;
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;

use super::AppState;
use super::error::HttpError;
use crate::accounts::{self, Token, User};

const BEARER: &str = "Bearer ";

pub struct CurrentUser(pub User);

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = HttpError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, HttpError> {
        let token = bearer(parts).ok_or(HttpError::Unauthorized)?;
        let user = accounts::authenticate(&state.db, &token)
            .await
            .map_err(HttpError::internal)?
            .ok_or(HttpError::Unauthorized)?;
        Ok(Self(user))
    }
}

fn bearer(parts: &Parts) -> Option<Token> {
    let header = parts.headers.get(AUTHORIZATION)?.to_str().ok()?;
    Token::parse(header.strip_prefix(BEARER)?).ok()
}
