mod command;
mod finish;
mod job;
mod model;
mod store;

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::sync::Arc;

use sqlx::SqliteConnection;
use time::OffsetDateTime;
use tokio::sync::{Mutex, MutexGuard};

use crate::accounts::{Email, UserId};
use crate::run_credentials::{RUN_PLACEHOLDER_KEY, RunCredentials, run_placeholder_secret};
use crate::task_archives::TaskArchiveBucket;
use futures::future::try_join_all;

use crate::cluster::{
    Cluster, ClusterError, CreatedJob, JobName, JobState, LogReader, PROJECT_ID_LABEL, PodState,
    TASK_LABEL,
};
use crate::db::Db;
use crate::events::{self, Actor, ApplyError, Event, EventsError, Stamp};
use crate::name::Name;
use crate::projects::{self, Project, ProjectId, ProjectsError};
use crate::tasks::TaskId;

pub use command::{Command, CommandError, CommandName, command_rules};
pub use job::{RunTemplate, TemplateError};
pub use model::{
    Branch, BranchError, ClassName, ClassNameError, Finish, FinishedPhase, Run, RunFinished,
    RunName, RunNameError, RunPhase, RunStarted, RunState, RunsEvent, ServiceName,
    ServiceNameError,
};

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
    task_archives: Arc<TaskArchiveBucket>,
    run_credentials: Arc<RunCredentials>,
    start_lock: Mutex<()>,
}

impl Runs {
    pub fn new(
        cluster: Arc<dyn Cluster>,
        template: RunTemplate,
        box_image: String,
        task_archives: Arc<TaskArchiveBucket>,
        run_credentials: Arc<RunCredentials>,
    ) -> Self {
        Self {
            cluster,
            template,
            box_image,
            task_archives,
            run_credentials,
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
        let _idle = self.lock_idle_task(project, &new.task).await?;
        let archive = self.task_archives.presign(&project.id, &new.task);
        let fields = job::fields(
            project,
            new,
            &self.box_image,
            archive,
            OffsetDateTime::now_utc(),
        );
        let job = self.template.job(&fields, &new.services).await?;
        let created = self.cluster.create_job(job).await?;
        if let Err(error) = self.write_run_placeholder_secret(&created).await {
            self.cluster.delete_job(&created.name).await?;
            return Err(error);
        }
        Ok(RunName::from(created.name))
    }

    async fn write_run_placeholder_secret(&self, job: &CreatedJob) -> Result<(), RunsError> {
        let placeholder = self.run_credentials.placeholder(&job.uid);
        let secret = run_placeholder_secret(&job.name);
        self.cluster
            .create_job_secret(&secret, job, RUN_PLACEHOLDER_KEY, &placeholder)
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
        match self.cluster.read_log(&pod.name, follow).await {
            Err(ClusterError::NoPod(_)) => Err(RunsError::JobGone { run: run.name }),
            other => Ok(other?),
        }
    }

    pub async fn stop(&self, db: &Db, user: &UserId, name: &RunName) -> Result<(), RunsError> {
        let run = find_run_for_member(db, user, name).await?;
        let job = self.find_job(&run).await?;
        let running = job.as_ref().is_some_and(JobState::is_running);
        if !running {
            self.run_state(db, run.clone(), job.as_ref()).await?;
        }
        self.cluster
            .delete_job(&JobName::new(run.name.as_str()))
            .await?;
        if running && run.finished.is_none() {
            self.finish(db, run, Finish::stopped()).await?;
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

    async fn find_job(&self, run: &Run) -> Result<Option<JobState>, RunsError> {
        let jobs = self.list_jobs_of_task(&run.project, &run.task).await?;
        Ok(find_job_in(&jobs, run).cloned())
    }

    async fn list_jobs_of_task(
        &self,
        project: &ProjectId,
        task: &TaskId,
    ) -> Result<Vec<JobState>, RunsError> {
        let labels = [
            (PROJECT_ID_LABEL, project.as_str()),
            (TASK_LABEL, task.as_str()),
        ];
        Ok(self.cluster.list_jobs(&labels).await?)
    }

    pub async fn lock_idle_task(
        &self,
        project: &Project,
        task: &TaskId,
    ) -> Result<MutexGuard<'_, ()>, RunsError> {
        // Careful: two limits of this lock.
        // 1. One lock for all tasks: starts of different tasks also wait for each other.
        //    A start holds it for one Job list and one Job create, so this is fine while
        //    starts are rare. If many starts come at once, use a lock per task, and keep
        //    the map of locks from growing without end.
        // 2. It works only with one Tenport replica: the lock lives in one process.
        //    With two replicas the race comes back. Before that, move the check to a
        //    place both replicas share: a fixed Job name per task (Kubernetes answers
        //    409 to the second create), or a row in the database.
        let idle = self.start_lock.lock().await;
        self.require_idle(project, task).await?;
        Ok(idle)
    }

    async fn require_idle(&self, project: &Project, task: &TaskId) -> Result<(), RunsError> {
        let jobs = self.list_jobs_of_task(&project.id, task).await?;
        let running: Vec<String> = jobs
            .into_iter()
            .filter(|job| job.is_running())
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

fn find_job_in<'a>(jobs: &'a [JobState], run: &Run) -> Option<&'a JobState> {
    jobs.iter()
        .find(|job| job.name.as_str() == run.name.as_str())
}

pub(crate) async fn apply(
    conn: &mut SqliteConnection,
    event: &RunsEvent,
    stamp: &Stamp<'_>,
) -> Result<(), ApplyError> {
    match event {
        RunsEvent::RunStarted(started) => store::insert_run(conn, started, stamp).await?,
        RunsEvent::RunFinished(finished) => {
            if !store::update_run_finish(conn, finished, stamp).await? {
                return Err(ApplyError::Refused(
                    "the run has finished already, or Tenport does not know it",
                ));
            }
        }
    }
    Ok(())
}
