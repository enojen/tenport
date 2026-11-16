use reqwest::StatusCode;
use serde_json::{Value, json};
use tenport_server::accounts::{Role, Token};

use crate::server::TestServer;

async fn org_of(server: &TestServer, token: &Token) -> String {
    let me: Value = reqwest::Client::new()
        .get(server.url("/me"))
        .bearer_auth(token.expose())
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    me["organizations"][0]["id"].as_str().unwrap().to_owned()
}

async fn post_project(server: &TestServer, token: &Token, body: Value) -> reqwest::Response {
    reqwest::Client::new()
        .post(server.url("/projects"))
        .bearer_auth(token.expose())
        .json(&body)
        .send()
        .await
        .unwrap()
}

async fn get_projects(server: &TestServer, token: &Token) -> Value {
    reqwest::Client::new()
        .get(server.url("/projects"))
        .bearer_auth(token.expose())
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap()
}

#[tokio::test]
async fn an_admin_adds_a_project_and_lists_it() {
    let server = TestServer::start().await;
    let token = server.register("Wegobi", "enojen", Role::Admin).await;
    let org = org_of(&server, &token).await;
    let response = post_project(
        &server,
        &token,
        json!({ "org": org, "name": "Tenport", "repo": "enojen/tenport" }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let project: Value = response.json().await.unwrap();
    assert_eq!(project["org"], org.as_str());
    assert_eq!(project["repo"], "enojen/tenport");
    assert_eq!(project["id"].as_str().unwrap().len(), 6);
    assert_eq!(get_projects(&server, &token).await, json!([project]));
}

#[tokio::test]
async fn a_member_is_forbidden_to_add_a_project() {
    let server = TestServer::start().await;
    let admin = server.register("Wegobi", "enojen", Role::Admin).await;
    let member = server.register("Wegobi", "ali", Role::Member).await;
    let org = org_of(&server, &member).await;
    let response = post_project(
        &server,
        &member,
        json!({ "org": org, "name": "Tenport", "repo": "enojen/tenport" }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let problem: Value = response.json().await.unwrap();
    assert_eq!(problem["code"], "forbidden");
    assert_eq!(get_projects(&server, &admin).await, json!([]));
}

#[tokio::test]
async fn a_bad_repo_is_a_bad_request() {
    let server = TestServer::start().await;
    let token = server.register("Wegobi", "enojen", Role::Admin).await;
    let org = org_of(&server, &token).await;
    let response = post_project(
        &server,
        &token,
        json!({ "org": org, "name": "Tenport", "repo": "tenport" }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let problem: Value = response.json().await.unwrap();
    assert_eq!(problem["code"], "bad-request");
    let detail = problem["detail"].as_str().unwrap();
    assert!(detail.contains("owner/name"), "{detail}");
}

#[tokio::test]
async fn projects_without_a_token_are_unauthorized() {
    let server = TestServer::start().await;
    let response = reqwest::get(server.url("/projects")).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
