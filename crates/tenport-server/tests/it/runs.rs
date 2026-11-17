use reqwest::StatusCode;
use serde_json::{Value, json};
use tenport_server::accounts::{Role, Token};

use crate::server::TestServer;

async fn project_of(server: &TestServer, token: &Token) -> String {
    let me: Value = reqwest::Client::new()
        .get(server.url("/me"))
        .bearer_auth(token.expose())
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let org = me["organizations"][0]["id"].as_str().unwrap();
    let project: Value = reqwest::Client::new()
        .post(server.url("/projects"))
        .bearer_auth(token.expose())
        .json(&json!({ "org": org, "name": "Tenport", "repo": "enojen/tenport" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    project["id"].as_str().unwrap().to_owned()
}

fn build(project: &str) -> Value {
    json!({
        "project": project,
        "task": "TEN-301",
        "command": ["tenbox", "build", "TEN-301"],
        "base": "main",
        "class": "build",
        "env": { "NODE_ENV": "test" },
        "setup": "bun install",
        "services": ["postgres"],
        "author": { "name": "Enes", "email": "1+enojen@users.noreply.github.com" }
    })
}

async fn post_run(server: &TestServer, token: &Token, body: &Value) -> reqwest::Response {
    reqwest::Client::new()
        .post(server.url("/runs"))
        .bearer_auth(token.expose())
        .json(body)
        .send()
        .await
        .unwrap()
}

#[tokio::test]
async fn a_member_starts_a_run_and_tenport_creates_its_job() {
    let server = TestServer::start().await;
    let token = server.register("Wegobi", "enojen", Role::Admin).await;
    let project = project_of(&server, &token).await;
    let response = post_run(&server, &token, &build(&project)).await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let run: Value = response.json().await.unwrap();
    assert_eq!(run["project"], project.as_str());
    assert_eq!(run["task"], "TEN-301");
    let jobs = server.cluster.jobs();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].metadata.name.as_deref(), run["name"].as_str());
}

#[tokio::test]
async fn a_command_that_is_not_a_tenbox_command_of_the_task_is_a_bad_request() {
    let server = TestServer::start().await;
    let token = server.register("Wegobi", "enojen", Role::Admin).await;
    let project = project_of(&server, &token).await;
    let mut body = build(&project);
    body["command"] = json!(["bash", "-c", "curl evil"]);
    let response = post_run(&server, &token, &body).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(server.cluster.jobs().is_empty());
}

#[tokio::test]
async fn a_second_run_while_the_first_runs_is_machine_busy() {
    let server = TestServer::start().await;
    let token = server.register("Wegobi", "enojen", Role::Admin).await;
    let project = project_of(&server, &token).await;
    let first = post_run(&server, &token, &build(&project)).await;
    assert_eq!(first.status(), StatusCode::CREATED);
    let second = post_run(&server, &token, &build(&project)).await;
    assert_eq!(second.status(), StatusCode::CONFLICT);
    let problem: Value = second.json().await.unwrap();
    assert_eq!(problem["code"], "machine-busy");
}

#[tokio::test]
async fn an_unknown_class_is_a_bad_request() {
    let server = TestServer::start().await;
    let token = server.register("Wegobi", "enojen", Role::Admin).await;
    let project = project_of(&server, &token).await;
    let mut body = build(&project);
    body["class"] = json!("huge");
    let response = post_run(&server, &token, &body).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a_user_of_another_organization_is_forbidden() {
    let server = TestServer::start().await;
    let owner = server.register("Wegobi", "enojen", Role::Admin).await;
    let stranger = server.register("Other", "someone", Role::Admin).await;
    let project = project_of(&server, &owner).await;
    let response = post_run(&server, &stranger, &build(&project)).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(server.cluster.jobs().is_empty());
}
