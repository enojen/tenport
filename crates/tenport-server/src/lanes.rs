mod model;
mod store;

#[cfg(test)]
mod tests;

use std::collections::HashSet;

use sqlx::SqliteConnection;
use time::OffsetDateTime;

use crate::accounts::UserId;
use crate::db::Db;
use crate::events::{self, Actor, Appended, ApplyError, Entry, Event, EventId, EventsError, Stamp};
use crate::projects::{self, ProjectId, ProjectsError};

pub use model::{
    LanesEvent, TaskClosed, TaskId, TaskIdError, TaskStarted, TrackerUrl, TrackerUrlError,
};

#[derive(Debug, thiserror::Error)]
pub enum LanesError {
    #[error("only a member of the organization of the project {project} reports its tasks")]
    NotMember { project: ProjectId },
    #[error(transparent)]
    Projects(#[from] ProjectsError),
    #[error(transparent)]
    Events(#[from] EventsError),
}

#[derive(Debug, Clone)]
pub struct Reported {
    pub id: EventId,
    pub at: OffsetDateTime,
    pub event: LanesEvent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Recorded {
    pub stored: usize,
    pub skipped: usize,
}

pub async fn record(
    db: &Db,
    user: &UserId,
    reported: Vec<Reported>,
) -> Result<Recorded, LanesError> {
    require_member(db, user, &reported).await?;
    let entries: Vec<Entry> = reported
        .into_iter()
        .map(|reported| Entry {
            id: reported.id,
            at: reported.at,
            actor: Actor::User(user.clone()),
            event: Event::Lanes(reported.event),
        })
        .collect();
    let appended = events::append_entries(db, &entries).await?;
    let stored = appended.iter().filter(|a| **a == Appended::New).count();
    Ok(Recorded {
        stored,
        skipped: appended.len() - stored,
    })
}

pub(crate) async fn apply(
    conn: &mut SqliteConnection,
    event: &LanesEvent,
    stamp: &Stamp<'_>,
) -> Result<(), ApplyError> {
    match event {
        LanesEvent::TaskStarted(started) => refuse_unless(
            store::insert_task(conn, started, stamp).await?,
            "the task is started already",
        ),
        LanesEvent::TaskClosed(closed) => refuse_unless(
            store::close_task(conn, closed, stamp).await?,
            "the task is not open",
        ),
    }
}

async fn require_member(db: &Db, user: &UserId, reported: &[Reported]) -> Result<(), LanesError> {
    let projects: HashSet<&ProjectId> = reported.iter().map(|r| r.event.project()).collect();
    for project in projects {
        if !projects::is_member(db, user, project).await? {
            return Err(LanesError::NotMember {
                project: project.clone(),
            });
        }
    }
    Ok(())
}

fn refuse_unless(done: bool, reason: &'static str) -> Result<(), ApplyError> {
    if done {
        Ok(())
    } else {
        Err(ApplyError::Refused(reason))
    }
}
