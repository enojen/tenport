use ring::signature::{RSA_PKCS1_2048_8192_SHA256, UnparsedPublicKey};
use serde_json::Value;
use time::macros::datetime;

use super::*;

const TEST_KEY: &str = include_str!("../../tests/github-app-test-key.pem");

fn app() -> GithubApp {
    GithubApp::new("5669876", TEST_KEY, "https://api.example.test/").unwrap()
}

fn claims(jwt: &str) -> Value {
    let payload = jwt.split('.').nth(1).unwrap();
    serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload).unwrap()).unwrap()
}

#[test]
fn the_app_jwt_is_signed_with_the_app_key_and_good_for_ten_minutes_around_now() {
    let app = app();
    let now = datetime!(2026-11-18 10:00:00 UTC);

    let jwt = app.jwt(now).unwrap();

    let (signed, signature) = jwt.rsplit_once('.').unwrap();
    let public = UnparsedPublicKey::new(&RSA_PKCS1_2048_8192_SHA256, app.key.public().as_ref());
    public
        .verify(
            signed.as_bytes(),
            &URL_SAFE_NO_PAD.decode(signature).unwrap(),
        )
        .unwrap();
    let claims = claims(&jwt);
    assert_eq!(claims["iss"], "5669876");
    assert_eq!(claims["iat"], now.unix_timestamp() - 60);
    assert_eq!(claims["exp"], now.unix_timestamp() + 540);
}

#[test]
fn a_key_that_is_not_an_rsa_key_is_refused() {
    let broken = "-----BEGIN RSA PRIVATE KEY-----\nbm90IGEga2V5\n-----END RSA PRIVATE KEY-----\n";

    assert!(matches!(
        GithubApp::new("1", broken, GITHUB_API),
        Err(GithubError::Key(_))
    ));
}

#[test]
fn the_debug_texts_hide_the_key_and_the_token() {
    let token = RepoToken {
        token: "ghs_secret".to_owned(),
        expires: datetime!(2026-11-18 11:00:00 UTC),
    };

    assert!(!format!("{token:?}").contains("ghs_secret"));
    assert!(format!("{:?}", app()).contains("5669876"));
}
