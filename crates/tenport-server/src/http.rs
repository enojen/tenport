mod accounts;
mod agents;
mod auth;
mod error;
mod events;
mod extract;
mod health;
mod lanes;
mod limits;
mod openapi;
mod page;
mod projects;
mod router;
mod runs;
mod sse;

use std::sync::Arc;

use crate::cluster::Cluster;
use crate::db::Db;
use crate::runs::Runs;

pub use limits::{Limiter, Limits};
pub use router::router;

#[derive(Clone)]
pub struct AppState {
    pub db: Db,
    pub cluster: Arc<dyn Cluster>,
    pub runs: Arc<Runs>,
    pub limiter: Arc<Limiter>,
}
