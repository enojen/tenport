use reqwest::{Response, StatusCode};
use serde_json::{Value, json};
use tenport_server::accounts::{Role, Token};
use tenport_server::cluster::SecretName;

use crate::server::TestServer;

const TRACKER_KEY: &str = "lin_api_not_a_real_key";

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

async fn put_secret(
    server: &TestServer,
    token: &Token,
    project: &str,
    name: &str,
    body: Value,
) -> Response {
    reqwest::Client::new()
        .put(server.url(&format!("/projects/{project}/secrets/{name}")))
        .bearer_auth(token.expose())
        .json(&body)
        .send()
        .await
        .unwrap()
}

async fn delete_secret(server: &TestServer, token: &Token, project: &str, name: &str) -> Response {
    reqwest::Client::new()
        .delete(server.url(&format!("/projects/{project}/secrets/{name}")))
        .bearer_auth(token.expose())
        .send()
        .await
        .unwrap()
}

async fn list_secrets(server: &TestServer, token: &Token, project: &str) -> Response {
    reqwest::Client::new()
        .get(server.url(&format!("/projects/{project}/secrets")))
        .bearer_auth(token.expose())
        .send()
        .await
        .unwrap()
}

fn project_secret(server: &TestServer, project: &str) -> Option<Vec<(String, String)>> {
    let name = SecretName::new(format!("project-{project}"));
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

#[tokio::test]
async fn an_admin_writes_a_secret_and_tenport_gives_back_only_its_name() {
    let server = TestServer::start().await;
    let admin = server.register("Wegobi", "enojen", Role::Admin).await;
    let project = add_project(&server, &admin).await;

    let response = put_secret(
        &server,
        &admin,
        &project,
        "tracker_api_key",
        json!({ "value": TRACKER_KEY }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let written: Value = response.json().await.unwrap();
    assert_eq!(written["name"], "tracker_api_key");
    assert!(written["changed"].is_string(), "{written}");
    assert_eq!(
        project_secret(&server, &project),
        Some(vec![("tracker_api_key".to_owned(), TRACKER_KEY.to_owned())])
    );
    let listed = list_secrets(&server, &admin, &project).await;
    assert_eq!(listed.status(), StatusCode::OK);
    let listed = listed.text().await.unwrap();
    assert!(!listed.contains(TRACKER_KEY), "{listed}");
    assert_eq!(
        serde_json::from_str::<Value>(&listed).unwrap(),
        json!([written])
    );
    let events = event_bodies(&server).await;
    assert!(events.contains("tracker_api_key"), "{events}");
    assert!(!events.contains(TRACKER_KEY), "{events}");
}

#[tokio::test]
async fn a_member_reads_the_names_but_is_forbidden_to_write() {
    let server = TestServer::start().await;
    let admin = server.register("Wegobi", "enojen", Role::Admin).await;
    let member = server.register("Wegobi", "ali", Role::Member).await;
    let project = add_project(&server, &admin).await;

    let response = put_secret(
        &server,
        &member,
        &project,
        "ntfy_url",
        json!({ "value": "https://ntfy.sh/x" }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(project_secret(&server, &project), None);
    let deleted = delete_secret(&server, &member, &project, "ntfy_url").await;
    assert_eq!(deleted.status(), StatusCode::FORBIDDEN);
    let listed = list_secrets(&server, &member, &project).await;
    assert_eq!(listed.status(), StatusCode::OK);
    assert_eq!(listed.json::<Value>().await.unwrap(), json!([]));
}

#[tokio::test]
async fn an_admin_of_another_organization_is_forbidden_to_list_or_write() {
    let server = TestServer::start().await;
    let admin = server.register("Wegobi", "enojen", Role::Admin).await;
    let stranger = server.register("Other", "veli", Role::Admin).await;
    let project = add_project(&server, &admin).await;

    let listed = list_secrets(&server, &stranger, &project).await;
    let written = put_secret(
        &server,
        &stranger,
        &project,
        "ntfy_url",
        json!({ "value": "https://ntfy.sh/x" }),
    )
    .await;
    let deleted = delete_secret(&server, &stranger, &project, "ntfy_url").await;

    assert_eq!(listed.status(), StatusCode::FORBIDDEN);
    assert_eq!(written.status(), StatusCode::FORBIDDEN);
    assert_eq!(deleted.status(), StatusCode::FORBIDDEN);
    assert_eq!(project_secret(&server, &project), None);
}

#[tokio::test]
async fn an_unknown_name_or_an_empty_value_is_a_bad_request() {
    let server = TestServer::start().await;
    let admin = server.register("Wegobi", "enojen", Role::Admin).await;
    let project = add_project(&server, &admin).await;

    let unknown = put_secret(
        &server,
        &admin,
        &project,
        "github_token",
        json!({ "value": "x" }),
    )
    .await;
    let empty = put_secret(
        &server,
        &admin,
        &project,
        "tracker_api_key",
        json!({ "value": "" }),
    )
    .await;

    assert_eq!(unknown.status(), StatusCode::BAD_REQUEST);
    assert_eq!(empty.status(), StatusCode::BAD_REQUEST);
    assert_eq!(project_secret(&server, &project), None);
}

#[tokio::test]
async fn deleting_a_secret_removes_its_key_and_a_second_delete_is_not_found() {
    let server = TestServer::start().await;
    let admin = server.register("Wegobi", "enojen", Role::Admin).await;
    let project = add_project(&server, &admin).await;
    put_secret(
        &server,
        &admin,
        &project,
        "tracker_api_key",
        json!({ "value": TRACKER_KEY }),
    )
    .await;
    put_secret(
        &server,
        &admin,
        &project,
        "ntfy_url",
        json!({ "value": "https://ntfy.sh/x" }),
    )
    .await;

    let first = delete_secret(&server, &admin, &project, "tracker_api_key").await;
    let second = delete_secret(&server, &admin, &project, "tracker_api_key").await;

    assert_eq!(first.status(), StatusCode::NO_CONTENT);
    assert_eq!(second.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        project_secret(&server, &project),
        Some(vec![(
            "ntfy_url".to_owned(),
            "https://ntfy.sh/x".to_owned()
        )])
    );
    let listed: Value = list_secrets(&server, &admin, &project)
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(listed.as_array().unwrap().len(), 1);
    assert_eq!(listed[0]["name"], "ntfy_url");
}
