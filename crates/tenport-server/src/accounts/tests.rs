use super::*;
use crate::db;
use crate::name::Name;

fn registration(organization: &str, github: &str) -> Registration {
    Registration {
        organization: Name::parse(organization).unwrap(),
        user: NewUser {
            name: Name::parse("Enes").unwrap(),
            email: None,
            github: GithubLogin::parse(github).unwrap(),
        },
        role: Role::Admin,
        token_name: Name::parse("laptop").unwrap(),
    }
}

#[tokio::test]
async fn registers_a_user_with_an_organization_and_a_token() {
    let (db, _dir) = db::temp().await;
    let registered = register(&db, Actor::Server, registration(" Wegobi ", "enojen"))
        .await
        .unwrap();
    assert!(registered.user.as_str().starts_with("u-"));
    assert!(registered.organization.as_str().starts_with("o-"));
    let user = authenticate(&db, &registered.token).await.unwrap().unwrap();
    assert_eq!(user.id, registered.user);
    assert_eq!(user.github.as_str(), "enojen");
    let memberships = memberships(&db, &user.id).await.unwrap();
    assert_eq!(
        memberships,
        vec![Membership {
            organization: registered.organization,
            organization_name: Name::parse("Wegobi").unwrap(),
            role: Role::Admin,
        }]
    );
}

#[tokio::test]
async fn a_second_user_joins_the_organization_that_exists() {
    let (db, _dir) = db::temp().await;
    let first = register(&db, Actor::Server, registration("Wegobi", "enojen"))
        .await
        .unwrap();
    let second = register(&db, Actor::Server, registration("Wegobi", "ali"))
        .await
        .unwrap();
    assert_eq!(first.organization, second.organization);
}

#[tokio::test]
async fn refuses_a_github_login_that_exists_and_writes_nothing() {
    let (db, _dir) = db::temp().await;
    register(&db, Actor::Server, registration("Wegobi", "enojen"))
        .await
        .unwrap();
    let error = register(&db, Actor::Server, registration("Other", "enojen"))
        .await
        .unwrap_err();
    assert!(
        matches!(&error, AccountsError::UserExists { github } if github.as_str() == "enojen"),
        "{error}"
    );
    assert!(organization_id(&db, "Other").await.is_none());
}

#[tokio::test]
async fn an_unknown_token_finds_nobody() {
    let (db, _dir) = db::temp().await;
    register(&db, Actor::Server, registration("Wegobi", "enojen"))
        .await
        .unwrap();
    let user = authenticate(&db, &Token::random()).await.unwrap();
    assert!(user.is_none());
}

#[test]
fn a_token_has_the_tenbox_shape_and_hides_itself() {
    let token = Token::random();
    let plain = token.expose();
    assert!(
        plain.starts_with("tbx_") && plain.len() == 4 + 64,
        "{plain}"
    );
    assert_eq!(format!("{token:?}"), "Token(..)");
    assert_eq!(token.hash().as_str().len(), 64);
    assert_ne!(token.hash().as_str(), plain);
}

#[test]
fn a_token_of_another_shape_does_not_parse() {
    let hex = "ab".repeat(32);
    for bad in [
        String::new(),
        hex.clone(),
        format!("tbx_{}", &hex[1..]),
        format!("tbx_{}", hex.to_uppercase()),
        format!("tbx_{}g", &hex[1..]),
    ] {
        assert_eq!(Token::parse(&bad).unwrap_err(), TokenError, "{bad:?}");
    }
}

#[test]
fn the_hash_is_the_sha256_that_tenbox_stores() {
    let token = Token::parse(&format!("tbx_{}", "ab".repeat(32))).unwrap();
    assert_eq!(
        token.hash().as_str(),
        "8ac99102a9d0a6f44c09534998705f918a91a5e12dde9d5814e28f920fd0e03b"
    );
}

#[test]
fn a_github_login_is_lowercase_letters_digits_and_single_hyphens() {
    assert_eq!(GithubLogin::parse(" EnoJen ").unwrap().as_str(), "enojen");
    assert!(GithubLogin::parse("a-b-1").is_ok());
    for bad in ["", "-a", "a-", "a--b", "a_b", "a b", &"a".repeat(40)] {
        assert_eq!(GithubLogin::parse(bad), Err(GithubLoginError), "{bad:?}");
    }
}

#[test]
fn an_email_has_one_at_with_text_on_both_sides() {
    assert_eq!(Email::parse(" a@b.c ").unwrap().as_str(), "a@b.c");
    for bad in ["", "a", "@b", "a@", "a@b@c"] {
        assert_eq!(Email::parse(bad), Err(EmailError), "{bad:?}");
    }
}

#[test]
fn an_event_with_a_bad_github_login_does_not_read() {
    let body = serde_json::json!({
        "type": "user.added",
        "id": "u-abcdef",
        "name": "Enes",
        "email": null,
        "github": "bad login"
    });
    assert!(serde_json::from_value::<AccountsEvent>(body).is_err());
}
