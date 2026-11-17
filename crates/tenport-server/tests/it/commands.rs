use std::process::{Command, Output};

use serde_json::Value;
use tempfile::TempDir;
use tenport_server::accounts::{self, Token};
use tenport_server::db;

const ADD_AYSE: [&str; 10] = [
    "user", "add", "--name", "Ayse", "--github", "ayse", "--org", "demo", "--role", "admin",
];

fn tenport(dir: &TempDir, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tenport"))
        .args(args)
        .env("TENPORT_DATABASE", dir.path().join("tenport.db"))
        .output()
        .unwrap()
}

#[tokio::test]
async fn user_add_prints_a_token_that_signs_in() {
    let dir = tempfile::tempdir().unwrap();
    let output = tenport(&dir, &ADD_AYSE);
    assert!(output.status.success(), "{output:?}");
    let added: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(added["role"], "admin");
    let token = Token::parse(added["token"].as_str().unwrap()).unwrap();
    let db = db::open(&dir.path().join("tenport.db")).await.unwrap();
    let user = accounts::authenticate(&db, &token).await.unwrap().unwrap();
    assert_eq!(user.github.as_str(), "ayse");
    assert_eq!(serde_json::to_value(&user.id).unwrap(), added["user"]);
}

#[test]
fn the_same_login_twice_fails() {
    let dir = tempfile::tempdir().unwrap();
    assert!(tenport(&dir, &ADD_AYSE).status.success());
    let again = tenport(&dir, &ADD_AYSE);
    assert_eq!(again.status.code(), Some(1));
    assert!(again.stdout.is_empty());
    assert!(String::from_utf8_lossy(&again.stderr).contains("exists"));
}

#[test]
fn a_wrong_command_shows_the_usage() {
    let dir = tempfile::tempdir().unwrap();
    let output = tenport(&dir, &["user", "add", "--name", "Ayse"]);
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("user add needs --github"), "{stderr}");
    assert!(stderr.contains("usage: tenport"), "{stderr}");
}
