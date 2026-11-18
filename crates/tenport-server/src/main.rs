use std::io::Write;
use std::process::ExitCode;
use std::sync::Arc;

use anyhow::Context;
use tenport_server::cluster::{Cluster, KubeCluster};
use tenport_server::command::{self, Command, USAGE};
use tenport_server::config::Config;
use tenport_server::http::{self, AppState, Limiter, Limits};
use tenport_server::run_credentials::RunCredentialKey;
use tenport_server::runs::{RunTemplate, Runs};
use tenport_server::task_archives::TaskArchiveBucket;
use tenport_server::{db, logs};
use tokio::net::TcpListener;
use tokio::signal::unix::{Signal, SignalKind, signal};

const USAGE_EXIT: u8 = 2;

#[tokio::main]
async fn main() -> ExitCode {
    logs::init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = match command::parse(&args) {
        Ok(command) => command,
        Err(error) => {
            tracing::error!("{error}\n{USAGE}");
            return ExitCode::from(USAGE_EXIT);
        }
    };
    let result = match command {
        Command::Serve => serve().await,
        Command::UserAdd(registration) => add_user(registration).await,
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!("{error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn add_user(registration: tenport_server::accounts::Registration) -> anyhow::Result<()> {
    let config = Config::from_env()?;
    let db = db::open(&config.database).await?;
    let added = command::add_user(&db, registration).await?;
    let mut out = std::io::stdout().lock();
    serde_json::to_writer(&mut out, &added)?;
    writeln!(out)?;
    Ok(())
}

async fn serve() -> anyhow::Result<()> {
    let config = Config::from_env()?;
    let bucket = config.task_archive_bucket.as_ref().context(
        "TENPORT_TASK_ARCHIVE_BUCKET is not set: runs need the bucket of the task archives",
    )?;
    let task_archives = TaskArchiveBucket::open(bucket)?;
    let key_file = config
        .run_credential_key_file
        .as_ref()
        .context("TENPORT_RUN_CREDENTIAL_KEY_FILE is not set: each run needs a placeholder")?;
    let run_credential_key = RunCredentialKey::read(key_file)?;
    let db = db::open(&config.database).await?;
    let cluster = KubeCluster::connect(&config.run_namespace)
        .await
        .context("cannot reach the Kubernetes API")?;
    let template = RunTemplate {
        dir: config.run_template,
        services_dir: config.run_services,
    };
    let cluster: Arc<dyn Cluster> = Arc::new(cluster);
    let runs = Runs::new(
        cluster.clone(),
        template,
        config.box_image,
        task_archives,
        run_credential_key,
    );
    let listener = TcpListener::bind(config.listen)
        .await
        .with_context(|| format!("cannot listen on {}", config.listen))?;
    let terminate = signal(SignalKind::terminate()).context("cannot watch SIGTERM")?;
    tracing::info!(address = %config.listen, "listening");
    let state = AppState {
        db,
        cluster,
        runs: Arc::new(runs),
        limiter: Arc::new(Limiter::new(Limits::default())),
    };
    axum::serve(listener, http::router(state))
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
