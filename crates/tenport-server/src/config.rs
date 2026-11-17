use std::env;
use std::net::SocketAddr;
use std::path::PathBuf;

#[cfg(test)]
mod tests;

const LISTEN: &str = "TENPORT_LISTEN";
const DATABASE: &str = "TENPORT_DATABASE";
const RUN_TEMPLATE: &str = "TENPORT_RUN_TEMPLATE";
const RUN_SERVICES: &str = "TENPORT_RUN_SERVICES";
const RUN_NAMESPACE: &str = "TENPORT_RUN_NAMESPACE";
const BOX_IMAGE: &str = "TENPORT_BOX_IMAGE";
const DEFAULT_LISTEN: &str = "127.0.0.1:8080";
const DEFAULT_RUN_TEMPLATE: &str = "/etc/tenport/run-template";
const DEFAULT_RUN_SERVICES: &str = "/etc/tenport/run-services";
const DEFAULT_RUN_NAMESPACE: &str = "tenbox";
const DEFAULT_BOX_IMAGE: &str = "ghcr.io/enojen/tenbox-box:latest";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub listen: SocketAddr,
    pub database: PathBuf,
    pub run_template: PathBuf,
    pub run_services: PathBuf,
    pub run_namespace: String,
    pub box_image: String,
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
            run_template: PathBuf::from(or_default(lookup(RUN_TEMPLATE), DEFAULT_RUN_TEMPLATE)),
            run_services: PathBuf::from(or_default(lookup(RUN_SERVICES), DEFAULT_RUN_SERVICES)),
            run_namespace: or_default(lookup(RUN_NAMESPACE), DEFAULT_RUN_NAMESPACE),
            box_image: or_default(lookup(BOX_IMAGE), DEFAULT_BOX_IMAGE),
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

fn or_default(value: Option<String>, default: &str) -> String {
    value.unwrap_or_else(|| default.to_owned())
}
