use std::net::SocketAddr;

use tempfile::TempDir;
use tenport_server::db::{self, Db};
use tenport_server::http::{AppState, router};
use tokio::net::TcpListener;

pub struct TestServer {
    pub address: SocketAddr,
    pub db: Db,
    _dir: TempDir,
}

impl TestServer {
    pub async fn start() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let db = db::open(&dir.path().join("tenport.db")).await.unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let app = router(AppState { db: db.clone() });
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Self {
            address,
            db,
            _dir: dir,
        }
    }

    pub fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.address)
    }
}
