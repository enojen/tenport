mod command;
mod model;
mod store;
mod template;

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::sync::Arc;

use sqlx::SqliteConnection;
use time::OffsetDateTime;
use tokio::sync::Mutex;

use crate::accounts::{Email, UserId};
use crate::task_archives::{TaskArchiveBucket, TaskArchiveState, TaskArchivesError};
use futures::future::try_join_all;
use futures::try_join;

use crate::cluster::{
    Cluster, ClusterError, JobName, JobPhase, JobState, LogRead, LogReader, PodState,
};
use crate::db::Db;
use crate::events::{self, Actor, ApplyError, Event, EventsError, Stamp};
use crate::name::Name;
use crate::projects::{self, Project, ProjectId, ProjectsError};
use crate::tasks::{TaskDeleted, TaskId, TasksEvent};

pub use command::{Command, CommandError, CommandName, command_rules};
pub use model::{
    Branch, BranchError, ClassName, ClassNameError, Run, RunName, RunNameError, RunPhase,
    RunStarted, RunState, RunsEvent, ServiceName, ServiceNameError,
};
pub use template::{RunTemplate, TemplateError};

use template::TemplateFields;

const GITHUB: &str = "https://github.com";
const LABEL_VALUE_MAX: usize = 63;

#[derive(Debug, thiserror::Error)]
pub enum RunsError {
    #[error("Tenport does not know the run {run}")]
    NotFound { run: RunName },
    #[error("the box container of the run {run} has not started{}", reason_text(.reason))]
    NotStarted {
        run: RunName,
        reason: Option<String>,
    },
    #[error("the run {run} has no log: its Job is gone")]
    JobGone { run: RunName },
    #[error("task {task} still runs on the cluster: {}; wait until it ends", jobs.join(", "))]
    Busy { task: TaskId, jobs: Vec<String> },
    #[error(transparent)]
    Template(#[from] TemplateError),
    #[error(transparent)]
    TaskArchives(#[from] TaskArchivesError),
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
    start_lock: Mutex<()>,
}

impl Runs {
    pub fn new(
        cluster: Arc<dyn Cluster>,
        template: RunTemplate,
        box_image: String,
        task_archives: TaskArchiveBucket,
    ) -> Self {
        Self {
            cluster,
            template,
            box_image,
            task_archives,
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
        Ok(RunName::from(self.cluster.create_job(job).await?))
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
        let (runs, jobs) = try_join!(
            list_runs_of_task(db, project, task, after, limit),
            self.list_jobs_of_task(project, task)
        )?;
        let states = runs.into_iter().map(|run| {
            let job = find_job_in(&jobs, &run);
            self.run_state(run, job)
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
        self.run_state(run, job.as_ref()).await
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
        self.cluster
            .delete_job(&JobName::new(run.name.as_str()))
            .await?;
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

    async fn run_state(&self, run: Run, job: Option<&JobState>) -> Result<RunState, RunsError> {
        let Some(job) = job else {
            return Ok(RunState {
                run,
                phase: RunPhase::Unknown,
                reason: None,
            });
        };
        let pod = self.cluster.find_pod(&job.name).await?;
        Ok(RunState {
            run,
            phase: job.phase.into(),
            reason: pod.and_then(|pod| pod.reason),
        })
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
    }
    Ok(())
}
