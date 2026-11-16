use std::process::ExitCode;

use tenport_server::config::Config;
use tenport_server::db;

#[tokio::main]
async fn main() -> ExitCode {
    let config = match Config::from_env() {
        Ok(config) => config,
        Err(error) => return fail(&error),
    };
    if let Err(error) = db::open(&config.database).await {
        return fail(&error);
    }
    println!("tenport: would listen on {}", config.listen);
    ExitCode::SUCCESS
}

fn fail(error: &dyn std::error::Error) -> ExitCode {
    eprintln!("tenport: {error}");
    ExitCode::FAILURE
}
