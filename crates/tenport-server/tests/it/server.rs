use std::net::SocketAddr;

use tempfile::TempDir;
use tenport_server::accounts::{self, GithubLogin, NewUser, Registration, Role, Token};
use tenport_server::db::{self, Db};
use tenport_server::events::Actor;
use tenport_server::http::{AppState, router};
use tenport_server::name::Name;
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

impl TestServer {
    pub async fn register(&self, org: &str, github: &str, role: Role) -> Token {
        let registration = Registration {
            org: Name::parse(org).unwrap(),
            user: NewUser {
                name: Name::parse(github).unwrap(),
                email: None,
                github: GithubLogin::parse(github).unwrap(),
            },
            role,
            token_name: Name::parse("test").unwrap(),
        };
        accounts::register(&self.db, Actor::Server, registration)
            .await
            .unwrap()
            .token
    }
}
