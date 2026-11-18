use sqlx::SqliteConnection;

use super::model::{TaskClosed, TaskDeleted, TaskStarted};
use crate::events::Stamp;

pub async fn open_task(
    conn: &mut SqliteConnection,
    started: &TaskStarted,
    stamp: &Stamp<'_>,
) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        "insert into tasks (project_id, id, title, url, created_at, created_by)
         values (?, ?, ?, ?, ?, ?)
         on conflict (project_id, id) do update set closed_at = null, closed_by = null
         where tasks.closed_at is not null",
        started.project,
        started.task,
        started.title,
        started.url,
        stamp.at,
        stamp.actor,
    )
    .execute(conn)
    .await?;
    Ok(done.rows_affected() == 1)
}

pub async fn close_task(
    conn: &mut SqliteConnection,
    closed: &TaskClosed,
    stamp: &Stamp<'_>,
) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        "update tasks set closed_at = ?, closed_by = ?
         where project_id = ? and id = ? and closed_at is null",
        stamp.at,
        stamp.actor,
        closed.project,
        closed.task,
    )
    .execute(conn)
    .await?;
    Ok(done.rows_affected() == 1)
}

pub async fn delete_task(
    conn: &mut SqliteConnection,
    deleted: &TaskDeleted,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "delete from tasks where project_id = ? and id = ?",
        deleted.project,
        deleted.task,
    )
    .execute(conn)
    .await?;
    Ok(())
}

#[cfg(test)]
pub async fn count_open_tasks(executor: impl sqlx::SqliteExecutor<'_>) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!("select count(*) from tasks where closed_at is null")
        .fetch_one(executor)
        .await
}
