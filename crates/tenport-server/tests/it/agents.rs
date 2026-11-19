use k8s_openapi::api::batch::v1::Job;
use reqwest::{Response, StatusCode};
use serde_json::{Value, json};
use tenport_server::accounts::{Role, Token};
use tenport_server::cluster::SecretName;

use crate::runs::{build, post_run, project_of};
use crate::server::TestServer;

const LOGIN: &str = r#"{"claudeAiOauth":{"accessToken":"not-a-real-token","refreshToken":"not-a-real-refresh","expiresAt":1}}"#;

async fn add_agent(server: &TestServer, token: &Token, body: Value) -> Response {
    reqwest::Client::new()
        .post(server.url("/agents"))
        .bearer_auth(token.expose())
        .json(&body)
        .send()
        .await
        .unwrap()
}

async fn added_agent(server: &TestServer, token: &Token, name: &str) -> Value {
    let response = add_agent(
        server,
        token,
        json!({ "kind": "claude-code", "name": name }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    response.json().await.unwrap()
}

async fn list_agents(server: &TestServer, token: &Token) -> Value {
    let response = reqwest::Client::new()
        .get(server.url("/agents"))
        .bearer_auth(token.expose())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    response.json().await.unwrap()
}

async fn put_login(server: &TestServer, token: &Token, agent: &str, body: Value) -> Response {
    reqwest::Client::new()
        .put(server.url(&format!("/agents/{agent}/login")))
        .bearer_auth(token.expose())
        .json(&body)
        .send()
        .await
        .unwrap()
}

fn agent_secret(server: &TestServer, agent: &str) -> Option<Vec<(String, String)>> {
    let name = SecretName::new(format!("agent-{agent}"));
    server
        .cluster
        .secret(&name)
        .map(|data| data.into_iter().collect())
}

async fn event_bodies(server: &TestServer) -> String {
    let bodies: Vec<String> = sqlx::query_scalar("select body from events")
        .fetch_all(&server.db)
        .await
        .unwrap();
    bodies.join("\n")
}

fn credentials_secret_of(job: &Job) -> String {
    let spec = job.spec.as_ref().unwrap().template.spec.as_ref().unwrap();
    let container = spec.containers.iter().find(|c| c.name == "box").unwrap();
    let env = container.env.as_ref().unwrap();
    let var = env
        .iter()
        .find(|var| var.name == "CLAUDE_CREDENTIALS")
        .unwrap();
    let key = var
        .value_from
        .as_ref()
        .unwrap()
        .secret_key_ref
        .as_ref()
        .unwrap();
    key.name.clone()
}

#[tokio::test]
async fn a_user_adds_agents_and_the_first_is_the_default() {
    let server = TestServer::start().await;
    let token = server.add_user("Wegobi", "enojen", Role::Admin).await;

    let personal = added_agent(&server, &token, "personal").await;
    let work = added_agent(&server, &token, "work").await;

    assert!(
        personal["id"].as_str().unwrap().starts_with("a-"),
        "{personal}"
    );
    assert_eq!(personal["kind"], "claude-code");
    assert_eq!(personal["name"], "personal");
    assert_eq!(personal["default"], true);
    assert_eq!(personal["login"], Value::Null);
    assert_eq!(work["default"], false);
    assert_eq!(list_agents(&server, &token).await, json!([personal, work]));
}

#[tokio::test]
async fn a_second_agent_with_the_same_name_is_a_conflict() {
    let server = TestServer::start().await;
    let token = server.add_user("Wegobi", "enojen", Role::Admin).await;
    added_agent(&server, &token, "personal").await;

    let response = add_agent(
        &server,
        &token,
        json!({ "kind": "claude-code", "name": "personal" }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn an_agent_of_a_kind_tenport_does_not_know_is_refused() {
    let server = TestServer::start().await;
    let token = server.add_user("Wegobi", "enojen", Role::Admin).await;

    let response = add_agent(
        &server,
        &token,
        json!({ "kind": "codex", "name": "personal" }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a_user_sees_only_their_own_agents() {
    let server = TestServer::start().await;
    let enes = server.add_user("Wegobi", "enojen", Role::Admin).await;
    let ayse = server.add_user("Wegobi", "ayse", Role::Member).await;
    added_agent(&server, &enes, "personal").await;

    assert_eq!(list_agents(&server, &ayse).await, json!([]));
}

#[tokio::test]
async fn the_login_goes_to_the_secret_of_the_agent_and_never_back() {
    let server = TestServer::start().await;
    let token = server.add_user("Wegobi", "enojen", Role::Admin).await;
    let agent = added_agent(&server, &token, "personal").await;
    let id = agent["id"].as_str().unwrap();

    let response = put_login(&server, &token, id, json!({ "value": LOGIN })).await;

    assert_eq!(response.status(), StatusCode::OK);
    let written: Value = response.json().await.unwrap();
    assert_eq!(written["agent"], id);
    assert!(written["changed"].is_string(), "{written}");
    assert_eq!(
        agent_secret(&server, id),
        Some(vec![("credentials.json".to_owned(), LOGIN.to_owned())])
    );
    let listed = list_agents(&server, &token).await;
    assert_eq!(listed[0]["login"], json!({ "changed": written["changed"] }));
    assert!(!listed.to_string().contains("not-a-real-token"));
    assert!(!event_bodies(&server).await.contains("not-a-real-token"));
}

#[tokio::test]
async fn a_new_login_replaces_the_old_one() {
    let server = TestServer::start().await;
    let token = server.add_user("Wegobi", "enojen", Role::Admin).await;
    let agent = added_agent(&server, &token, "personal").await;
    let id = agent["id"].as_str().unwrap();
    put_login(&server, &token, id, json!({ "value": LOGIN })).await;
    let newer = LOGIN.replace("not-a-real-token", "a-newer-token");

    let response = put_login(&server, &token, id, json!({ "value": newer })).await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        agent_secret(&server, id),
        Some(vec![("credentials.json".to_owned(), newer)])
    );
}

#[tokio::test]
async fn nobody_writes_the_login_of_another_users_agent() {
    let server = TestServer::start().await;
    let enes = server.add_user("Wegobi", "enojen", Role::Admin).await;
    let ayse = server.add_user("Wegobi", "ayse", Role::Admin).await;
    let agent = added_agent(&server, &enes, "personal").await;
    let id = agent["id"].as_str().unwrap();

    let response = put_login(&server, &ayse, id, json!({ "value": LOGIN })).await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(agent_secret(&server, id), None);
}

#[tokio::test]
async fn a_login_that_is_not_json_is_refused() {
    let server = TestServer::start().await;
    let token = server.add_user("Wegobi", "enojen", Role::Admin).await;
    let agent = added_agent(&server, &token, "personal").await;
    let id = agent["id"].as_str().unwrap();

    for value in ["", "not json", "[1]"] {
        let response = put_login(&server, &token, id, json!({ "value": value })).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{value}");
    }
    assert_eq!(agent_secret(&server, id), None);
}

#[tokio::test]
async fn a_run_reads_the_login_of_the_default_agent() {
    let server = TestServer::start().await;
    let token = server.add_user("Wegobi", "enojen", Role::Admin).await;
    let personal = added_agent(&server, &token, "personal").await;
    added_agent(&server, &token, "work").await;
    let project = project_of(&server, &token).await;

    let response = post_run(&server, &token, &build(&project)).await;

    assert_eq!(response.status(), StatusCode::CREATED);
    let id = personal["id"].as_str().unwrap();
    let jobs = server.cluster.jobs();
    assert_eq!(credentials_secret_of(&jobs[0]), format!("agent-{id}"));
    let run: Value = response.json().await.unwrap();
    let state: Value = reqwest::Client::new()
        .get(server.url(&format!("/runs/{}", run["name"].as_str().unwrap())))
        .bearer_auth(token.expose())
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(state["agent"], id);
}

#[tokio::test]
async fn a_run_reads_the_login_of_the_agent_it_names() {
    let server = TestServer::start().await;
    let token = server.add_user("Wegobi", "enojen", Role::Admin).await;
    added_agent(&server, &token, "personal").await;
    let work = added_agent(&server, &token, "work").await;
    let project = project_of(&server, &token).await;
    let mut body = build(&project);
    body["agent"] = work["id"].clone();

    let response = post_run(&server, &token, &body).await;

    assert_eq!(response.status(), StatusCode::CREATED);
    let id = work["id"].as_str().unwrap();
    assert_eq!(
        credentials_secret_of(&server.cluster.jobs()[0]),
        format!("agent-{id}")
    );
}

#[tokio::test]
async fn a_run_never_uses_the_agent_of_another_user() {
    let server = TestServer::start().await;
    let enes = server.add_user("Wegobi", "enojen", Role::Admin).await;
    let ayse = server.add_user("Wegobi", "ayse", Role::Admin).await;
    added_agent(&server, &ayse, "personal").await;
    let theirs = added_agent(&server, &enes, "personal").await;
    let project = project_of(&server, &ayse).await;
    let mut body = build(&project);
    body["agent"] = theirs["id"].clone();

    let response = post_run(&server, &ayse, &body).await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert!(server.cluster.jobs().is_empty());
}

#[tokio::test]
async fn a_user_with_no_agent_cannot_start_a_run() {
    let server = TestServer::start().await;
    let token = server.add_user("Wegobi", "enojen", Role::Admin).await;
    let project = project_of(&server, &token).await;

    let response = post_run(&server, &token, &build(&project)).await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let problem: Value = response.json().await.unwrap();
    assert_eq!(problem["code"], "no-agent");
    assert!(server.cluster.jobs().is_empty());
}
