#[cfg(test)]
mod tests;

use serde::{Deserialize, Serialize};

use crate::name::parsed_text;

const URL_SCHEMES: [&str; 2] = ["https://", "http://"];

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[serde(try_from = "String", into = "String")]
#[sqlx(transparent)]
pub struct TrackerUrl(String);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("a tracker link starts with https:// or http:// and has no spaces")]
pub struct TrackerUrlError;

impl TrackerUrl {
    pub fn parse(raw: &str) -> Result<Self, TrackerUrlError> {
        let url = raw.trim();
        let has_scheme = URL_SCHEMES
            .iter()
            .any(|scheme| url.len() > scheme.len() && url.starts_with(scheme));
        if has_scheme && !url.contains(char::is_whitespace) {
            Ok(Self(url.to_owned()))
        } else {
            Err(TrackerUrlError)
        }
    }
}

parsed_text!(TrackerUrl, TrackerUrlError);
