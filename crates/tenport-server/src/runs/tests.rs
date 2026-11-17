use std::path::Path;

use super::*;
use crate::accounts::{self, GithubLogin, NewUser, Registered, Registration, Role};
use std::time::Duration;

use futures::FutureExt;
use futures::future::{BoxFuture, join_all};
use k8s_openapi::api::batch::v1::Job;

use crate::cluster::{FakeCluster, JobName, PodName};
use crate::db;
use crate::projects::{NewProject, Repo};

const STARTED_TOGETHER: usize = 8;
const CREATE_TIME: Duration = Duration::from_millis(50);

struct SlowCreateCluster(FakeCluster);

impl Cluster for SlowCreateCluster {
    fn create_job(&self, job: Job) -> BoxFuture<'_, Result<JobName, ClusterError>> {
        async move {
            tokio::time::sleep(CREATE_TIME).await;
            self.0.create_job(job).await
        }
        .boxed()
    }

    fn list_jobs<'a>(
        &'a self,
        selector: &'a str,
    ) -> BoxFuture<'a, Result<Vec<JobState>, ClusterError>> {
        self.0.list_jobs(selector)
    }

    fn find_pod<'a>(
        &'a self,
        job: &'a JobName,
    ) -> BoxFuture<'a, Result<Option<PodState>, ClusterError>> {
        self.0.find_pod(job)
    }

    fn read_log<'a>(
        &'a self,
        pod: &'a PodName,
        read: LogRead,
    ) -> BoxFuture<'a, Result<LogReader, ClusterError>> {
        self.0.read_log(pod, read)
    }

    fn delete_job<'a>(&'a self, job: &'a JobName) -> BoxFuture<'a, Result<bool, ClusterError>> {
        self.0.delete_job(job)
    }
}

async fn register(db: &Db, org: &str, github: &str) -> Registered {
    let registration = Registration {
        org: Name::parse(org).unwrap(),
        user: NewUser {
            name: Name::parse(github).unwrap(),
            email: None,
            github: GithubLogin::parse(github).unwrap(),
        },
        role: Role::Admin,
        token_name: Name::parse("laptop").unwrap(),
    };
    accounts::register(db, Actor::Server, registration)
        .await
        .unwrap()
}

async fn project_of(db: &Db, owner: &Registered) -> ProjectId {
    let new = NewProject {
        org: owner.org.clone(),
        name: Name::parse("Tenport").unwrap(),
        repo: Repo::parse("enojen/tenport").unwrap(),
    };
    projects::add(db, &owner.user, new).await.unwrap().id
}

fn runs_on(cluster: Arc<dyn Cluster>) -> Runs {
    let tests = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let template = RunTemplate {
        dir: tests.join("run-template"),
        services_dir: tests.join("run-services"),
    };
    Runs::new(cluster, template, "box:1".to_owned())
}

fn build(project: &ProjectId) -> NewRun {
    let task = TaskId::parse("TEN-1").unwrap();
    let words = ["tenbox", "build", "TEN-1"].map(str::to_owned).to_vec();
    NewRun {
        project: project.clone(),
        command: Command::parse(words, &task).unwrap(),
        task,
        base: Branch::parse("main").unwrap(),
        class: ClassName::parse("build").unwrap(),
        env: BTreeMap::new(),
        setup: String::new(),
        services: Vec::new(),
        author: Author {
            name: Name::parse("Enes").unwrap(),
            email: Email::parse("1+enojen@users.noreply.github.com").unwrap(),
        },
    }
}

async fn count_runs(db: &Db) -> i64 {
    store::count_runs(&mut db.acquire().await.unwrap())
        .await
        .unwrap()
}

#[tokio::test]
async fn a_started_run_is_a_job_on_the_cluster_and_a_run_in_the_database() {
    let (db, _dir) = db::temp().await;
    let owner = register(&db, "Wegobi", "enojen").await;
    let project = project_of(&db, &owner).await;
    let cluster = Arc::new(FakeCluster::default());
    let run = runs_on(cluster.clone())
        .start(&db, &owner.user, build(&project))
        .await
        .unwrap();
    assert!(run.name.as_str().starts_with("task-ten-1-"));
    let jobs = cluster.jobs();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].metadata.name.as_deref(), Some(run.name.as_str()));
    assert_eq!(count_runs(&db).await, 1);
}

#[tokio::test]
async fn a_second_run_of_the_task_waits_until_the_first_job_ends() {
    let (db, _dir) = db::temp().await;
    let owner = register(&db, "Wegobi", "enojen").await;
    let project = project_of(&db, &owner).await;
    let cluster = Arc::new(FakeCluster::default());
    let runs = runs_on(cluster.clone());
    let first = runs.start(&db, &owner.user, build(&project)).await.unwrap();
    let busy = runs.start(&db, &owner.user, build(&project)).await;
    assert!(matches!(busy, Err(RunsError::Busy { jobs, .. }) if jobs == [first.name.to_string()]));
    cluster.set_phase(&JobName::new(first.name.as_str()), JobPhase::Succeeded);
    let after_end = runs.start(&db, &owner.user, build(&project)).await;
    assert!(!matches!(after_end, Err(RunsError::Busy { .. })));
}

#[tokio::test]
async fn runs_of_the_task_started_together_make_one_job() {
    let (db, _dir) = db::temp().await;
    let owner = register(&db, "Wegobi", "enojen").await;
    let project = project_of(&db, &owner).await;
    let cluster = Arc::new(SlowCreateCluster(FakeCluster::default()));
    let runs = runs_on(cluster.clone());
    let starts = (0..STARTED_TOGETHER).map(|_| runs.start(&db, &owner.user, build(&project)));
    let started = join_all(starts).await;
    let busy = started
        .iter()
        .filter(|started| matches!(started, Err(RunsError::Busy { .. })))
        .count();
    assert_eq!(busy, STARTED_TOGETHER - 1);
    assert_eq!(cluster.0.jobs().len(), 1);
}

#[tokio::test]
async fn only_a_member_of_the_projects_organization_starts_a_run() {
    let (db, _dir) = db::temp().await;
    let owner = register(&db, "Wegobi", "enojen").await;
    let stranger = register(&db, "Other", "someone").await;
    let project = project_of(&db, &owner).await;
    let cluster = Arc::new(FakeCluster::default());
    let refused = runs_on(cluster.clone())
        .start(&db, &stranger.user, build(&project))
        .await;
    assert!(matches!(refused, Err(RunsError::NotMember { .. })));
    assert!(cluster.jobs().is_empty());
}

#[test]
fn the_run_stamp_is_month_day_and_time_in_utc() {
    let at = OffsetDateTime::from_unix_timestamp(1_794_906_303).unwrap();
    assert_eq!(run_stamp(at), "1117-090503");
}
