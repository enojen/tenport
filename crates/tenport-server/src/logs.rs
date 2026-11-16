use std::io::{IsTerminal, stderr};

use tracing_subscriber::EnvFilter;

const DEFAULT_FILTER: &str = "info";

pub fn init() {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(DEFAULT_FILTER));
    let logs = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(stderr);
    if stderr().is_terminal() {
        logs.init();
    } else {
        logs.json().init();
    }
}
