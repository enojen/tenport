use reqwest::{Response, StatusCode};
use serde_json::{Value, json};
use tenport_server::accounts::Role;
use tenport_server::cluster::{FakeCluster, JobName, JobPhase};
use tenport_server::run_credentials::RunCredentialKey;

use crate::runs::{build, post_run, project_of};
use crate::server::{INJECTOR_TOKEN, RUN_CREDENTIAL_KEY, TestServer};

async fn started_job(server: &TestServer) -> (JobName, String) {
    let token = server.add_user("Wegobi", "enojen", Role::Admin).await;
    let project = project_of(server, &token).await;
    let run: Value = post_run(server, &token, &build(&project))
        .await
        .json()
        .await
        .unwrap();
    let job = JobName::new(run["name"].as_str().unwrap());
    let placeholder =
        RunCredentialKey::new(RUN_CREDENTIAL_KEY).placeholder(&FakeCluster::uid_of(&job));
    (job, placeholder)
}

async fn ask(server: &TestServer, token: &str, placeholder: &str, host: &str) -> Response {
    reqwest::Client::new()
        .post(server.url("/run-credentials"))
        .bearer_auth(token)
        .json(&json!({ "placeholder": placeholder, "host": host }))
        .send()
        .await
        .unwrap()
}

#[tokio::test]
async fn the_placeholder_of_an_open_job_gets_a_token_for_its_project_repository_only() {
    let server = TestServer::start().await;
    server.github.install("enojen/tenport");
    let (_, placeholder) = started_job(&server).await;

    let response = ask(&server, INJECTOR_TOKEN, &placeholder, "api.github.com").await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.json::<Value>().await.unwrap(),
        json!({ "value": "ghs_test_1" })
    );
    assert_eq!(
        server.github.mints(),
        vec![json!({
            "repositories": ["tenport"],
            "permissions": { "contents": "write", "pull_requests": "write", "issues": "write" }
        })]
    );
    assert!(
        server
            .github
            .authorizations()
            .iter()
            .all(|header| header.starts_with("Bearer ey") && header.matches('.').count() == 2)
    );
}

#[tokio::test]
async fn a_second_ask_for_the_same_job_gets_the_same_token_without_a_new_one() {
    let server = TestServer::start().await;
    server.github.install("enojen/tenport");
    let (_, placeholder) = started_job(&server).await;

    ask(&server, INJECTOR_TOKEN, &placeholder, "github.com").await;
    let second = ask(&server, INJECTOR_TOKEN, &placeholder, "api.github.com").await;

    assert_eq!(second.json::<Value>().await.unwrap()["value"], "ghs_test_1");
    assert_eq!(server.github.mints().len(), 1);
}

#[tokio::test]
async fn only_the_injector_token_may_ask() {
    let server = TestServer::start().await;
    server.github.install("enojen/tenport");
    let (_, placeholder) = started_job(&server).await;
    let user = server.add_user("Wegobi", "ali", Role::Admin).await;

    let other = ask(&server, "another-token", &placeholder, "api.github.com").await;
    let user = ask(&server, user.expose(), &placeholder, "api.github.com").await;

    assert_eq!(other.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(user.status(), StatusCode::UNAUTHORIZED);
    assert!(server.github.mints().is_empty());
}

#[tokio::test]
async fn an_unknown_placeholder_another_host_or_an_ended_job_is_not_found() {
    let server = TestServer::start().await;
    server.github.install("enojen/tenport");
    let (job, placeholder) = started_job(&server).await;

    let unknown = ask(&server, INJECTOR_TOKEN, "tbxph_unknown", "api.github.com").await;
    let tracker = ask(&server, INJECTOR_TOKEN, &placeholder, "api.linear.app").await;
    server.cluster.set_phase(&job, JobPhase::Succeeded);
    let ended = ask(&server, INJECTOR_TOKEN, &placeholder, "api.github.com").await;

    assert_eq!(unknown.status(), StatusCode::NOT_FOUND);
    assert_eq!(tracker.status(), StatusCode::NOT_FOUND);
    assert_eq!(ended.status(), StatusCode::NOT_FOUND);
    assert!(server.github.mints().is_empty());
}

#[tokio::test]
async fn a_repository_without_the_app_is_not_found() {
    let server = TestServer::start().await;
    let (_, placeholder) = started_job(&server).await;

    let response = ask(&server, INJECTOR_TOKEN, &placeholder, "api.github.com").await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let problem: Value = response.json().await.unwrap();
    assert!(
        problem["detail"]
            .as_str()
            .unwrap()
            .contains("not installed on enojen/tenport"),
        "{problem}"
    );
}
