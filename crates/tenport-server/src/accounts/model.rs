use std::fmt;

use serde::{Deserialize, Serialize};

const ID_LENGTH: usize = 6;
const ID_ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
const ORG_ID_PREFIX: &str = "o-";

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OrgId(String);

impl OrgId {
    pub fn random() -> Self {
        Self(format!("{ORG_ID_PREFIX}{}", random_chars()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for OrgId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UserId(String);

impl UserId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for UserId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AccountsEvent {
    #[serde(rename = "organization.added")]
    OrganizationAdded(OrganizationAdded),
}

impl AccountsEvent {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::OrganizationAdded(_) => "organization.added",
        }
    }

    pub fn subject(&self) -> String {
        match self {
            Self::OrganizationAdded(added) => format!("organization:{}", added.id),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrganizationAdded {
    pub id: OrgId,
    pub name: String,
}

fn random_chars() -> String {
    (0..ID_LENGTH)
        .map(|_| char::from(ID_ALPHABET[rand::random_range(0..ID_ALPHABET.len())]))
        .collect()
}
