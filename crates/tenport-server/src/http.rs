mod accounts;
mod agents;
mod auth;
mod error;
mod events;
mod health;
mod lanes;
mod openapi;
mod projects;
mod router;
mod runs;
mod sse;

use crate::db::Db;

pub use router::router;

#[derive(Clone)]
pub struct AppState {
    pub db: Db,
}
