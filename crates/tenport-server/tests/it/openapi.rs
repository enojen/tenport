use reqwest::StatusCode;
use serde_json::Value;

use crate::server::TestServer;

async fn openapi() -> Value {
    let server = TestServer::start().await;
    let response = reqwest::get(server.url("/openapi.json")).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    response.json().await.unwrap()
}

#[tokio::test]
async fn names_the_api_and_its_version() {
    let api = openapi().await;
    assert_eq!(api["openapi"], "3.1.0");
    assert_eq!(api["info"]["title"], "Tenport");
    assert_eq!(api["info"]["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(api["info"]["license"]["name"], "FSL-1.1-ALv2");
}

#[tokio::test]
async fn lists_health_with_both_answers() {
    let api = openapi().await;
    let responses = &api["paths"]["/health"]["get"]["responses"];
    assert!(responses["200"]["content"]["application/json"].is_object());
    assert!(responses["503"]["content"]["application/problem+json"].is_object());
    assert!(api["components"]["schemas"]["Problem"].is_object());
}
