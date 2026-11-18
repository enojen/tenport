use reqwest::StatusCode;
use serde_json::{Value, json};
use tenport_server::accounts::{Role, Token};

use crate::server::TestServer;

async fn add_project(server: &TestServer, token: &Token) -> String {
    let client = reqwest::Client::new();
    let me: Value = client
        .get(server.url("/me"))
        .bearer_auth(token.expose())
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let project: Value = client
        .post(server.url("/projects"))
        .bearer_auth(token.expose())
        .json(&json!({
            "organization": me["organizations"][0]["id"],
            "name": "Tenport",
            "repo": "enojen/tenport"
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    project["id"].as_str().unwrap().to_owned()
}

async fn post_events(server: &TestServer, token: &Token, body: Value) -> reqwest::Response {
    reqwest::Client::new()
        .post(server.url("/events"))
        .bearer_auth(token.expose())
        .json(&body)
        .send()
        .await
        .unwrap()
}

fn task_started(id: &str, project: &str) -> Value {
    json!({
        "id": id,
        "at": "2026-11-16T10:00:00Z",
        "type": "task.started",
        "project": project,
        "task": "TEN-1",
        "title": "Add projects",
        "url": "https://github.com/enojen/tenport/issues/7"
    })
}

const FIRST_ID: &str = "019a8f2e-7c4b-7d21-9a3e-5b8c1d2e3f40";

#[tokio::test]
async fn events_are_stored_once() {
    let server = TestServer::start().await;
    let token = server.register("Wegobi", "enojen", Role::Admin).await;
    let project = add_project(&server, &token).await;
    let body = json!([task_started(FIRST_ID, &project)]);
    let first = post_events(&server, &token, body.clone()).await;
    assert_eq!(first.status(), StatusCode::OK);
    assert_eq!(
        first.json::<Value>().await.unwrap(),
        json!({ "stored": 1, "skipped": 0 })
    );
    let again: Value = post_events(&server, &token, body)
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(again, json!({ "stored": 0, "skipped": 1 }));
}

#[tokio::test]
async fn the_same_id_with_other_content_is_a_conflict() {
    let server = TestServer::start().await;
    let token = server.register("Wegobi", "enojen", Role::Admin).await;
    let project = add_project(&server, &token).await;
    post_events(&server, &token, json!([task_started(FIRST_ID, &project)])).await;
    let mut changed = task_started(FIRST_ID, &project);
    changed["title"] = json!("Another title");
    let response = post_events(&server, &token, json!([changed])).await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let problem: Value = response.json().await.unwrap();
    assert_eq!(problem["code"], "conflict");
}

#[tokio::test]
async fn a_stranger_is_forbidden() {
    let server = TestServer::start().await;
    let owner = server.register("Wegobi", "enojen", Role::Admin).await;
    let stranger = server.register("Other", "ali", Role::Admin).await;
    let project = add_project(&server, &owner).await;
    let response = post_events(
        &server,
        &stranger,
        json!([task_started(FIRST_ID, &project)]),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn an_unknown_type_is_a_bad_request() {
    let server = TestServer::start().await;
    let token = server.register("Wegobi", "enojen", Role::Admin).await;
    let project = add_project(&server, &token).await;
    let mut event = task_started(FIRST_ID, &project);
    event["type"] = json!("task.renamed");
    let response = post_events(&server, &token, json!([event])).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
