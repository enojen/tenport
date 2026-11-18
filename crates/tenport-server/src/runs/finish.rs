use std::sync::Arc;
use std::time::Duration;

use futures::AsyncReadExt;
use serde_json::Value;

use super::model::{Finish, FinishedPhase, Run, RunFinished, RunPhase, RunState, RunsEvent};
use super::{Runs, RunsError, find_job_in, find_run, store};
use crate::cluster::{ClusterError, JobPhase, JobState, PodName, PodState, TENPORT_JOBS};
use crate::db::Db;
use crate::events::{self, Actor, Event, EventsError};

const BACKOFF_LIMIT_EXCEEDED: &str = "BackoffLimitExceeded";
const OBJECT_START: &str = "{";
const OBJECT_END: &str = "}";
const OOM_KILL: &str = "oom_kill ";

impl Runs {
    pub async fn record_finished_runs_every(self: Arc<Self>, db: Db, period: Duration) {
        let mut ticks = tokio::time::interval(period);
        loop {
            ticks.tick().await;
            if let Err(error) = self.record_finished_runs(&db).await {
                tracing::warn!(%error, "cannot record the finished runs");
            }
        }
    }

    pub async fn record_finished_runs(&self, db: &Db) -> Result<usize, RunsError> {
        let runs = list_unfinished_runs(db).await?;
        if runs.is_empty() {
            return Ok(0);
        }
        let jobs = self.cluster.list_jobs(TENPORT_JOBS).await?;
        let mut recorded = 0;
        for run in runs {
            let job = find_job_in(&jobs, &run);
            if job.is_some_and(JobState::is_running) {
                continue;
            }
            let name = run.name.clone();
            match self.run_state(db, run, job).await {
                Ok(_) => recorded += 1,
                Err(error) => {
                    tracing::warn!(run = %name, %error, "cannot record the finish of the run")
                }
            }
        }
        Ok(recorded)
    }

    pub(super) async fn run_state(
        &self,
        db: &Db,
        run: Run,
        job: Option<&JobState>,
    ) -> Result<RunState, RunsError> {
        if let Some(finish) = run.finished.clone() {
            return Ok(finished_state(run, finish));
        }
        let Some(job) = job else {
            return self.finish(db, run, Finish::unknown()).await;
        };
        match job.phase {
            JobPhase::Running => self.running_state(run, job).await,
            JobPhase::Succeeded => {
                self.finish_from_job(db, run, job, FinishedPhase::Succeeded)
                    .await
            }
            JobPhase::Failed => {
                self.finish_from_job(db, run, job, FinishedPhase::Failed)
                    .await
            }
        }
    }

    pub(super) async fn finish(
        &self,
        db: &Db,
        run: Run,
        finish: Finish,
    ) -> Result<RunState, RunsError> {
        let finished = RunFinished {
            project: run.project.clone(),
            task: run.task.clone(),
            run: run.name.clone(),
            finish: finish.clone(),
        };
        let event = Event::Runs(RunsEvent::RunFinished(finished));
        match events::append(db, Actor::Server, event).await {
            Ok(_) => Ok(finished_state(run, finish)),
            Err(EventsError::Refused { .. }) => recorded_state(db, run).await,
            Err(error) => Err(error.into()),
        }
    }

    async fn running_state(&self, run: Run, job: &JobState) -> Result<RunState, RunsError> {
        let pod = self.cluster.find_pod(&job.name).await?;
        Ok(RunState {
            run,
            phase: RunPhase::Running,
            reason: pod.and_then(|pod| pod.reason),
            exit_code: None,
            oom_kill: None,
            output: None,
        })
    }

    async fn finish_from_job(
        &self,
        db: &Db,
        run: Run,
        job: &JobState,
        phase: FinishedPhase,
    ) -> Result<RunState, RunsError> {
        let pod = self.cluster.find_pod(&job.name).await?;
        let log = match &pod {
            Some(pod) => self.read_whole_log(&pod.name).await?,
            None => String::new(),
        };
        let finish = Finish {
            phase,
            reason: finished_reason(job, pod.as_ref()),
            exit_code: pod.as_ref().and_then(|pod| pod.exit_code),
            oom_kill: oom_kill(&log),
            output: command_output(&log),
        };
        self.finish(db, run, finish).await
    }

    async fn read_whole_log(&self, pod: &PodName) -> Result<String, RunsError> {
        let mut reader = match self.cluster.read_log(pod, false).await {
            Err(ClusterError::NoPod(_)) => return Ok(String::new()),
            other => other?,
        };
        let mut bytes = Vec::new();
        reader
            .read_to_end(&mut bytes)
            .await
            .map_err(|error| RunsError::Log(error.to_string()))?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }
}

async fn recorded_state(db: &Db, run: Run) -> Result<RunState, RunsError> {
    let recorded = find_run(db, &run.name).await?;
    match recorded.finished.clone() {
        Some(finish) => Ok(finished_state(recorded, finish)),
        None => Err(RunsError::NotFound { run: run.name }),
    }
}

async fn list_unfinished_runs(db: &Db) -> Result<Vec<Run>, RunsError> {
    let mut conn = db.acquire().await?;
    Ok(store::list_unfinished_runs(&mut conn).await?)
}

fn finished_state(run: Run, finish: Finish) -> RunState {
    RunState {
        phase: finish.phase.into(),
        reason: finish.reason,
        exit_code: finish.exit_code,
        oom_kill: finish.oom_kill,
        output: finish.output,
        run,
    }
}

fn finished_reason(job: &JobState, pod: Option<&PodState>) -> Option<String> {
    let job_reason = job
        .failed_reason
        .clone()
        .filter(|reason| reason != BACKOFF_LIMIT_EXCEEDED);
    job_reason
        .or_else(|| pod.and_then(|pod| pod.pod_reason.clone()))
        .or_else(|| pod.and_then(|pod| pod.reason.clone()))
}

fn command_output(log: &str) -> Option<Value> {
    let lines: Vec<&str> = log.lines().collect();
    let end = lines.iter().rposition(|line| *line == OBJECT_END)?;
    let start = lines[..end]
        .iter()
        .rposition(|line| *line == OBJECT_START)?;
    let value: Value = serde_json::from_str(&lines[start..=end].join("\n")).ok()?;
    value.is_object().then_some(value)
}

fn oom_kill(log: &str) -> Option<u64> {
    log.lines()
        .rev()
        .find_map(|line| line.rsplit_once(OOM_KILL))
        .and_then(|(_, count)| count.trim().parse().ok())
}

#[cfg(test)]
mod tests;
