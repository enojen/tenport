use reqwest::StatusCode;
use reqwest::header::CONTENT_TYPE;
use serde_json::{Value, json};

use crate::server::TestServer;

#[tokio::test]
async fn health_is_ok_while_the_database_answers() {
    let server = TestServer::start().await;
    let response = reqwest::get(server.url("/health")).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.json::<Value>().await.unwrap(),
        json!({ "ok": true })
    );
}

#[tokio::test]
async fn health_is_unavailable_when_the_database_is_closed() {
    let server = TestServer::start().await;
    server.db.close().await;
    let response = reqwest::get(server.url("/health")).await.unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let problem: Value = response.json().await.unwrap();
    assert_eq!(problem["code"], "database-down");
}

#[tokio::test]
async fn an_unknown_path_is_a_problem() {
    let server = TestServer::start().await;
    let response = reqwest::get(server.url("/nothing")).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(response.headers()[CONTENT_TYPE], "application/problem+json");
    assert_eq!(
        response.json::<Value>().await.unwrap(),
        json!({
            "type": "about:blank",
            "title": "Not Found",
            "status": 404,
            "detail": "no route for GET /nothing",
            "code": "not-found"
        })
    );
}
