use std::path::Path;

use super::*;
use crate::accounts::{self, GithubLogin, NewUser, Registered, Registration, Role};
use crate::cluster::{FakeCluster, JobName};
use crate::db;
use crate::projects::{NewProject, Repo};

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

fn runs_on(cluster: Arc<FakeCluster>) -> Runs {
    let tests = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    Runs {
        cluster,
        template: RunTemplate {
            dir: tests.join("run-template"),
            services_dir: tests.join("run-services"),
        },
        box_image: "box:1".to_owned(),
    }
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
