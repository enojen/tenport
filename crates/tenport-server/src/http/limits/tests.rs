use super::*;

fn limiter() -> Limiter {
    Limiter::new(Limits {
        requests_per_second: 10,
        request_burst: 2,
        log_streams_per_user: 2,
        ..Limits::default()
    })
}

fn user(id: &str) -> UserId {
    serde_json::from_value(serde_json::json!(id)).unwrap()
}

#[test]
fn a_user_takes_the_burst_then_waits_for_the_rate() {
    let limiter = limiter();
    let now = Instant::now();
    let enes = user("u-aaaaaa");
    assert!(limiter.take_request(&enes, now).is_ok());
    assert!(limiter.take_request(&enes, now).is_ok());
    let refused = limiter.take_request(&enes, now);
    assert!(matches!(
        refused,
        Err(HttpError::TooManyRequests { retry_after: 1 })
    ));
    let later = now + Duration::from_millis(100);
    assert!(limiter.take_request(&enes, later).is_ok());
}

#[test]
fn each_user_has_their_own_rate() {
    let limiter = limiter();
    let now = Instant::now();
    let enes = user("u-aaaaaa");
    let ali = user("u-bbbbbb");
    for _ in 0..2 {
        limiter.take_request(&enes, now).unwrap();
    }
    assert!(limiter.take_request(&enes, now).is_err());
    assert!(limiter.take_request(&ali, now).is_ok());
}

#[test]
fn a_closed_log_stream_gives_its_place_back() {
    let limiter = limiter();
    let enes = user("u-aaaaaa");
    let first = limiter.open_log_stream(&enes).unwrap();
    let _second = limiter.open_log_stream(&enes).unwrap();
    assert!(matches!(
        limiter.open_log_stream(&enes),
        Err(HttpError::TooManyLogStreams { limit: 2 })
    ));
    drop(first);
    assert!(limiter.open_log_stream(&enes).is_ok());
}
