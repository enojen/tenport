use std::process::ExitCode;

use tenport_server::config::Config;

fn main() -> ExitCode {
    match Config::from_env() {
        Ok(config) => {
            println!("tenport: would listen on {}", config.listen);
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("tenport: {error}");
            ExitCode::FAILURE
        }
    }
}
