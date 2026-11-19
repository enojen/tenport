use std::fmt;

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;

use crate::accounts::UserId;
use crate::cluster::SecretName;
use crate::id::short_id;
use crate::name::Name;

short_id!(AgentId, "a-", "an agent id");

pub const AGENT_LOGIN_KEY: &str = "credentials.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema, sqlx::Type)]
#[serde(rename_all = "kebab-case")]
#[sqlx(type_name = "text", rename_all = "kebab-case")]
pub enum AgentKind {
    ClaudeCode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Agent {
    pub id: AgentId,
    pub kind: AgentKind,
    pub name: Name,
    pub default: bool,
    pub login_changed: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AgentsEvent {
    #[serde(rename = "agent.added")]
    AgentAdded(AgentAdded),
    #[serde(rename = "agent.login_changed")]
    LoginChanged(LoginChanged),
}

impl AgentsEvent {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::AgentAdded(_) => "agent.added",
            Self::LoginChanged(_) => "agent.login_changed",
        }
    }

    pub fn subject(&self) -> String {
        let agent = match self {
            Self::AgentAdded(added) => &added.agent,
            Self::LoginChanged(changed) => &changed.agent,
        };
        format!("agent:{agent}")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentAdded {
    pub user: UserId,
    pub agent: AgentId,
    pub kind: AgentKind,
    pub name: Name,
    pub default: bool,
}

impl From<AgentAdded> for Agent {
    fn from(added: AgentAdded) -> Self {
        Self {
            id: added.agent,
            kind: added.kind,
            name: added.name,
            default: added.default,
            login_changed: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoginChanged {
    pub agent: AgentId,
}

pub fn agent_secret(agent: &AgentId) -> SecretName {
    SecretName::new(format!("agent-{agent}"))
}

const MAX_LOGIN_LENGTH: usize = 16_384;

#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(try_from = "String")]
pub struct AgentLogin(String);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "a login is a JSON object of 1 to {MAX_LOGIN_LENGTH} characters, the text of the agent's credentials file"
)]
pub struct AgentLoginError;

impl AgentLogin {
    pub fn parse(raw: &str) -> Result<Self, AgentLoginError> {
        let fits = (1..=MAX_LOGIN_LENGTH).contains(&raw.chars().count());
        let is_object =
            serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(raw).is_ok();
        if fits && is_object {
            Ok(Self(raw.to_owned()))
        } else {
            Err(AgentLoginError)
        }
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for AgentLogin {
    type Error = AgentLoginError;

    fn try_from(raw: String) -> Result<Self, AgentLoginError> {
        Self::parse(&raw)
    }
}

impl fmt::Debug for AgentLogin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AgentLogin(hidden)")
    }
}
