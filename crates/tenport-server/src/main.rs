use std::process::ExitCode;

use anyhow::Context;
use tenport_server::config::Config;
use tenport_server::http::{self, AppState};
use tenport_server::{db, logs};
use tokio::net::TcpListener;
use tokio::signal::unix::{Signal, SignalKind, signal};

#[tokio::main]
async fn main() -> ExitCode {
    logs::init();
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!("{error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> anyhow::Result<()> {
    let config = Config::from_env()?;
    let db = db::open(&config.database).await?;
    let listener = TcpListener::bind(config.listen)
        .await
        .with_context(|| format!("cannot listen on {}", config.listen))?;
    let terminate = signal(SignalKind::terminate()).context("cannot watch SIGTERM")?;
    tracing::info!(address = %config.listen, "listening");
    axum::serve(listener, http::router(AppState { db }))
        .with_graceful_shutdown(stopped(terminate))
        .await?;
    tracing::info!("stopped");
    Ok(())
}

async fn stopped(mut terminate: Signal) {
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = terminate.recv() => {}
    }
}
