use std::env;
use std::net::SocketAddr;
use std::path::PathBuf;

#[cfg(test)]
mod tests;

const LISTEN: &str = "TENPORT_LISTEN";
const DATABASE: &str = "TENPORT_DATABASE";
const DEFAULT_LISTEN: &str = "127.0.0.1:8080";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub listen: SocketAddr,
    pub database: PathBuf,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ConfigError {
    #[error("{name} is not set")]
    Missing { name: &'static str },
    #[error("{name} is not an address and a port: {value}")]
    BadAddress { name: &'static str, value: String },
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_lookup(|name| env::var(name).ok())
    }

    fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self, ConfigError> {
        Ok(Self {
            listen: listen(lookup(LISTEN))?,
            database: database(lookup(DATABASE))?,
        })
    }
}

fn listen(value: Option<String>) -> Result<SocketAddr, ConfigError> {
    let value = value.unwrap_or_else(|| DEFAULT_LISTEN.to_owned());
    value.parse().map_err(|_| ConfigError::BadAddress {
        name: LISTEN,
        value,
    })
}

fn database(value: Option<String>) -> Result<PathBuf, ConfigError> {
    value
        .map(PathBuf::from)
        .ok_or(ConfigError::Missing { name: DATABASE })
}
