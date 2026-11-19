mod model;
mod store;

use sqlx::SqliteConnection;

use crate::accounts::UserId;
use crate::cluster::{Cluster, ClusterError};
use crate::db::Db;
use crate::events::{self, Actor, ApplyError, Event, EventsError, Stamp};
use crate::name::Name;

pub use model::{
    AGENT_LOGIN_KEY, Agent, AgentAdded, AgentId, AgentKind, AgentLogin, AgentLoginError,
    AgentsEvent, LoginChanged, LoginState, agent_secret,
};

const UNIQUE_NAME: &str = "agents.name";

#[derive(Debug, thiserror::Error)]
pub enum AgentsError {
    #[error("the user has an agent named {name} already")]
    NameTaken { name: Name },
    #[error("the user has no agent {agent}")]
    NotFound { agent: AgentId },
    #[error("the user has no agent; add one first")]
    NoAgent,
    #[error(transparent)]
    Cluster(#[from] ClusterError),
    #[error(transparent)]
    Events(#[from] EventsError),
    #[error("cannot read agents: {0}")]
    Database(#[from] sqlx::Error),
}

pub async fn add(
    db: &Db,
    user: &UserId,
    kind: AgentKind,
    name: Name,
) -> Result<Agent, AgentsError> {
    let has_default = store::find_default_agent_of_user(db, user).await?.is_some();
    let added = AgentAdded {
        user: user.clone(),
        agent: AgentId::random(),
        kind,
        name: name.clone(),
        default: !has_default,
    };
    let event = Event::Agents(AgentsEvent::AgentAdded(added.clone()));
    events::append(db, Actor::User(user.clone()), event)
        .await
        .map_err(|error| add_error(error, name))?;
    Ok(added.into())
}

fn add_error(error: EventsError, name: Name) -> AgentsError {
    if error.violates_unique(UNIQUE_NAME) {
        AgentsError::NameTaken { name }
    } else {
        AgentsError::Events(error)
    }
}

pub async fn list_of_user(db: &Db, user: &UserId) -> Result<Vec<Agent>, AgentsError> {
    Ok(store::list_agents_of_user(db, user).await?)
}

pub async fn write_login(
    db: &Db,
    cluster: &dyn Cluster,
    user: &UserId,
    agent: &AgentId,
    login: &AgentLogin,
) -> Result<LoginState, AgentsError> {
    require_own(db, user, agent).await?;
    cluster
        .write_secret_key(&agent_secret(agent), AGENT_LOGIN_KEY, login.expose())
        .await?;
    let changed = LoginChanged {
        agent: agent.clone(),
    };
    let event = Event::Agents(AgentsEvent::LoginChanged(changed));
    events::append(db, Actor::User(user.clone()), event).await?;
    let written = require_own(db, user, agent).await?;
    let changed = written.login_changed.ok_or_else(|| AgentsError::NotFound {
        agent: agent.clone(),
    })?;
    Ok(LoginState {
        agent: written.id,
        changed,
    })
}

pub async fn named_or_default(
    db: &Db,
    user: &UserId,
    agent: Option<&AgentId>,
) -> Result<AgentId, AgentsError> {
    match agent {
        Some(agent) => Ok(require_own(db, user, agent).await?.id),
        None => store::find_default_agent_of_user(db, user)
            .await?
            .ok_or(AgentsError::NoAgent),
    }
}

async fn require_own(db: &Db, user: &UserId, agent: &AgentId) -> Result<Agent, AgentsError> {
    store::find_agent_of_user(db, user, agent)
        .await?
        .ok_or_else(|| AgentsError::NotFound {
            agent: agent.clone(),
        })
}

pub(crate) async fn apply(
    conn: &mut SqliteConnection,
    event: &AgentsEvent,
    stamp: &Stamp<'_>,
) -> Result<(), ApplyError> {
    match event {
        AgentsEvent::AgentAdded(added) => store::insert_agent(conn, added, stamp).await?,
        AgentsEvent::LoginChanged(changed) => {
            if !store::update_agent_login(conn, changed, stamp).await? {
                return Err(ApplyError::Refused("the agent is not known"));
            }
        }
    }
    Ok(())
}
