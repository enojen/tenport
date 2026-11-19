use std::path::Path;

use super::*;
use crate::accounts::Role;
use crate::fixtures::{add_user, add_user_with_agent, project_of};
use crate::run_credentials::{RunCredentialKey, RunCredentials};
use crate::task_archives::TaskArchiveBucket;
use std::time::Duration;

use futures::future::join_all;

use crate::cluster::{FakeCluster, JobName, JobPhase};
use crate::db;

const STARTED_TOGETHER: usize = 8;
const CREATE_TIME: Duration = Duration::from_millis(50);

fn runs_on(cluster: Arc<FakeCluster>) -> Runs {
    let tests = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let template = RunTemplate {
        dir: tests.join("run-template"),
        services_dir: tests.join("run-services"),
    };
    let run_credentials = RunCredentials::new(
        cluster.clone(),
        RunCredentialKey::new(RUN_CREDENTIAL_KEY),
        test_github(),
    );
    Runs::new(
        cluster,
        template,
        "box:1".to_owned(),
        Arc::new(test_bucket()),
        Arc::new(run_credentials),
    )
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
        agent: None,
    }
}

async fn count_runs(db: &Db) -> i64 {
    store::count_runs(db).await.unwrap()
}

#[tokio::test]
async fn a_started_run_is_a_job_on_the_cluster_and_a_run_in_the_database() {
    let (db, _dir) = db::temp().await;
    let owner = add_user_with_agent(&db, "Wegobi", "enojen", Role::Admin).await;
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
    let owner = add_user_with_agent(&db, "Wegobi", "enojen", Role::Admin).await;
    let project = project_of(&db, &owner).await;
    let cluster = Arc::new(FakeCluster::default());
    let runs = runs_on(cluster.clone());
    let first = runs.start(&db, &owner.user, build(&project)).await.unwrap();
    let busy = runs.start(&db, &owner.user, build(&project)).await;
    assert!(matches!(busy, Err(RunsError::Busy { jobs, .. }) if jobs == [first.name.to_string()]));
    cluster.set_phase(&JobName::from(&first.name), JobPhase::Succeeded);
    let after_end = runs.start(&db, &owner.user, build(&project)).await;
    assert!(!matches!(after_end, Err(RunsError::Busy { .. })));
}

#[tokio::test]
async fn runs_of_the_task_started_together_make_one_job() {
    let (db, _dir) = db::temp().await;
    let owner = add_user_with_agent(&db, "Wegobi", "enojen", Role::Admin).await;
    let project = project_of(&db, &owner).await;
    let cluster = Arc::new(FakeCluster::with_create_time(CREATE_TIME));
    let runs = runs_on(cluster.clone());
    let starts = (0..STARTED_TOGETHER).map(|_| runs.start(&db, &owner.user, build(&project)));
    let started = join_all(starts).await;
    let busy = started
        .iter()
        .filter(|started| matches!(started, Err(RunsError::Busy { .. })))
        .count();
    assert_eq!(busy, STARTED_TOGETHER - 1);
    assert_eq!(cluster.jobs().len(), 1);
}

#[tokio::test]
async fn only_a_member_of_the_projects_organization_starts_a_run() {
    let (db, _dir) = db::temp().await;
    let owner = add_user_with_agent(&db, "Wegobi", "enojen", Role::Admin).await;
    let stranger = add_user(&db, "Other", "someone", Role::Admin).await;
    let project = project_of(&db, &owner).await;
    let cluster = Arc::new(FakeCluster::default());
    let refused = runs_on(cluster.clone())
        .start(&db, &stranger.user, build(&project))
        .await;
    assert!(matches!(
        refused,
        Err(RunsError::Projects(ProjectsError::NotMember { .. }))
    ));
    assert!(cluster.jobs().is_empty());
}

pub const RUN_CREDENTIAL_KEY: &str = "key-for-tests";

fn test_github() -> crate::github::GithubApp {
    let key = include_str!("../../tests/github-app-test-key.pem");
    crate::github::GithubApp::new("1", key, "https://api.example.test").unwrap()
}

fn test_bucket() -> TaskArchiveBucket {
    TaskArchiveBucket::new(
        "https://objects.example.test/tenbox-test",
        "hel1",
        "key-id".to_owned(),
        "key-secret".to_owned(),
    )
    .unwrap()
}

async fn count_run_finished(db: &Db) -> i64 {
    sqlx::query_scalar("select count(*) from events where type = 'run.finished'")
        .fetch_one(&mut *db.acquire().await.unwrap())
        .await
        .unwrap()
}

#[tokio::test]
async fn the_timer_records_an_ended_run_once_and_leaves_a_running_one() {
    let (db, _dir) = db::temp().await;
    let owner = add_user_with_agent(&db, "Wegobi", "enojen", Role::Admin).await;
    let project = project_of(&db, &owner).await;
    let cluster = Arc::new(FakeCluster::default());
    let runs = runs_on(cluster.clone());
    let run = runs.start(&db, &owner.user, build(&project)).await.unwrap();

    assert_eq!(runs.record_finished_runs(&db).await.unwrap(), 0);
    cluster.set_phase(&JobName::from(&run.name), JobPhase::Succeeded);
    assert_eq!(runs.record_finished_runs(&db).await.unwrap(), 1);
    assert_eq!(runs.record_finished_runs(&db).await.unwrap(), 0);

    assert_eq!(count_run_finished(&db).await, 1);
    let recorded = find_run(&db, &run.name).await.unwrap();
    assert_eq!(recorded.finished.unwrap().phase, FinishedPhase::Succeeded);
}

#[tokio::test]
async fn a_run_whose_job_is_gone_before_its_end_was_seen_ends_unknown() {
    let (db, _dir) = db::temp().await;
    let owner = add_user_with_agent(&db, "Wegobi", "enojen", Role::Admin).await;
    let project = project_of(&db, &owner).await;
    let cluster = Arc::new(FakeCluster::default());
    let runs = runs_on(cluster.clone());
    let run = runs.start(&db, &owner.user, build(&project)).await.unwrap();
    cluster.delete_job(&JobName::from(&run.name)).await.unwrap();

    assert_eq!(runs.record_finished_runs(&db).await.unwrap(), 1);

    let recorded = find_run(&db, &run.name).await.unwrap();
    assert_eq!(recorded.finished.unwrap(), Finish::unknown());
}

#[tokio::test]
async fn a_second_end_of_a_run_is_refused_and_the_first_stays() {
    let (db, _dir) = db::temp().await;
    let owner = add_user_with_agent(&db, "Wegobi", "enojen", Role::Admin).await;
    let project = project_of(&db, &owner).await;
    let cluster = Arc::new(FakeCluster::default());
    let runs = runs_on(cluster.clone());
    let run = runs.start(&db, &owner.user, build(&project)).await.unwrap();
    cluster.set_phase(&JobName::from(&run.name), JobPhase::Failed);
    let first = runs
        .finish(&db, run.clone(), Finish::stopped())
        .await
        .unwrap();

    let second = runs
        .finish(&db, run.clone(), Finish::unknown())
        .await
        .unwrap();

    assert_eq!(first.phase, RunPhase::Failed);
    assert_eq!(second.phase, RunPhase::Failed);
    assert_eq!(count_run_finished(&db).await, 1);
}
