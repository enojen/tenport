use reqwest::StatusCode;
use serde_json::Value;
use tenport_server::accounts::Role;

use crate::server::TestServer;

#[tokio::test]
async fn me_names_the_user_of_the_token_and_their_organizations() {
    let server = TestServer::start().await;
    let token = server.register("Wegobi", "enojen", Role::Admin).await;
    let response = reqwest::Client::new()
        .get(server.url("/me"))
        .bearer_auth(token.expose())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let me: Value = response.json().await.unwrap();
    assert_eq!(me["github"], "enojen");
    assert_eq!(me["organizations"][0]["name"], "Wegobi");
    assert_eq!(me["organizations"][0]["role"], "admin");
}

#[tokio::test]
async fn me_without_a_token_is_unauthorized() {
    let server = TestServer::start().await;
    let response = reqwest::get(server.url("/me")).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let problem: Value = response.json().await.unwrap();
    assert_eq!(problem["code"], "unauthorized");
}

#[tokio::test]
async fn me_with_an_unknown_token_is_unauthorized() {
    let server = TestServer::start().await;
    server.register("Wegobi", "enojen", Role::Admin).await;
    let response = reqwest::Client::new()
        .get(server.url("/me"))
        .bearer_auth("tbx_unknown")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
