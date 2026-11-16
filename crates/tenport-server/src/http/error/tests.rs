use std::io;
use std::sync::{Arc, Mutex};

use axum::body::to_bytes;
use serde_json::Value;

use super::*;

#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl io::Write for Captured {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn answer_with_logs(error: HttpError) -> (Response, String) {
    let captured = Captured::default();
    let writer = captured.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(move || writer.clone())
        .with_ansi(false)
        .finish();
    let response = tracing::subscriber::with_default(subscriber, || error.into_response());
    let logs = String::from_utf8(captured.0.lock().unwrap().clone()).unwrap();
    (response, logs)
}

#[tokio::test]
async fn a_server_error_logs_its_source_and_hides_it_from_the_client() {
    let error = HttpError::internal(io::Error::other("disk is full"));
    let (response, logs) = answer_with_logs(error);
    assert!(
        logs.contains("ERROR") && logs.contains("disk is full"),
        "{logs}"
    );
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let problem: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(problem["detail"], "the server failed; see its log");
    assert_eq!(problem["code"], "internal");
}

#[test]
fn a_client_error_logs_nothing() {
    let (response, logs) = answer_with_logs(HttpError::Unauthorized);
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(logs, "");
}
