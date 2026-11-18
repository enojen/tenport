use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::extract::State;
use axum::http::{Method, StatusCode, Uri};
use tokio::net::TcpListener;

pub const BUCKET_NAME: &str = "tenbox-test";

#[derive(Clone)]
pub struct FakeBucket {
    pub url: String,
    objects: Arc<Mutex<BTreeSet<String>>>,
}

impl FakeBucket {
    pub async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/{BUCKET_NAME}", listener.local_addr().unwrap());
        let objects = Arc::new(Mutex::new(BTreeSet::new()));
        let app = Router::new().fallback(answer).with_state(objects.clone());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Self { url, objects }
    }

    pub fn put(&self, key: &str) {
        self.objects.lock().unwrap().insert(key.to_owned());
    }

    pub fn has(&self, key: &str) -> bool {
        self.objects.lock().unwrap().contains(key)
    }
}

async fn answer(
    State(objects): State<Arc<Mutex<BTreeSet<String>>>>,
    method: Method,
    uri: Uri,
) -> StatusCode {
    let prefix = format!("/{BUCKET_NAME}/");
    let Some(key) = uri.path().strip_prefix(&prefix) else {
        return StatusCode::NOT_FOUND;
    };
    let mut objects = objects.lock().unwrap();
    match method {
        Method::HEAD if objects.contains(key) => StatusCode::OK,
        Method::HEAD => StatusCode::NOT_FOUND,
        Method::DELETE => {
            objects.remove(key);
            StatusCode::NO_CONTENT
        }
        _ => StatusCode::METHOD_NOT_ALLOWED,
    }
}
