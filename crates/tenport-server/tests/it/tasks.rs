use reqwest::{Response, StatusCode};
use serde_json::{Value, json};
use tenport_server::accounts::{Role, Token};

use crate::runs::{build, post_run, project_of};
use crate::server::TestServer;

const FIRST_ID: &str = "019a8f2e-7c4b-7d21-9a3e-5b8c1d2e3f40";
const SECOND_ID: &str = "019a8f2e-7c4b-7d21-9a3e-5b8c1d2e3f41";
const THIRD_ID: &str = "019a8f2e-7c4b-7d21-9a3e-5b8c1d2e3f42";

fn archive_key(project: &str) -> String {
    format!("tasks/{project}/TEN-301.tar.zst")
}

async fn delete_task(server: &TestServer, token: &Token, project: &str) -> Response {
    reqwest::Client::new()
        .delete(server.url(&format!("/tasks/TEN-301?project={project}")))
        .bearer_auth(token.expose())
        .send()
        .await
        .unwrap()
}

async fn post_events(server: &TestServer, token: &Token, body: Value) -> Response {
    reqwest::Client::new()
        .post(server.url("/events"))
        .bearer_auth(token.expose())
        .json(&body)
        .send()
        .await
        .unwrap()
}

fn task_event(id: &str, kind: &str, project: &str) -> Value {
    json!({
        "id": id,
        "at": "2026-11-18T10:00:00Z",
        "type": kind,
        "project": project,
        "task": "TEN-301"
    })
}

async fn event_types(server: &TestServer) -> Vec<String> {
    sqlx::query_scalar("select type from events order by rowid")
        .fetch_all(&server.db)
        .await
        .unwrap()
}

#[tokio::test]
async fn a_member_deletes_a_task_and_its_archive_and_a_second_delete_finds_it_missing() {
    let server = TestServer::start().await;
    let token = server.add_user("Wegobi", "enojen", Role::Member).await;
    let admin = server.add_user("Wegobi", "veli", Role::Admin).await;
    let project = project_of(&server, &admin).await;
    server.bucket.put(&archive_key(&project));

    let first = delete_task(&server, &token, &project).await;
    let second = delete_task(&server, &token, &project).await;

    assert_eq!(first.status(), StatusCode::OK);
    assert_eq!(
        first.json::<Value>().await.unwrap(),
        json!({ "project": project, "task": "TEN-301", "archive": "deleted" })
    );
    assert!(!server.bucket.has(&archive_key(&project)));
    assert_eq!(second.status(), StatusCode::OK);
    assert_eq!(second.json::<Value>().await.unwrap()["archive"], "missing");
    let deleted = event_types(&server)
        .await
        .into_iter()
        .filter(|kind| kind == "task.deleted")
        .count();
    assert_eq!(deleted, 2);
}

#[tokio::test]
async fn a_task_with_a_running_job_is_machine_busy_and_keeps_its_archive() {
    let server = TestServer::start().await;
    let token = server.add_user("Wegobi", "enojen", Role::Admin).await;
    let project = project_of(&server, &token).await;
    server.bucket.put(&archive_key(&project));
    let run = post_run(&server, &token, &build(&project)).await;
    assert_eq!(run.status(), StatusCode::CREATED);

    let response = delete_task(&server, &token, &project).await;

    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(
        response.json::<Value>().await.unwrap()["code"],
        "machine-busy"
    );
    assert!(server.bucket.has(&archive_key(&project)));
    assert!(
        !event_types(&server)
            .await
            .contains(&"task.deleted".to_owned())
    );
}

#[tokio::test]
async fn a_user_of_another_organization_is_forbidden_and_the_archive_stays() {
    let server = TestServer::start().await;
    let token = server.add_user("Wegobi", "enojen", Role::Admin).await;
    let stranger = server.add_user("Other", "veli", Role::Admin).await;
    let project = project_of(&server, &token).await;
    server.bucket.put(&archive_key(&project));

    let response = delete_task(&server, &stranger, &project).await;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(server.bucket.has(&archive_key(&project)));
}

#[tokio::test]
async fn a_deleted_task_can_start_again() {
    let server = TestServer::start().await;
    let token = server.add_user("Wegobi", "enojen", Role::Admin).await;
    let project = project_of(&server, &token).await;
    let started = post_events(
        &server,
        &token,
        json!([task_event(FIRST_ID, "task.started", &project)]),
    )
    .await;
    assert_eq!(started.status(), StatusCode::OK);

    delete_task(&server, &token, &project).await;
    let again = post_events(
        &server,
        &token,
        json!([task_event(SECOND_ID, "task.started", &project)]),
    )
    .await;

    assert_eq!(again.status(), StatusCode::OK);
    assert_eq!(again.json::<Value>().await.unwrap()["stored"], 1);
}

#[tokio::test]
async fn a_client_cannot_send_task_deleted_as_an_event() {
    let server = TestServer::start().await;
    let token = server.add_user("Wegobi", "enojen", Role::Admin).await;
    let project = project_of(&server, &token).await;

    let response = post_events(
        &server,
        &token,
        json!([task_event(THIRD_ID, "task.deleted", &project)]),
    )
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(
        !event_types(&server)
            .await
            .contains(&"task.deleted".to_owned())
    );
}
