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

use std::sync::Arc;

use crate::db::Db;
use crate::runs::Runs;

pub use router::router;

#[derive(Clone)]
pub struct AppState {
    pub db: Db,
    pub runs: Arc<Runs>,
}
