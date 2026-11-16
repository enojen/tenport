use std::fmt;

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::accounts::{AccountsEvent, UserId};
use crate::projects::ProjectsEvent;

const SERVER_ACTOR: &str = "server";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EventId(Uuid);

impl EventId {
    pub fn now() -> Self {
        Self(Uuid::now_v7())
    }
}

impl fmt::Display for EventId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Actor {
    Server,
    User(UserId),
}

impl fmt::Display for Actor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Server => f.write_str(SERVER_ACTOR),
            Self::User(id) => id.fmt(f),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Event {
    Accounts(AccountsEvent),
    Projects(ProjectsEvent),
}

impl Event {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Accounts(event) => event.kind(),
            Self::Projects(event) => event.kind(),
        }
    }

    pub fn subject(&self) -> String {
        match self {
            Self::Accounts(event) => event.subject(),
            Self::Projects(event) => event.subject(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub id: EventId,
    pub at: OffsetDateTime,
    pub actor: Actor,
    pub event: Event,
}
