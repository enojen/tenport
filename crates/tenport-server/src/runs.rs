mod box_log;
mod command;
mod model;
mod store;
mod template;

#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::Duration;

use sqlx::SqliteConnection;
use time::OffsetDateTime;
use tokio::sync::Mutex;

use crate::accounts::{Email, UserId};
use crate::github::{GITHUB_HOSTS, GithubApp, GithubError, RepoToken};
use crate::run_credentials::{RUN_TOKEN_KEY, RunCredentialKey, run_token_secret};
use crate::task_archives::{TaskArchiveBucket, TaskArchiveState, TaskArchivesError};
use futures::AsyncReadExt;
use futures::future::try_join_all;

use crate::cluster::{
    Cluster, ClusterError, CreatedJob, JobName, JobPhase, JobState, JobUid, LogRead, LogReader,
    PodName, PodState,
};
use crate::db::Db;
use crate::events::{self, Actor, ApplyError, Event, EventsError, Stamp};
use crate::name::Name;
use crate::projects::{self, Project, ProjectId, ProjectsError};
use crate::tasks::{TaskDeleted, TaskId, TasksEvent};

pub use command::{Command, CommandError, CommandName, command_rules};
pub use model::{
    Branch, BranchError, ClassName, ClassNameError, EndPhase, Run, RunEnd, RunFinished, RunName,
    RunNameError, RunPhase, RunStarted, RunState, RunsEvent, ServiceName, ServiceNameError,
};
pub use template::{RunTemplate, TemplateError};

use template::TemplateFields;

const GITHUB: &str = "https://github.com";
const TENPORT_JOBS: &str = "app.kubernetes.io/managed-by=tenport";
const PROJECT_ID_LABEL: &str = "project-id";
const TOKEN_REUSE_MARGIN: time::Duration = time::Duration::minutes(5);
const LABEL_VALUE_MAX: usize = 63;
const BACKOFF_LIMIT_EXCEEDED: &str = "BackoffLimitExceeded";

