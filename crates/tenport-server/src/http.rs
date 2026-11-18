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
mod run_credentials;
mod runs;
mod sse;
mod tasks;

use std::sync::Arc;

use crate::cluster::Cluster;
use crate::db::Db;
use crate::run_credentials::InjectorToken;
use crate::runs::Runs;

pub use limits::{Limiter, Limits};
pub use router::router;

#[derive(Clone)]
pub struct AppState {
    pub db: Db,
    pub cluster: Arc<dyn Cluster>,
    pub runs: Arc<Runs>,
    pub injector_token: Arc<InjectorToken>,
    pub limiter: Arc<Limiter>,
}
