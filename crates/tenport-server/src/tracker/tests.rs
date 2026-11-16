use super::*;

#[test]
fn a_tracker_url_is_an_http_link() {
    assert!(TrackerUrl::parse("https://app.clickup.com/t/abc").is_ok());
    for bad in ["", "https://", "ftp://x", "https://a b", "app.clickup.com"] {
        assert_eq!(TrackerUrl::parse(bad), Err(TrackerUrlError), "{bad:?}");
    }
}
