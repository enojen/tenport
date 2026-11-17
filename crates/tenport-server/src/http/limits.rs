use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use tokio::sync::Semaphore;

use super::AppState;
use super::error::HttpError;
use crate::accounts::UserId;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub request_timeout: Duration,
    pub concurrent_requests: usize,
    pub requests_per_second: u32,
    pub request_burst: u32,
    pub log_streams_per_user: usize,
    pub body_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            request_timeout: Duration::from_secs(30),
            concurrent_requests: 64,
            requests_per_second: 10,
            request_burst: 20,
            log_streams_per_user: 5,
            body_bytes: 2 * 1024 * 1024,
        }
    }
}

pub struct Limiter {
    limits: Limits,
    requests: Semaphore,
    buckets: Mutex<HashMap<UserId, Bucket>>,
    log_streams: Arc<Mutex<HashMap<UserId, usize>>>,
}

struct Bucket {
    tokens: f64,
    filled_at: Instant,
}

pub struct LogStreamPermit {
    log_streams: Arc<Mutex<HashMap<UserId, usize>>>,
    user: UserId,
}

impl Limiter {
    pub fn new(limits: Limits) -> Self {
        Self {
            limits,
            requests: Semaphore::new(limits.concurrent_requests),
            buckets: Mutex::new(HashMap::new()),
            log_streams: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn limits(&self) -> &Limits {
        &self.limits
    }

    pub fn take_request(&self, user: &UserId, now: Instant) -> Result<(), HttpError> {
        let rate = f64::from(self.limits.requests_per_second);
        let burst = f64::from(self.limits.request_burst);
        let mut buckets = lock(&self.buckets);
        let bucket = buckets.entry(user.clone()).or_insert(Bucket {
            tokens: burst,
            filled_at: now,
        });
        let elapsed = now
            .saturating_duration_since(bucket.filled_at)
            .as_secs_f64();
        bucket.tokens = (bucket.tokens + elapsed * rate).min(burst);
        bucket.filled_at = now;
        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            return Ok(());
        }
        Err(HttpError::TooManyRequests {
            retry_after: seconds_until_one_token(bucket.tokens, rate),
        })
    }

    pub fn open_log_stream(&self, user: &UserId) -> Result<LogStreamPermit, HttpError> {
        let limit = self.limits.log_streams_per_user;
        let mut streams = lock(&self.log_streams);
        let open = streams.entry(user.clone()).or_insert(0);
        if *open >= limit {
            return Err(HttpError::TooManyLogStreams { limit });
        }
        *open += 1;
        Ok(LogStreamPermit {
            log_streams: self.log_streams.clone(),
            user: user.clone(),
        })
    }
}

impl Drop for LogStreamPermit {
    fn drop(&mut self) {
        let mut streams = lock(&self.log_streams);
        if let Some(open) = streams.get_mut(&self.user) {
            *open = open.saturating_sub(1);
            if *open == 0 {
                streams.remove(&self.user);
            }
        }
    }
}

pub async fn limit_requests(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let limiter = &state.limiter;
    let _place = match limiter.requests.acquire().await {
        Ok(place) => place,
        Err(closed) => return HttpError::internal(closed).into_response(),
    };
    let timeout = limiter.limits.request_timeout;
    match tokio::time::timeout(timeout, next.run(request)).await {
        Ok(response) => response,
        Err(_) => HttpError::Timeout {
            seconds: timeout.as_secs(),
        }
        .into_response(),
    }
}

fn seconds_until_one_token(tokens: f64, rate: f64) -> u64 {
    let seconds = ((1.0 - tokens) / rate).ceil();
    if seconds < 1.0 { 1 } else { seconds as u64 }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