#[derive(Debug, thiserror::Error)]
pub enum RunsError {
    #[error("Tenport does not know the run {run}")]
    NotFound { run: RunName },
    #[error("the box container of the run {run} has not started{}", reason_text(.reason))]
    NotStarted {
        run: RunName,
        reason: Option<String>,
    },
    #[error("cannot read the log of the box: {0}")]
    Log(String),
    #[error("the run {run} has no log: its Job is gone")]
    JobGone { run: RunName },
    #[error("task {task} still runs on the cluster: {}; wait until it ends", jobs.join(", "))]
    Busy { task: TaskId, jobs: Vec<String> },
    #[error(transparent)]
    Template(#[from] TemplateError),
    #[error(transparent)]
    TaskArchives(#[from] TaskArchivesError),
    #[error("no open Job of Tenport has this placeholder for {host}")]
    NoRunCredential { host: String },
    #[error(transparent)]
    Github(#[from] GithubError),
    #[error(transparent)]
    Cluster(#[from] ClusterError),
    #[error(transparent)]
    Projects(#[from] ProjectsError),
    #[error(transparent)]
    Events(#[from] EventsError),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

fn reason_text(reason: &Option<String>) -> String {
    reason
        .as_ref()
        .map(|reason| format!(": {reason}"))
        .unwrap_or_default()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Author {
    pub name: Name,
    pub email: Email,
}

#[derive(Debug, Clone)]
pub struct NewRun {
    pub project: ProjectId,
    pub task: TaskId,
    pub command: Command,
    pub base: Branch,
    pub class: ClassName,
    pub env: BTreeMap<String, String>,
    pub setup: String,
    pub services: Vec<ServiceName>,
    pub author: Author,
}

pub struct Runs {
    cluster: Arc<dyn Cluster>,
    template: RunTemplate,
    box_image: String,
    task_archives: TaskArchiveBucket,
    run_credential_key: RunCredentialKey,
    github: GithubApp,
    repo_tokens: std::sync::Mutex<HashMap<JobUid, RepoToken>>,
    start_lock: Mutex<()>,
}

impl Runs {
    pub fn new(
        cluster: Arc<dyn Cluster>,
        template: RunTemplate,
        box_image: String,
        task_archives: TaskArchiveBucket,
        run_credential_key: RunCredentialKey,
        github: GithubApp,
    ) -> Self {
        Self {
            cluster,
            template,
            box_image,
            task_archives,
            run_credential_key,
            github,
            repo_tokens: std::sync::Mutex::default(),
            start_lock: Mutex::new(()),
        }
    }

    pub async fn start(&self, db: &Db, user: &UserId, new: NewRun) -> Result<Run, RunsError> {
        let project = projects::require_member(db, user, &new.project).await?;
        let name = self.create_job_when_idle(&project, &new).await?;
        let started = RunStarted {
            project: project.id.clone(),
            task: new.task.clone(),
            run: name.clone(),
            command: new.command.name(),
            class: new.class,
        };
        let event = Event::Runs(RunsEvent::RunStarted(started));
        events::append(db, Actor::User(user.clone()), event).await?;
        find_run(db, &name).await
    }

    async fn create_job_when_idle(
        &self,
        project: &Project,
        new: &NewRun,
    ) -> Result<RunName, RunsError> {
        // Careful: two limits of this lock.
        // 1. One lock for all tasks: starts of different tasks also wait for each other.
        //    A start holds it for one Job list and one Job create, so this is fine while
        //    starts are rare. If many starts come at once, use a lock per task, and keep
        //    the map of locks from growing without end.
        // 2. It works only with one Tenport replica: the lock lives in one process.
        //    With two replicas the race comes back. Before that, move the check to a
        //    place both replicas share: a fixed Job name per task (Kubernetes answers
        //    409 to the second create), or a row in the database.
        let _one_start_at_a_time = self.start_lock.lock().await;
        self.require_idle(project, &new.task).await?;
        let fields = self.fields(project, new, OffsetDateTime::now_utc());
        let job = self.template.job(&fields, &new.services).await?;
        let created = self.cluster.create_job(job).await?;
        if let Err(error) = self.write_run_token_secret(&created).await {
            self.cluster.delete_job(&created.name).await?;
            return Err(error);
        }
        Ok(RunName::from(created.name))
    }

    async fn write_run_token_secret(&self, job: &CreatedJob) -> Result<(), RunsError> {
        let placeholder = self.run_credential_key.placeholder(&job.uid);
        let secret = run_token_secret(&job.name);
        self.cluster
            .create_job_secret(&secret, job, RUN_TOKEN_KEY, &placeholder)
            .await?;
        Ok(())
    }

    pub async fn list_of_task(
        &self,
        db: &Db,
        user: &UserId,
        project: &ProjectId,
        task: &TaskId,
        after: Option<&RunName>,
        limit: i64,
    ) -> Result<Vec<RunState>, RunsError> {
        projects::require_member(db, user, project).await?;
        let runs = list_runs_of_task(db, project, task, after, limit).await?;
        let jobs = self.list_jobs_of_task(project, task).await?;
        let states = runs.into_iter().map(|run| {
            let job = find_job_in(&jobs, &run);
            self.run_state(db, run, job)
        });
        try_join_all(states).await
    }

    pub async fn record_ended_runs_every(self: Arc<Self>, db: Db, period: Duration) {
        let mut ticks = tokio::time::interval(period);
        loop {
            ticks.tick().await;
            if let Err(error) = self.record_ended_runs(&db).await {
                tracing::warn!(%error, "cannot record the ended runs");
            }
        }
    }

    pub async fn record_ended_runs(&self, db: &Db) -> Result<usize, RunsError> {
        let runs = list_runs_without_end(db).await?;
        if runs.is_empty() {
            return Ok(0);
        }
        let jobs = self.cluster.list_jobs(TENPORT_JOBS).await?;
        let mut recorded = 0;
        for run in runs {
            let job = find_job_in(&jobs, &run);
            if job.is_some_and(|job| job.phase == JobPhase::Running) {
                continue;
            }
            let name = run.name.clone();
            match self.run_state(db, run, job).await {
                Ok(_) => recorded += 1,
                Err(error) => {
                    tracing::warn!(run = %name, %error, "cannot record the end of the run")
                }
            }
        }
        Ok(recorded)
    }

    pub async fn find(
        &self,
        db: &Db,
        user: &UserId,
        name: &RunName,
    ) -> Result<RunState, RunsError> {
        let run = find_run_for_member(db, user, name).await?;
        let job = self.find_job(&run).await?;
        self.run_state(db, run, job.as_ref()).await
    }

    pub async fn open_log(
        &self,
        db: &Db,
        user: &UserId,
        name: &RunName,
        follow: bool,
    ) -> Result<LogReader, RunsError> {
        let run = find_run_for_member(db, user, name).await?;
        let pod = self.find_started_pod(&run).await?;
        let read = LogRead {
            follow,
            tail_lines: None,
        };
        match self.cluster.read_log(&pod.name, read).await {
            Err(ClusterError::NoPod(_)) => Err(RunsError::JobGone { run: run.name }),
            other => Ok(other?),
        }
    }

    pub async fn run_credential(
        &self,
        db: &Db,
        placeholder: &str,
        host: &str,
    ) -> Result<String, RunsError> {
        let none = || RunsError::NoRunCredential {
            host: host.to_owned(),
        };
        if !GITHUB_HOSTS.contains(&host) {
            return Err(none());
        }
        let open: Vec<JobState> = self
            .cluster
            .list_jobs(TENPORT_JOBS)
            .await?
            .into_iter()
            .filter(|job| job.phase == JobPhase::Running)
            .collect();
        self.forget_tokens_of_ended_jobs(&open);
        let job = open
            .iter()
            .find(|job| self.run_credential_key.placeholder(&job.uid) == placeholder)
            .ok_or_else(none)?;
        let now = OffsetDateTime::now_utc();
        if let Some(token) = self.reusable_token(&job.uid, now) {
            return Ok(token);
        }
        let project = job
            .labels
            .get(PROJECT_ID_LABEL)
            .and_then(|id| ProjectId::parse(id).ok())
            .ok_or_else(none)?;
        let project = projects::find(db, &project).await?.ok_or_else(none)?;
        let token = self.github.repo_token(&project.repo, now).await?;
        let value = token.token.clone();
        lock(&self.repo_tokens).insert(job.uid.clone(), token);
        Ok(value)
    }

    fn reusable_token(&self, job: &JobUid, now: OffsetDateTime) -> Option<String> {
        lock(&self.repo_tokens)
            .get(job)
            .filter(|token| token.expires - now > TOKEN_REUSE_MARGIN)
            .map(|token| token.token.clone())
    }

    fn forget_tokens_of_ended_jobs(&self, open: &[JobState]) {
        lock(&self.repo_tokens).retain(|uid, _| open.iter().any(|job| job.uid == *uid));
    }

    pub async fn delete_task(
        &self,
        db: &Db,
        user: &UserId,
        project: &ProjectId,
        task: &TaskId,
    ) -> Result<TaskArchiveState, RunsError> {
        let project = projects::require_member(db, user, project).await?;
        let _no_start_meanwhile = self.start_lock.lock().await;
        self.require_idle(&project, task).await?;
        let archive = self.task_archives.delete(&project.id, task).await?;
        let deleted = TaskDeleted {
            project: project.id,
            task: task.clone(),
        };
        let event = Event::Tasks(TasksEvent::TaskDeleted(deleted));
        events::append(db, Actor::User(user.clone()), event).await?;
        Ok(archive)
    }

    pub async fn stop(&self, db: &Db, user: &UserId, name: &RunName) -> Result<(), RunsError> {
        let run = find_run_for_member(db, user, name).await?;
        let job = self.find_job(&run).await?;
        let running = job
            .as_ref()
            .is_some_and(|job| job.phase == JobPhase::Running);
        if !running {
            self.run_state(db, run.clone(), job.as_ref()).await?;
        }
        self.cluster
            .delete_job(&JobName::new(run.name.as_str()))
            .await?;
        if running && run.end.is_none() {
            self.finish(db, run, RunEnd::stopped()).await?;
        }
        Ok(())
    }

    async fn find_started_pod(&self, run: &Run) -> Result<PodState, RunsError> {
        let Some(job) = self.find_job(run).await? else {
            return Err(RunsError::JobGone {
                run: run.name.clone(),
            });
        };
        match self.cluster.find_pod(&job.name).await? {
            Some(pod) if pod.started => Ok(pod),
            pod => Err(RunsError::NotStarted {
                run: run.name.clone(),
                reason: pod.and_then(|pod| pod.reason),
            }),
        }
    }

    async fn run_state(
        &self,
        db: &Db,
        run: Run,
        job: Option<&JobState>,
    ) -> Result<RunState, RunsError> {
        if let Some(end) = run.end.clone() {
            return Ok(ended_state(run, end));
        }
        let Some(job) = job else {
            return self.finish(db, run, RunEnd::unknown()).await;
        };
        match job.phase {
            JobPhase::Running => self.running_state(run, job).await,
            JobPhase::Succeeded => self.finish_ended(db, run, job, EndPhase::Succeeded).await,
            JobPhase::Failed => self.finish_ended(db, run, job, EndPhase::Failed).await,
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

    async fn finish_ended(
        &self,
        db: &Db,
        run: Run,
        job: &JobState,
        phase: EndPhase,
    ) -> Result<RunState, RunsError> {
        let pod = self.cluster.find_pod(&job.name).await?;
        let log = match &pod {
            Some(pod) => self.read_whole_log(&pod.name).await?,
            None => String::new(),
        };
        let end = RunEnd {
            phase,
            reason: end_reason(job, pod.as_ref()),
            exit_code: pod.as_ref().and_then(|pod| pod.exit_code),
            oom_kill: box_log::oom_kill(&log),
            output: box_log::command_output(&log),
        };
        self.finish(db, run, end).await
    }

    async fn read_whole_log(&self, pod: &PodName) -> Result<String, RunsError> {
        let mut reader = match self.cluster.read_log(pod, LogRead::default()).await {
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

    async fn finish(&self, db: &Db, run: Run, end: RunEnd) -> Result<RunState, RunsError> {
        let finished = RunFinished {
            project: run.project.clone(),
            task: run.task.clone(),
            run: run.name.clone(),
            end: end.clone(),
        };
        let event = Event::Runs(RunsEvent::RunFinished(finished));
        match events::append(db, Actor::Server, event).await {
            Ok(_) => Ok(ended_state(run, end)),
            Err(EventsError::Refused { .. }) => self.recorded_state(db, run).await,
            Err(error) => Err(error.into()),
        }
    }

    async fn recorded_state(&self, db: &Db, run: Run) -> Result<RunState, RunsError> {
        let recorded = find_run(db, &run.name).await?;
        match recorded.end.clone() {
            Some(end) => Ok(ended_state(recorded, end)),
            None => Err(RunsError::NotFound { run: run.name }),
        }
    }

    async fn find_job(&self, run: &Run) -> Result<Option<JobState>, RunsError> {
        let jobs = self.list_jobs_of_task(&run.project, &run.task).await?;
        Ok(find_job_in(&jobs, run).cloned())
    }

    async fn list_jobs_of_task(
        &self,
        project: &ProjectId,
        task: &TaskId,
    ) -> Result<Vec<JobState>, RunsError> {
        let selector = format!("project-id={project},task={task}");
        Ok(self.cluster.list_jobs(&selector).await?)
    }

    async fn require_idle(&self, project: &Project, task: &TaskId) -> Result<(), RunsError> {
        let jobs = self.list_jobs_of_task(&project.id, task).await?;
        let running: Vec<String> = jobs
            .into_iter()
            .filter(|job| job.phase == JobPhase::Running)
            .map(|job| job.name.as_str().to_owned())
            .collect();
        if running.is_empty() {
            Ok(())
        } else {
            Err(RunsError::Busy {
                task: task.clone(),
                jobs: running,
            })
        }
    }

    fn fields(&self, project: &Project, new: &NewRun, now: OffsetDateTime) -> TemplateFields {
        let kube_task = kube_name(&new.task);
        let run = run_stamp(now);
        let archive = self.task_archives.presign(&project.id, &new.task);
        TemplateFields {
            name: kube_task,
            task: new.task.to_string(),
            run,
            project: project_label(project),
            project_id: project.id.to_string(),
            repo: format!("{GITHUB}/{}.git", project.repo.as_str()),
            base: new.base.as_str().to_owned(),
            git_name: new.author.name.as_str().to_owned(),
            git_email: new.author.email.to_string(),
            image: self.box_image.clone(),
            command: new.command.name().as_str().to_owned(),
            cmd: new.command.shell_line(),
            env: new.env.clone(),
            class: new.class.clone(),
            archive_get: archive.get,
            archive_put: archive.put,
            setup: new.setup.clone(),
        }
    }
}

async fn find_run_for_member(db: &Db, user: &UserId, name: &RunName) -> Result<Run, RunsError> {
    let run = find_run(db, name).await?;
    projects::require_member(db, user, &run.project).await?;
    Ok(run)
}

async fn find_run(db: &Db, name: &RunName) -> Result<Run, RunsError> {
    store::find_run(&mut *db.acquire().await?, name)
        .await?
        .ok_or_else(|| RunsError::NotFound { run: name.clone() })
}

async fn list_runs_without_end(db: &Db) -> Result<Vec<Run>, RunsError> {
    let mut conn = db.acquire().await?;
    Ok(store::list_runs_without_end(&mut conn).await?)
}

fn ended_state(run: Run, end: RunEnd) -> RunState {
    RunState {
        phase: end.phase.into(),
        reason: end.reason,
        exit_code: end.exit_code,
        oom_kill: end.oom_kill,
        output: end.output,
        run,
    }
}

fn end_reason(job: &JobState, pod: Option<&PodState>) -> Option<String> {
    let job_reason = job
        .failed_reason
        .clone()
        .filter(|reason| reason != BACKOFF_LIMIT_EXCEEDED);
    job_reason
        .or_else(|| pod.and_then(|pod| pod.pod_reason.clone()))
        .or_else(|| pod.and_then(|pod| pod.reason.clone()))
}

async fn list_runs_of_task(
    db: &Db,
    project: &ProjectId,
    task: &TaskId,
    after: Option<&RunName>,
    limit: i64,
) -> Result<Vec<Run>, RunsError> {
    let mut conn = db.acquire().await?;
    Ok(store::list_runs_of_task(&mut conn, project, task, after, limit).await?)
}

fn lock<T>(mutex: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn find_job_in<'a>(jobs: &'a [JobState], run: &Run) -> Option<&'a JobState> {
    jobs.iter()
        .find(|job| job.name.as_str() == run.name.as_str())
}

fn project_label(project: &Project) -> String {
    let mut value = String::new();
    for char in project.name.as_str().chars() {
        if is_label_char(char) {
            value.push(char);
        } else if !value.ends_with('-') {
            value.push('-');
        }
    }
    let value: String = value.chars().take(LABEL_VALUE_MAX).collect();
    let value = value.trim_matches(|char: char| !char.is_ascii_alphanumeric());
    if value.is_empty() {
        project.id.to_string()
    } else {
        value.to_owned()
    }
}

fn is_label_char(char: char) -> bool {
    char.is_ascii_alphanumeric() || matches!(char, '-' | '_' | '.')
}

fn kube_name(task: &TaskId) -> String {
    task.to_string().to_lowercase().replace('_', "-")
}

fn run_stamp(now: OffsetDateTime) -> String {
    format!(
        "{:02}{:02}-{:02}{:02}{:02}",
        u8::from(now.month()),
        now.day(),
        now.hour(),
        now.minute(),
        now.second()
    )
}

pub(crate) async fn apply(
    conn: &mut SqliteConnection,
    event: &RunsEvent,
    stamp: &Stamp<'_>,
) -> Result<(), ApplyError> {
    match event {
        RunsEvent::RunStarted(started) => store::insert_run(conn, started, stamp).await?,
        RunsEvent::RunFinished(finished) => {
            if !store::update_run_end(conn, finished, stamp).await? {
                return Err(ApplyError::Refused(
                    "the run has an end already, or Tenport does not know it",
                ));
            }
        }
    }
    Ok(())
}
