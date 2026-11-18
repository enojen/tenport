use std::path::Path;

use super::*;
use crate::accounts::{OrgId, Role};
use crate::fixtures::{project_of, register};
use crate::projects::Repo;
use crate::run_credentials::RunCredentialKey;
use crate::task_archives::TaskArchiveBucket;
use std::time::Duration;

use futures::future::join_all;

use crate::cluster::{FakeCluster, JobName};
use crate::db;

const STARTED_TOGETHER: usize = 8;
const CREATE_TIME: Duration = Duration::from_millis(50);

fn runs_on(cluster: Arc<FakeCluster>) -> Runs {
    let tests = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let template = RunTemplate {
        dir: tests.join("run-template"),
        services_dir: tests.join("run-services"),
    };
    Runs::new(
        cluster,
        template,
        "box:1".to_owned(),
        test_bucket(),
        RunCredentialKey::new(RUN_CREDENTIAL_KEY),
        test_github(),
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
    let owner = register(&db, "Wegobi", "enojen", Role::Admin).await;
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
    let owner = register(&db, "Wegobi", "enojen", Role::Admin).await;
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
    let owner = register(&db, "Wegobi", "enojen", Role::Admin).await;
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
    let owner = register(&db, "Wegobi", "enojen", Role::Admin).await;
    let stranger = register(&db, "Other", "someone", Role::Admin).await;
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

#[test]
fn the_run_stamp_is_month_day_and_time_in_utc() {
    let at = OffsetDateTime::from_unix_timestamp(1_794_906_303).unwrap();
    assert_eq!(run_stamp(at), "1117-090503");
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

fn project_named(name: &str) -> Project {
    Project {
        id: ProjectId::parse("7ihtah").unwrap(),
        org: OrgId::parse("o-5bt4rj").unwrap(),
        name: Name::parse(name).unwrap(),
        repo: Repo::parse("enojen/tenport").unwrap(),
    }
}

#[test]
fn a_long_project_label_is_cut_to_63_characters_that_end_in_a_letter_or_digit() {
    let name = format!("{}-.x", "a".repeat(61));

    let label = project_label(&project_named(&name));

    assert_eq!(label, "a".repeat(61));
}

#[test]
fn a_project_name_that_is_a_label_value_stays_as_it_is() {
    assert_eq!(
        project_label(&project_named("Tenbox_demo.2")),
        "Tenbox_demo.2"
    );
}
