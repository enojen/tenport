use std::time::{Duration, Instant};

use futures::future::join_all;
use reqwest::StatusCode;
use reqwest::header::{LINK, RETRY_AFTER};
use serde_json::{Value, json};
use tenport_server::accounts::{Role, Token};
use tenport_server::cluster::{FakeCluster, JobName, PodName, PodState};
use tenport_server::events::{self, Actor, Event};
use tenport_server::http::Limits;
use tenport_server::projects::ProjectId;
use tenport_server::runs::{ClassName, CommandName, RunName, RunStarted, RunsEvent};
use tenport_server::tasks::TaskId;

use crate::runs::{build, get, post_run, project_of, started_run};
use crate::server::TestServer;

async fn problem(response: reqwest::Response) -> Value {
    response.json().await.unwrap()
}

#[tokio::test]
async fn a_method_a_path_does_not_have_is_a_problem() {
    let server = TestServer::start().await;
    let token = server
        .add_user_with_agent("Wegobi", "enojen", Role::Admin)
        .await;
    let response = reqwest::Client::new()
        .put(server.url("/runs"))
        .bearer_auth(token.expose())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(problem(response).await["code"], "method-not-allowed");
}

#[tokio::test]
async fn a_body_over_the_limit_is_payload_too_large() {
    let server = TestServer::start().await;
    let token = server
        .add_user_with_agent("Wegobi", "enojen", Role::Admin)
        .await;
    let response = reqwest::Client::new()
        .post(server.url("/events"))
        .bearer_auth(token.expose())
        .header("content-type", "application/json")
        .body(vec![b'a'; 3 * 1024 * 1024])
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(problem(response).await["code"], "payload-too-large");
}

