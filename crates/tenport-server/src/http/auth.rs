use std::time::Instant;

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
        state.limiter.take_request(&user.id, Instant::now())?;
        Ok(Self(user))
    }
}

pub struct Injector;

impl FromRequestParts<AppState> for Injector {
    type Rejection = HttpError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, HttpError> {
        let token = bearer_text(parts).ok_or(HttpError::Unauthorized)?;
        if state.injector_token.matches(token) {
            Ok(Self)
        } else {
            Err(HttpError::Unauthorized)
        }
    }
}

fn bearer(parts: &Parts) -> Option<Token> {
    Token::parse(bearer_text(parts)?).ok()
}

fn bearer_text(parts: &Parts) -> Option<&str> {
    let header = parts.headers.get(AUTHORIZATION)?.to_str().ok()?;
    header.strip_prefix(BEARER)
}
