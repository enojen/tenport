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
use crate::runs::{Runs, RunsError};
use crate::task_archives::{TaskArchiveBucket, TaskArchiveState, TaskArchivesError};

pub use model::{TaskClosed, TaskDeleted, TaskId, TaskIdError, TaskStarted, TasksEvent};

#[derive(Debug, thiserror::Error)]
pub enum TasksError {
    #[error("only Tenport writes {kind}: send DELETE /tasks/{{task}}?project=<id>")]
    EventNotAllowed { kind: &'static str },
    #[error(transparent)]
    Projects(#[from] ProjectsError),
    #[error(transparent)]
    Runs(#[from] RunsError),
    #[error(transparent)]
    TaskArchives(#[from] TaskArchivesError),
    #[error(transparent)]
    Events(#[from] EventsError),
}

#[derive(Debug, Clone)]
pub struct Reported {
    pub id: EventId,
    pub at: OffsetDateTime,
    pub event: TasksEvent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Recorded {
    pub stored: usize,
    pub skipped: usize,
}

pub async fn delete_task(
    db: &Db,
    runs: &Runs,
    task_archives: &TaskArchiveBucket,
    user: &UserId,
    project: &ProjectId,
    task: &TaskId,
) -> Result<TaskArchiveState, TasksError> {
    let project = projects::require_member(db, user, project).await?;
    let _idle = runs.lock_idle_task(&project, task).await?;
    let archive = task_archives.delete(&project.id, task).await?;
    let deleted = TaskDeleted {
        project: project.id,
        task: task.clone(),
    };
    let event = Event::Tasks(TasksEvent::TaskDeleted(deleted));
    events::append(db, Actor::User(user.clone()), event).await?;
    Ok(archive)
}

pub async fn record_events(
    db: &Db,
    user: &UserId,
    reported: Vec<Reported>,
) -> Result<Recorded, TasksError> {
    require_allowed_events(&reported)?;
    require_member_of_all(db, user, &reported).await?;
    let entries: Vec<Entry> = reported
        .into_iter()
        .map(|reported| Entry {
            id: reported.id,
            at: reported.at,
            actor: Actor::User(user.clone()),
            event: Event::Tasks(reported.event),
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
    event: &TasksEvent,
    stamp: &Stamp<'_>,
) -> Result<(), ApplyError> {
    match event {
        TasksEvent::TaskStarted(started) => refuse_unless(
            store::insert_task(conn, started, stamp).await?,
            "the task is open already",
        ),
        TasksEvent::TaskClosed(closed) => refuse_unless(
            store::update_task_closed(conn, closed, stamp).await?,
            "the task is not open",
        ),
        TasksEvent::TaskDeleted(deleted) => Ok(store::delete_task(conn, deleted).await?),
    }
}

fn require_allowed_events(reported: &[Reported]) -> Result<(), TasksError> {
    match reported
        .iter()
        .find(|r| matches!(r.event, TasksEvent::TaskDeleted(_)))
    {
        Some(r) => Err(TasksError::EventNotAllowed {
            kind: r.event.kind(),
        }),
        None => Ok(()),
    }
}

async fn require_member_of_all(
    db: &Db,
    user: &UserId,
    reported: &[Reported],
) -> Result<(), TasksError> {
    let projects: HashSet<&ProjectId> = reported.iter().map(|r| r.event.project()).collect();
    for project in projects {
        projects::require_member(db, user, project).await?;
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
