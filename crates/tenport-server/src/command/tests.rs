use super::*;

fn args(text: &str) -> Vec<String> {
    text.split_whitespace().map(str::to_owned).collect()
}

fn registration(text: &str) -> Registration {
    match parse(&args(text)) {
        Ok(Command::UserAdd(registration)) => registration,
        other => panic!("not a user add: {other:?}"),
    }
}

#[test]
fn no_arguments_serve() {
    assert!(matches!(parse(&[]), Ok(Command::Serve)));
}

#[test]
fn user_add_reads_every_flag() {
    let added = registration(
        "user add --name Ayse --github Ayse-K --org demo --email a@x.dev --role admin",
    );
    assert_eq!(added.org.as_str(), "demo");
    assert_eq!(added.user.name.as_str(), "Ayse");
    assert_eq!(added.user.github.as_str(), "ayse-k");
    assert_eq!(
        added
            .user
            .email
            .map(|email| email.as_str().to_owned())
            .as_deref(),
        Some("a@x.dev")
    );
    assert_eq!(added.role, Role::Admin);
    assert_eq!(added.token_name.as_str(), "user add");
}

#[test]
fn a_user_is_a_member_with_no_email_by_default() {
    let added = registration("user add --name Can --github can --org demo");
    assert_eq!(added.role, Role::Member);
    assert_eq!(added.user.email, None);
}

#[test]
fn user_add_needs_a_name_a_login_and_an_org() {
    let missing = |text: &str| parse(&args(text)).err();
    assert_eq!(
        missing("user add --github can --org demo"),
        Some(CommandError::Missing("--name"))
    );
    assert_eq!(
        missing("user add --name Can --org demo"),
        Some(CommandError::Missing("--github"))
    );
    assert_eq!(
        missing("user add --name Can --github can"),
        Some(CommandError::Missing("--org"))
    );
}

#[test]
fn a_wrong_flag_or_value_is_refused() {
    let error = |text: &str| parse(&args(text)).err().map(|error| error.to_string());
    let base = "user add --name Can --github can --org demo";
    assert_eq!(
        error(&format!("{base} --role owner")).as_deref(),
        Some("--role: admin or member")
    );
    assert_eq!(
        error(&format!("{base} --type admin")).as_deref(),
        Some("unknown flag: --type")
    );
    assert_eq!(
        error(&format!("{base} --org other")).as_deref(),
        Some("--org is given twice")
    );
    assert_eq!(
        error(&format!("{base} --email")).as_deref(),
        Some("--email needs a value")
    );
    assert_eq!(
        error("user add --name Can --github -can --org demo").as_deref(),
        Some("--github: a GitHub login has 1 to 39 letters, digits and single hyphens inside")
    );
    assert_eq!(
        error("user remove").as_deref(),
        Some("unknown command: user remove")
    );
}
