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

use crate::accounts::{Email, UserId};
use crate::cluster::{Cluster, ClusterError, JobPhase};
use crate::db::Db;
use crate::events::{self, Actor, ApplyError, Event, EventsError, Stamp};
use crate::name::Name;
use crate::projects::{self, Project, ProjectId, ProjectsError};
use crate::tasks::TaskId;

pub use command::{Command, CommandError, CommandName, command_rules};
pub use model::{
    Branch, BranchError, ClassName, ClassNameError, RunName, RunStarted, RunsEvent, ServiceName,
    ServiceNameError,
};
pub use template::{RunTemplate, TemplateError};

use template::TemplateFields;

const GITHUB: &str = "https://github.com";

#[derive(Debug, thiserror::Error)]
pub enum RunsError {
    #[error("only a member of the organization of the project {project} starts its runs")]
    NotMember { project: ProjectId },
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    pub name: RunName,
    pub project: ProjectId,
    pub task: TaskId,
}

pub struct Runs {
    pub cluster: Arc<dyn Cluster>,
    pub template: RunTemplate,
    pub box_image: String,
}

impl Runs {
    pub async fn start(&self, db: &Db, user: &UserId, new: NewRun) -> Result<Run, RunsError> {
        let project = projects::find_project_for_member(db, user, &new.project)
            .await?
            .ok_or_else(|| RunsError::NotMember {
                project: new.project.clone(),
            })?;
        self.require_idle(&project, &new.task).await?;
        let fields = self.fields(&project, &new, OffsetDateTime::now_utc());
        let job = self.template.job(&fields, &new.services)?;
        let name = RunName::from(self.cluster.create_job(job).await?);
        let started = RunStarted {
            project: project.id.clone(),
            task: new.task.clone(),
            run: name.clone(),
            command: new.command.name(),
            class: new.class,
        };
        let event = Event::Runs(RunsEvent::RunStarted(started));
        events::append(db, Actor::User(user.clone()), event).await?;
        Ok(Run {
            name,
            project: project.id,
            task: new.task,
        })
    }

    async fn require_idle(&self, project: &Project, task: &TaskId) -> Result<(), RunsError> {
        let selector = format!("project-id={},task={task}", project.id);
        let jobs = self.cluster.list_jobs(&selector).await?;
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
        TemplateFields {
            name: kube_task,
            task: new.task.to_string(),
            run,
            project: project.name.as_str().to_owned(),
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
            archive_get: String::new(),
            archive_put: String::new(),
            setup: new.setup.clone(),
        }
    }
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
