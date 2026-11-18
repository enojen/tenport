use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use reqwest::header::{ACCEPT, USER_AGENT};
use reqwest::{Client, RequestBuilder, StatusCode};
use ring::rand::SystemRandom;
use ring::signature::{RSA_PKCS1_SHA256, RsaKeyPair};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::json;
use time::OffsetDateTime;

use crate::https::{HttpsError, https_client};
use crate::projects::Repo;

#[cfg(test)]
mod tests;

pub const GITHUB_API: &str = "https://api.github.com";
pub const GITHUB_HOSTS: [&str; 2] = ["github.com", "api.github.com"];
const JWT_BACK_SECONDS: i64 = 60;
const JWT_AHEAD_SECONDS: i64 = 540;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
const API_ACCEPT: &str = "application/vnd.github+json";
const CLIENT_NAME: &str = "tenport";
const PKCS1_HEADER: &str = "-----BEGIN RSA PRIVATE KEY-----";

#[derive(Debug, thiserror::Error)]
pub enum GithubError {
    #[error("cannot read {}: {source}", path.display())]
    Read { path: PathBuf, source: io::Error },
    #[error("the GitHub App key is not an RSA private key in PEM: {0}")]
    Key(String),
    #[error(transparent)]
    Client(#[from] HttpsError),
    #[error("GitHub did not answer {url}: {source}")]
    Request { url: String, source: reqwest::Error },
    #[error("GitHub answered {url} with {status}")]
    Answer { url: String, status: StatusCode },
    #[error("the GitHub App is not installed on {repo}")]
    NotInstalled { repo: String },
}

pub struct GithubApp {
    id: String,
    key: RsaKeyPair,
    api: String,
    client: Client,
}

#[derive(Clone)]
pub struct RepoToken {
    pub token: String,
    pub expires: OffsetDateTime,
}

#[derive(Deserialize)]
struct Installation {
    id: u64,
}

#[derive(Deserialize)]
struct AccessToken {
    token: String,
    #[serde(with = "time::serde::rfc3339")]
    expires_at: OffsetDateTime,
}

impl GithubApp {
    pub fn new(id: &str, key_pem: &str, api: &str) -> Result<Self, GithubError> {
        Ok(Self {
            id: id.to_owned(),
            key: key_pair(key_pem)?,
            api: api.trim_end_matches('/').to_owned(),
            client: https_client(REQUEST_TIMEOUT)?,
        })
    }

    pub fn read(id: &str, key_file: &Path, api: &str) -> Result<Self, GithubError> {
        let pem = std::fs::read_to_string(key_file).map_err(|source| GithubError::Read {
            path: key_file.to_owned(),
            source,
        })?;
        Self::new(id, &pem, api)
    }

    pub async fn repo_token(
        &self,
        repo: &Repo,
        now: OffsetDateTime,
    ) -> Result<RepoToken, GithubError> {
        let jwt = self.jwt(now)?;
        let installation_url = format!("{}/repos/{}/installation", self.api, repo.as_str());
        let installation: Option<Installation> = self
            .send(self.client.get(&installation_url), &jwt, &installation_url)
            .await?;
        let installation = installation.ok_or_else(|| GithubError::NotInstalled {
            repo: repo.as_str().to_owned(),
        })?;
        let tokens_url = format!(
            "{}/app/installations/{}/access_tokens",
            self.api, installation.id
        );
        let body = json!({
            "repositories": [repo.name()],
            "permissions": { "contents": "write", "pull_requests": "write", "issues": "write" },
        });
        let minted: Option<AccessToken> = self
            .send(self.client.post(&tokens_url).json(&body), &jwt, &tokens_url)
            .await?;
        let minted = minted.ok_or_else(|| GithubError::NotInstalled {
            repo: repo.as_str().to_owned(),
        })?;
        Ok(RepoToken {
            token: minted.token,
            expires: minted.expires_at,
        })
    }

    fn jwt(&self, now: OffsetDateTime) -> Result<String, GithubError> {
        let seconds = now.unix_timestamp();
        let header = URL_SAFE_NO_PAD.encode(json!({ "alg": "RS256", "typ": "JWT" }).to_string());
        let claims = json!({
            "iat": seconds - JWT_BACK_SECONDS,
            "exp": seconds + JWT_AHEAD_SECONDS,
            "iss": self.id,
        });
        let payload = URL_SAFE_NO_PAD.encode(claims.to_string());
        let signed = format!("{header}.{payload}");
        let mut signature = vec![0; self.key.public().modulus_len()];
        self.key
            .sign(
                &RSA_PKCS1_SHA256,
                &SystemRandom::new(),
                signed.as_bytes(),
                &mut signature,
            )
            .map_err(|error| GithubError::Key(error.to_string()))?;
        Ok(format!("{signed}.{}", URL_SAFE_NO_PAD.encode(signature)))
    }

    async fn send<T: DeserializeOwned>(
        &self,
        request: RequestBuilder,
        jwt: &str,
        url: &str,
    ) -> Result<Option<T>, GithubError> {
        let failed = |source| GithubError::Request {
            url: url.to_owned(),
            source,
        };
        let response = request
            .bearer_auth(jwt)
            .header(ACCEPT, API_ACCEPT)
            .header(USER_AGENT, CLIENT_NAME)
            .send()
            .await
            .map_err(failed)?;
        match response.status() {
            StatusCode::NOT_FOUND => Ok(None),
            status if status.is_success() => Ok(Some(response.json().await.map_err(failed)?)),
            status => Err(GithubError::Answer {
                url: url.to_owned(),
                status,
            }),
        }
    }
}

impl fmt::Debug for GithubApp {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GithubApp")
            .field("id", &self.id)
            .field("api", &self.api)
            .finish_non_exhaustive()
    }
}

impl fmt::Debug for RepoToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RepoToken")
            .field("expires", &self.expires)
            .finish_non_exhaustive()
    }
}

fn key_pair(pem: &str) -> Result<RsaKeyPair, GithubError> {
    let body: String = pem
        .lines()
        .filter(|line| !line.starts_with("-----"))
        .map(str::trim)
        .collect();
    let der = STANDARD
        .decode(body)
        .map_err(|error| GithubError::Key(error.to_string()))?;
    let pair = if pem.contains(PKCS1_HEADER) {
        RsaKeyPair::from_der(&der)
    } else {
        RsaKeyPair::from_pkcs8(&der)
    };
    pair.map_err(|error| GithubError::Key(error.to_string()))
}
