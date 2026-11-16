mod model;
mod store;

#[cfg(test)]
mod tests;

use sqlx::SqliteConnection;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::accounts;
use crate::db::Db;

pub use model::{Actor, Entry, Event, EventId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Appended {
    New,
    Duplicate,
}

pub struct Stamp<'a> {
    pub at: &'a str,
    pub actor: &'a str,
}

#[derive(Debug, thiserror::Error)]
pub enum EventsError {
    #[error("the event {id} was recorded before with other content")]
    Conflict { id: EventId },
    #[error("cannot encode the event: {0}")]
    Encode(#[from] serde_json::Error),
    #[error("cannot write the time of the event: {0}")]
    Time(#[from] time::error::Format),
    #[error("cannot record the event: {0}")]
    Database(#[from] sqlx::Error),
}

impl EventsError {
    pub fn is_unique_violation(&self) -> bool {
        match self {
            Self::Database(sqlx::Error::Database(error)) => error.is_unique_violation(),
            _ => false,
        }
    }
}

pub async fn append(db: &Db, actor: Actor, event: Event) -> Result<EventId, EventsError> {
    let entry = Entry {
        id: EventId::now(),
        at: OffsetDateTime::now_utc(),
        actor,
        event,
    };
    append_entry(db, &entry).await?;
    Ok(entry.id)
}

pub async fn append_entry(db: &Db, entry: &Entry) -> Result<Appended, EventsError> {
    let row = row(entry)?;
    let mut tx = db.begin().await?;
    if !store::insert(&mut tx, &row).await? {
        return duplicate(&mut tx, &row, entry.id).await;
    }
    apply(&mut tx, entry, &row).await?;
    tx.commit().await?;
    Ok(Appended::New)
}

async fn duplicate(
    conn: &mut SqliteConnection,
    row: &store::Row,
    id: EventId,
) -> Result<Appended, EventsError> {
    match store::find(conn, &row.id).await? {
        Some(existing) if existing == *row => Ok(Appended::Duplicate),
        _ => Err(EventsError::Conflict { id }),
    }
}

async fn apply(
    conn: &mut SqliteConnection,
    entry: &Entry,
    row: &store::Row,
) -> Result<(), EventsError> {
    let stamp = Stamp {
        at: &row.at,
        actor: &row.actor,
    };
    match &entry.event {
        Event::Accounts(event) => accounts::apply(conn, event, &stamp).await?,
    }
    Ok(())
}

fn row(entry: &Entry) -> Result<store::Row, EventsError> {
    Ok(store::Row {
        id: entry.id.to_string(),
        kind: entry.event.kind().to_owned(),
        subject: entry.event.subject(),
        actor: entry.actor.to_string(),
        at: entry.at.format(&Rfc3339)?,
        body: serde_json::to_string(&entry.event)?,
    })
}