#[tokio::test]
async fn an_empty_project_id_is_a_bad_request() {
    let server = TestServer::start().await;
    let token = server
        .add_user_with_agent("Wegobi", "enojen", Role::Admin)
        .await;
    let response = get(&server, &token, "/runs?project=&task=TEN-1").await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a_bad_query_says_only_what_is_wrong() {
    let server = TestServer::start().await;
    let token = server
        .add_user_with_agent("Wegobi", "enojen", Role::Admin)
        .await;
    let response = get(&server, &token, "/runs?task=TEN-1").await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let detail = problem(response).await["detail"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(detail, "missing field `project`");
}

#[tokio::test]
async fn a_user_over_the_request_rate_gets_too_many_requests_with_retry_after() {
    let server = TestServer::start().await;
    let token = server
        .add_user_with_agent("Wegobi", "enojen", Role::Admin)
        .await;
    let answers = join_all((0..30).map(|_| get(&server, &token, "/me"))).await;
    let refused: Vec<&reqwest::Response> = answers
        .iter()
        .filter(|response| response.status() == StatusCode::TOO_MANY_REQUESTS)
        .collect();
    assert!(!refused.is_empty());
    assert!(refused[0].headers().contains_key(RETRY_AFTER));
    let other = server
        .add_user_with_agent("Other", "someone", Role::Admin)
        .await;
    assert_eq!(get(&server, &other, "/me").await.status(), StatusCode::OK);
}

#[tokio::test]
async fn a_request_over_the_time_limit_is_a_timeout() {
    let limits = Limits {
        request_timeout: Duration::from_secs(1),
        ..Limits::default()
    };
    let cluster = FakeCluster::with_create_time(Duration::from_secs(5));
    let server = TestServer::start_with(cluster, limits).await;
    let token = server
        .add_user_with_agent("Wegobi", "enojen", Role::Admin)
        .await;
    let project = project_of(&server, &token).await;
    let response = post_run(&server, &token, &build(&project)).await;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(problem(response).await["code"], "timeout");
}

#[tokio::test]
async fn requests_over_the_concurrent_limit_wait_for_a_free_place() {
    let limits = Limits {
        concurrent_requests: 1,
        ..Limits::default()
    };
    let cluster = FakeCluster::with_create_time(Duration::from_millis(400));
    let server = TestServer::start_with(cluster, limits).await;
    let token = server
        .add_user_with_agent("Wegobi", "enojen", Role::Admin)
        .await;
    let project = project_of(&server, &token).await;
    let body = build(&project);
    let slow = post_run(&server, &token, &body);
    let quick = async {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let started = Instant::now();
        let response = get(&server, &token, "/me").await;
        (response.status(), started.elapsed())
    };
    let (_, (status, waited)) = tokio::join!(slow, quick);
    assert_eq!(status, StatusCode::OK);
    assert!(waited >= Duration::from_millis(200), "{waited:?}");
}

#[tokio::test]
async fn a_user_opens_five_followed_logs_at_most() {
    let server = TestServer::start().await;
    let token = server
        .add_user_with_agent("Wegobi", "enojen", Role::Admin)
        .await;
    let (_, name) = started_run(&server, &token).await;
    let pod = PodState {
        name: PodName::new(format!("{name}-abcde")),
        started: true,
        box_reason: None,
        exit_code: None,
        pod_reason: None,
    };
    server
        .cluster
        .set_pod(&JobName::new(&name), pod, "cloning\n");
    let path = format!("/runs/{name}/log?follow=true");
    let mut open = Vec::new();
    for _ in 0..5 {
        let response = get(&server, &token, &path).await;
        assert_eq!(response.status(), StatusCode::OK);
        open.push(response);
    }
    let refused = get(&server, &token, &path).await;
    assert_eq!(refused.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(refused.headers().contains_key(RETRY_AFTER));
    assert_eq!(problem(refused).await["code"], "too-many-log-streams");
}

async fn add_project(server: &TestServer, token: &Token, organization: &str, name: &str) {
    let response = reqwest::Client::new()
        .post(server.url("/projects"))
        .bearer_auth(token.expose())
        .json(&json!({ "organization": organization, "name": name, "repo": "enojen/tenport" }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
}

async fn organization_of(server: &TestServer, token: &Token) -> String {
    let me: Value = get(server, token, "/me").await.json().await.unwrap();
    me["organizations"][0]["id"].as_str().unwrap().to_owned()
}

fn next_link(response: &reqwest::Response) -> Option<String> {
    let link = response.headers().get(LINK)?.to_str().unwrap();
    let start = link.find('<')? + 1;
    let end = link.find('>')?;
    assert!(link.ends_with("rel=\"next\""), "{link}");
    Some(link[start..end].to_owned())
}

fn names(list: &Value) -> Vec<&str> {
    list.as_array()
        .unwrap()
        .iter()
        .map(|item| item["name"].as_str().unwrap())
        .collect()
}

#[tokio::test]
async fn the_projects_come_in_pages_with_a_link_to_the_next() {
    let server = TestServer::start().await;
    let token = server
        .add_user_with_agent("Wegobi", "enojen", Role::Admin)
        .await;
    let organization = organization_of(&server, &token).await;
    for name in ["Alpha", "Beta", "Gamma"] {
        add_project(&server, &token, &organization, name).await;
    }
    let first = get(&server, &token, "/projects?limit=2").await;
    let next = next_link(&first).unwrap();
    let first: Value = first.json().await.unwrap();
    assert_eq!(names(&first), ["Alpha", "Beta"]);
    let second = get(&server, &token, &next).await;
    assert!(next_link(&second).is_none());
    let second: Value = second.json().await.unwrap();
    assert_eq!(names(&second), ["Gamma"]);
}

#[tokio::test]
async fn a_list_without_limit_is_one_page_of_fifty() {
    let server = TestServer::start().await;
    let token = server
        .add_user_with_agent("Wegobi", "enojen", Role::Admin)
        .await;
    let organization = organization_of(&server, &token).await;
    add_project(&server, &token, &organization, "Alpha").await;
    let response = get(&server, &token, "/projects").await;
    assert!(next_link(&response).is_none());
    let list: Value = response.json().await.unwrap();
    assert_eq!(names(&list), ["Alpha"]);
}

#[tokio::test]
async fn a_limit_out_of_range_is_a_bad_request() {
    let server = TestServer::start().await;
    let token = server
        .add_user_with_agent("Wegobi", "enojen", Role::Admin)
        .await;
    for limit in ["0", "101", "x"] {
        let response = get(&server, &token, &format!("/projects?limit={limit}")).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{limit}");
    }
}

async fn record_run(server: &TestServer, project: &str, run: &str) {
    let started = RunStarted {
        project: ProjectId::parse(project).unwrap(),
        task: TaskId::parse("TEN-301").unwrap(),
        run: RunName::from(JobName::new(run)),
        command: CommandName::Build,
        class: ClassName::parse("build").unwrap(),
        agent: None,
    };
    let event = Event::Runs(RunsEvent::RunStarted(started));
    events::append(&server.db, Actor::Server, event)
        .await
        .unwrap();
}

#[tokio::test]
async fn the_runs_of_a_task_come_in_pages_with_a_link_to_the_next() {
    let server = TestServer::start().await;
    let token = server
        .add_user_with_agent("Wegobi", "enojen", Role::Admin)
        .await;
    let project = project_of(&server, &token).await;
    for run in ["task-ten-301-a", "task-ten-301-b", "task-ten-301-c"] {
        record_run(&server, &project, run).await;
    }
    let path = format!("/runs?project={project}&task=TEN-301&limit=2");
    let first = get(&server, &token, &path).await;
    let next = next_link(&first).unwrap();
    let first: Value = first.json().await.unwrap();
    assert_eq!(names(&first), ["task-ten-301-a", "task-ten-301-b"]);
    let second = get(&server, &token, &next).await;
    assert!(next_link(&second).is_none());
    let second: Value = second.json().await.unwrap();
    assert_eq!(names(&second), ["task-ten-301-c"]);
}
