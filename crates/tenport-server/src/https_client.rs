use std::sync::Arc;
use std::time::Duration;

use reqwest::Client;

#[derive(Debug, thiserror::Error)]
#[error("cannot make the HTTPS client: {0}")]
pub struct HttpsError(String);

pub fn https_client(timeout: Duration) -> Result<Client, HttpsError> {
    let roots = rustls::RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
    };
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let tls = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|error| HttpsError(error.to_string()))?
        .with_root_certificates(roots)
        .with_no_client_auth();
    Client::builder()
        .tls_backend_preconfigured(tls)
        .timeout(timeout)
        .build()
        .map_err(|error| HttpsError(error.to_string()))
}
