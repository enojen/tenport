use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use axum::Json;
use axum::Router;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use serde_json::{Value, json};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use tokio::net::TcpListener;

pub const INSTALLATION: u64 = 42;

#[derive(Default)]
struct Seen {
    installed: BTreeSet<String>,
    mints: Vec<Value>,
    authorizations: Vec<String>,
}

#[derive(Clone)]
pub struct FakeGithub {
    pub url: String,
    seen: Arc<Mutex<Seen>>,
}

impl FakeGithub {
    pub async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let seen = Arc::new(Mutex::new(Seen::default()));
        let app = Router::new()
            .route("/repos/{owner}/{repo}/installation", get(installation))
            .route("/app/installations/{id}/access_tokens", post(access_token))
            .with_state(seen.clone());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Self { url, seen }
    }

    pub fn install(&self, repo: &str) {
        self.seen.lock().unwrap().installed.insert(repo.to_owned());
    }

    pub fn mints(&self) -> Vec<Value> {
        self.seen.lock().unwrap().mints.clone()
    }

    pub fn authorizations(&self) -> Vec<String> {
        self.seen.lock().unwrap().authorizations.clone()
    }
}

fn note_authorization(seen: &Mutex<Seen>, headers: &HeaderMap) {
    let authorization = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    seen.lock().unwrap().authorizations.push(authorization);
}

async fn installation(
    State(seen): State<Arc<Mutex<Seen>>>,
    Path((owner, repo)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Json<Value>, StatusCode> {
    note_authorization(&seen, &headers);
    if seen
        .lock()
        .unwrap()
        .installed
        .contains(&format!("{owner}/{repo}"))
    {
        Ok(Json(json!({ "id": INSTALLATION })))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

async fn access_token(
    State(seen): State<Arc<Mutex<Seen>>>,
    Path(id): Path<u64>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    note_authorization(&seen, &headers);
    if id != INSTALLATION {
        return Err(StatusCode::NOT_FOUND);
    }
    let mut seen = seen.lock().unwrap();
    seen.mints.push(body);
    let expires = (OffsetDateTime::now_utc() + time::Duration::hours(1))
        .format(&Rfc3339)
        .unwrap();
    Ok(Json(json!({
        "token": format!("ghs_test_{}", seen.mints.len()),
        "expires_at": expires,
    })))
}
