use sqlx::{SqliteConnection, SqliteExecutor};
use time::OffsetDateTime;

use super::command::CommandName;
use super::model::{ClassName, Finish, FinishedPhase, Run, RunFinished, RunName, RunStarted};
use crate::events::Stamp;
use crate::projects::ProjectId;
use crate::tasks::TaskId;

struct RunRow {
    name: RunName,
    project: ProjectId,
    task: TaskId,
    command: CommandName,
    class: ClassName,
    created: OffsetDateTime,
    phase: Option<FinishedPhase>,
    reason: Option<String>,
    exit_code: Option<i32>,
    oom_kill: Option<i64>,
    output: Option<String>,
}

impl RunRow {
    fn into_run(self) -> Result<Run, sqlx::Error> {
        let finished = match self.phase {
            Some(phase) => Some(Finish {
                phase,
                reason: self.reason,
                exit_code: self.exit_code,
                oom_kill: self.oom_kill.and_then(|count| u64::try_from(count).ok()),
                output: parse_output(self.output.as_deref())?,
            }),
            None => None,
        };
        Ok(Run {
            name: self.name,
            project: self.project,
            task: self.task,
            command: self.command,
            class: self.class,
            created: self.created,
            finished,
        })
    }
}

fn parse_output(text: Option<&str>) -> Result<Option<serde_json::Value>, sqlx::Error> {
    text.map(serde_json::from_str)
        .transpose()
        .map_err(|error| sqlx::Error::Decode(Box::new(error)))
}

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

pub async fn update_run_finish(
    conn: &mut SqliteConnection,
    finished: &RunFinished,
    stamp: &Stamp<'_>,
) -> Result<bool, sqlx::Error> {
    let finish = &finished.finish;
    let output = finish.output.as_ref().map(serde_json::Value::to_string);
    let oom_kill = finish.oom_kill.and_then(|count| i64::try_from(count).ok());
    let updated = sqlx::query!(
        "update runs set finished_at = ?, phase = ?, reason = ?, exit_code = ?, oom_kill = ?,
                         output = ?
         where name = ? and finished_at is null",
        stamp.at,
        finish.phase,
        finish.reason,
        finish.exit_code,
        oom_kill,
        output,
        finished.run,
    )
    .execute(conn)
    .await?;
    Ok(updated.rows_affected() == 1)
}

pub async fn find_run(
    executor: impl SqliteExecutor<'_>,
    name: &RunName,
) -> Result<Option<Run>, sqlx::Error> {
    let row = sqlx::query_as!(
        RunRow,
        r#"select name as "name: _", project_id as "project: _", task_id as "task: _",
                  command as "command: _", class as "class: _", created_at as "created: _",
                  phase as "phase: _", reason, exit_code as "exit_code: _", oom_kill, output
           from runs
           where name = ?"#,
        name,
    )
    .fetch_optional(executor)
    .await?;
    row.map(RunRow::into_run).transpose()
}

pub async fn list_runs_of_task(
    executor: impl SqliteExecutor<'_>,
    project: &ProjectId,
    task: &TaskId,
    after: Option<&RunName>,
    limit: i64,
) -> Result<Vec<Run>, sqlx::Error> {
    let rows = sqlx::query_as!(
        RunRow,
        r#"select name as "name: _", project_id as "project: _", task_id as "task: _",
                  command as "command: _", class as "class: _", created_at as "created: _",
                  phase as "phase: _", reason, exit_code as "exit_code: _", oom_kill, output
           from runs
           where project_id = ? and task_id = ?
             and (? is null
                  or (created_at, name) > (select created_at, name from runs where name = ?))
           order by created_at, name
           limit ?"#,
        project,
        task,
        after,
        after,
        limit,
    )
    .fetch_all(executor)
    .await?;
    rows.into_iter().map(RunRow::into_run).collect()
}

pub async fn list_unfinished_runs(
    executor: impl SqliteExecutor<'_>,
) -> Result<Vec<Run>, sqlx::Error> {
    let rows = sqlx::query_as!(
        RunRow,
        r#"select name as "name: _", project_id as "project: _", task_id as "task: _",
                  command as "command: _", class as "class: _", created_at as "created: _",
                  phase as "phase: _", reason, exit_code as "exit_code: _", oom_kill, output
           from runs
           where finished_at is null
           order by created_at, name"#,
    )
    .fetch_all(executor)
    .await?;
    rows.into_iter().map(RunRow::into_run).collect()
}

#[cfg(test)]
pub async fn count_runs(executor: impl SqliteExecutor<'_>) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!("select count(*) from runs")
        .fetch_one(executor)
        .await
}
