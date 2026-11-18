use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;

use tempfile::TempDir;
use tenport_server::accounts::{self, GithubLogin, NewUser, Registration, Role, Token};
use tenport_server::cluster::FakeCluster;
use tenport_server::db::{self, Db};
use tenport_server::events::Actor;
use tenport_server::github::GithubApp;
use tenport_server::http::{AppState, Limiter, Limits, router};
use tenport_server::name::Name;
use tenport_server::run_credentials::{InjectorToken, RunCredentialKey, RunCredentials};
use tenport_server::runs::{RunTemplate, Runs};
use tenport_server::task_archives::TaskArchiveBucket;
use tokio::net::TcpListener;

use crate::bucket::FakeBucket;
use crate::github::FakeGithub;

pub struct TestServer {
    pub address: SocketAddr,
    pub db: Db,
    pub cluster: Arc<FakeCluster>,
    pub bucket: FakeBucket,
    pub github: FakeGithub,
    _dir: TempDir,
}

impl TestServer {
    pub async fn start() -> Self {
        Self::start_on(FakeCluster::default()).await
    }

    pub async fn start_on(cluster: FakeCluster) -> Self {
        Self::start_with(cluster, Limits::default()).await
    }

    pub async fn start_with(cluster: FakeCluster, limits: Limits) -> Self {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let dir = tempfile::tempdir().unwrap();
        let db = db::open(&dir.path().join("tenport.db")).await.unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let cluster = Arc::new(cluster);
        let bucket = FakeBucket::start().await;
        let github = FakeGithub::start().await;
        let task_archives = Arc::new(test_bucket(&bucket.url));
        let run_credentials = Arc::new(run_credentials_on(cluster.clone(), &github.url));
        let runs = runs_on(
            cluster.clone(),
            task_archives.clone(),
            run_credentials.clone(),
        );
        let app = router(AppState {
            db: db.clone(),
            cluster: cluster.clone(),
            runs: Arc::new(runs),
            run_credentials,
            task_archives,
            injector_token: Arc::new(InjectorToken::new(INJECTOR_TOKEN)),
            limiter: Arc::new(Limiter::new(limits)),
        });
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Self {
            address,
            db,
            cluster,
            bucket,
            github,
            _dir: dir,
        }
    }

    pub fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.address)
    }
}

impl TestServer {
    pub async fn register(&self, organization: &str, github: &str, role: Role) -> Token {
        let registration = Registration {
            organization: Name::parse(organization).unwrap(),
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

fn runs_on(
    cluster: Arc<FakeCluster>,
    task_archives: Arc<TaskArchiveBucket>,
    run_credentials: Arc<RunCredentials>,
) -> Runs {
    let tests = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let template = RunTemplate {
        dir: tests.join("run-template"),
        services_dir: tests.join("run-services"),
    };
    Runs::new(
        cluster,
        template,
        "box:1".to_owned(),
        task_archives,
        run_credentials,
    )
}

fn run_credentials_on(cluster: Arc<FakeCluster>, github_url: &str) -> RunCredentials {
    RunCredentials::new(
        cluster,
        RunCredentialKey::new(RUN_CREDENTIAL_KEY),
        GithubApp::new(GITHUB_APP_ID, GITHUB_APP_KEY, github_url).unwrap(),
    )
}

pub const RUN_CREDENTIAL_KEY: &str = "key-for-tests";
pub const INJECTOR_TOKEN: &str = "injector-token-for-tests";
pub const GITHUB_APP_ID: &str = "5669876";
const GITHUB_APP_KEY: &str = include_str!("../github-app-test-key.pem");

fn test_bucket(url: &str) -> TaskArchiveBucket {
    TaskArchiveBucket::new(url, "hel1", "key-id".to_owned(), "key-secret".to_owned()).unwrap()
}
