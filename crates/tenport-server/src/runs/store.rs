use sqlx::SqliteConnection;

use super::model::RunStarted;
use crate::events::Stamp;

pub async fn insert_run(
    conn: &mut SqliteConnection,
    started: &RunStarted,
    stamp: &Stamp<'_>,
) -> Result<(), sqlx::Error> {
    let command = started.command.as_str();
    sqlx::query!(
        "insert into runs (name, project_id, task_id, command, class, created_at, created_by)
         values (?, ?, ?, ?, ?, ?, ?)",
        started.run,
        started.project,
        started.task,
        command,
        started.class,
        stamp.at,
        stamp.actor,
    )
    .execute(conn)
    .await?;
    Ok(())
}

#[cfg(test)]
pub async fn count_runs(conn: &mut SqliteConnection) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!("select count(*) from runs")
        .fetch_one(conn)
        .await
}
