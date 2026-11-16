mod model;
mod store;

#[cfg(test)]
mod tests;

use std::slice;

use sqlx::SqliteConnection;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::db::Db;
use crate::{accounts, lanes, projects};

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
pub enum ApplyError {
    #[error("{0}")]
    Refused(&'static str),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum EventsError {
    #[error("the event {id} was recorded before with other content")]
    Conflict { id: EventId },
    #[error("the event {id} is refused: {reason}")]
    Refused { id: EventId, reason: &'static str },
    #[error("cannot encode the event: {0}")]
    Encode(#[from] serde_json::Error),
    #[error("cannot write the time of the event: {0}")]
    Time(#[from] time::error::Format),
    #[error("cannot record the event: {0}")]
    Database(#[from] sqlx::Error),
}

impl EventsError {
    pub fn violates_unique(&self, column: &str) -> bool {
        match self {
            Self::Database(sqlx::Error::Database(error)) => {
                error.is_unique_violation() && error.message().contains(column)
            }
            _ => false,
        }
    }
}

pub async fn append(db: &Db, actor: Actor, event: Event) -> Result<EventId, EventsError> {
    let entry = new_entry(actor, event, OffsetDateTime::now_utc());
    append_entries(db, slice::from_ref(&entry)).await?;
    Ok(entry.id)
}

pub async fn append_all(
    db: &Db,
    actor: Actor,
    events: Vec<Event>,
) -> Result<Vec<EventId>, EventsError> {
    let at = OffsetDateTime::now_utc();
    let entries: Vec<Entry> = events
        .into_iter()
        .map(|event| new_entry(actor.clone(), event, at))
        .collect();
    append_entries(db, &entries).await?;
    Ok(entries.iter().map(|entry| entry.id).collect())
}

pub async fn append_entries(db: &Db, entries: &[Entry]) -> Result<Vec<Appended>, EventsError> {
    let mut tx = db.begin().await?;
    let mut appended = Vec::with_capacity(entries.len());
    for entry in entries {
        appended.push(write(&mut tx, entry).await?);
    }
    tx.commit().await?;
    Ok(appended)
}

fn new_entry(actor: Actor, event: Event, at: OffsetDateTime) -> Entry {
    Entry {
        id: EventId::now(),
        at,
        actor,
        event,
    }
}

async fn write(conn: &mut SqliteConnection, entry: &Entry) -> Result<Appended, EventsError> {
    let row = row(entry)?;
    if !store::insert_event(conn, &row).await? {
        return duplicate(conn, &row, entry.id).await;
    }
    apply(conn, entry, &row)
        .await
        .map_err(|error| applied_error(error, entry.id))?;
    Ok(Appended::New)
}

async fn duplicate(
    conn: &mut SqliteConnection,
    row: &store::Row,
    id: EventId,
) -> Result<Appended, EventsError> {
    match store::find_event_by_id(conn, &row.id).await? {
        Some(existing) if existing == *row => Ok(Appended::Duplicate),
        _ => Err(EventsError::Conflict { id }),
    }
}

async fn apply(
    conn: &mut SqliteConnection,
    entry: &Entry,
    row: &store::Row,
) -> Result<(), ApplyError> {
    let stamp = Stamp {
        at: &row.at,
        actor: &row.actor,
    };
    match &entry.event {
        Event::Accounts(event) => accounts::apply(conn, event, &stamp).await?,
        Event::Projects(event) => projects::apply(conn, event, &stamp).await?,
        Event::Lanes(event) => lanes::apply(conn, event, &stamp).await?,
    }
    Ok(())
}

fn applied_error(error: ApplyError, id: EventId) -> EventsError {
    match error {
        ApplyError::Refused(reason) => EventsError::Refused { id, reason },
        ApplyError::Database(error) => EventsError::Database(error),
    }
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
