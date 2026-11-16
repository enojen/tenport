mod model;
mod store;

#[cfg(test)]
mod tests;

use sqlx::SqliteConnection;

use crate::db::Db;
use crate::events::{self, Actor, Event, EventsError, Stamp};

pub use model::{AccountsEvent, OrgId, OrganizationAdded, UserId};
#[cfg(test)]
pub(crate) use store::organization_by_name;

#[derive(Debug, thiserror::Error)]
pub enum AccountsError {
    #[error("an organization needs a name")]
    EmptyName,
    #[error("the organization {name} exists")]
    OrganizationExists { name: String },
    #[error(transparent)]
    Events(#[from] EventsError),
}

pub async fn add_organization(db: &Db, actor: Actor, name: &str) -> Result<OrgId, AccountsError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AccountsError::EmptyName);
    }
    let id = OrgId::random();
    let added = OrganizationAdded {
        id: id.clone(),
        name: name.to_owned(),
    };
    events::append(
        db,
        actor,
        Event::Accounts(AccountsEvent::OrganizationAdded(added)),
    )
    .await
    .map_err(|error| organization_error(error, name))?;
    Ok(id)
}

fn organization_error(error: EventsError, name: &str) -> AccountsError {
    if error.is_unique_violation() {
        AccountsError::OrganizationExists {
            name: name.to_owned(),
        }
    } else {
        AccountsError::Events(error)
    }
}

pub(crate) async fn apply(
    conn: &mut SqliteConnection,
    event: &AccountsEvent,
    stamp: &Stamp<'_>,
) -> Result<(), sqlx::Error> {
    match event {
        AccountsEvent::OrganizationAdded(added) => {
            store::insert_organization(conn, added, stamp.at, stamp.actor).await
        }
    }
}
