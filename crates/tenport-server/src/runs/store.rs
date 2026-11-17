use sqlx::SqliteConnection;

use super::model::{RunName, RunStarted, StoredRun};
use crate::events::Stamp;
use crate::projects::ProjectId;
use crate::tasks::TaskId;

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

pub async fn find_run(
    conn: &mut SqliteConnection,
    name: &RunName,
) -> Result<Option<StoredRun>, sqlx::Error> {
    sqlx::query_as!(
        StoredRun,
        r#"select name as "name: _", project_id as "project: _", task_id as "task: _",
                  command as "command: _", class as "class: _", created_at as "created: _"
           from runs where name = ?"#,
        name,
    )
    .fetch_optional(conn)
    .await
}

pub async fn list_runs_of_task(
    conn: &mut SqliteConnection,
    project: &ProjectId,
    task: &TaskId,
) -> Result<Vec<StoredRun>, sqlx::Error> {
    sqlx::query_as!(
        StoredRun,
        r#"select name as "name: _", project_id as "project: _", task_id as "task: _",
                  command as "command: _", class as "class: _", created_at as "created: _"
           from runs where project_id = ? and task_id = ? order by created_at, name"#,
        project,
        task,
    )
    .fetch_all(conn)
    .await
}

#[cfg(test)]
pub async fn count_runs(conn: &mut SqliteConnection) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!("select count(*) from runs")
        .fetch_one(conn)
        .await
}
