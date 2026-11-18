use sqlx::SqliteConnection;
use time::OffsetDateTime;

use super::command::CommandName;
use super::model::{ClassName, EndPhase, Run, RunEnd, RunFinished, RunName, RunStarted};
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
    phase: Option<EndPhase>,
    reason: Option<String>,
    exit_code: Option<i32>,
    oom_kill: Option<i64>,
    output: Option<String>,
}

impl RunRow {
    fn into_run(self) -> Result<Run, sqlx::Error> {
        let end = match self.phase {
            Some(phase) => Some(RunEnd {
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
            end,
        })
    }
}

fn parse_output(text: Option<&str>) -> Result<Option<serde_json::Value>, sqlx::Error> {
    text.map(serde_json::from_str)
        .transpose()
        .map_err(|error| sqlx::Error::Decode(Box::new(error)))
}

fn into_runs(rows: Vec<RunRow>) -> Result<Vec<Run>, sqlx::Error> {
    rows.into_iter().map(RunRow::into_run).collect()
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

pub async fn update_run_end(
    conn: &mut SqliteConnection,
    finished: &RunFinished,
    stamp: &Stamp<'_>,
) -> Result<bool, sqlx::Error> {
    let end = &finished.end;
    let output = end.output.as_ref().map(serde_json::Value::to_string);
    let oom_kill = end.oom_kill.and_then(|count| i64::try_from(count).ok());
    let updated = sqlx::query!(
        "update runs set finished_at = ?, phase = ?, reason = ?, exit_code = ?, oom_kill = ?,
                         output = ?
         where name = ? and finished_at is null",
        stamp.at,
        end.phase,
        end.reason,
        end.exit_code,
        oom_kill,
        output,
        finished.run,
    )
    .execute(conn)
    .await?;
    Ok(updated.rows_affected() == 1)
}

pub async fn find_run(
    conn: &mut SqliteConnection,
    name: &RunName,
) -> Result<Option<Run>, sqlx::Error> {
    let row = sqlx::query_as!(
        RunRow,
        r#"select name as "name: _", project_id as "project: _", task_id as "task: _",
                  command as "command: _", class as "class: _", created_at as "created: _",
                  phase as "phase: _", reason, exit_code as "exit_code: _", oom_kill, output
           from runs where name = ?"#,
        name,
    )
    .fetch_optional(conn)
    .await?;
    row.map(RunRow::into_run).transpose()
}

pub async fn list_runs_of_task(
    conn: &mut SqliteConnection,
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
    .fetch_all(conn)
    .await?;
    into_runs(rows)
}

pub async fn list_runs_without_end(conn: &mut SqliteConnection) -> Result<Vec<Run>, sqlx::Error> {
    let rows = sqlx::query_as!(
        RunRow,
        r#"select name as "name: _", project_id as "project: _", task_id as "task: _",
                  command as "command: _", class as "class: _", created_at as "created: _",
                  phase as "phase: _", reason, exit_code as "exit_code: _", oom_kill, output
           from runs where finished_at is null
           order by created_at, name"#,
    )
    .fetch_all(conn)
    .await?;
    into_runs(rows)
}

#[cfg(test)]
pub async fn count_runs(conn: &mut SqliteConnection) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!("select count(*) from runs")
        .fetch_one(conn)
        .await
}
