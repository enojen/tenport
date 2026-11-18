use std::fs;

use super::*;

#[test]
fn the_run_token_secret_of_a_job_is_its_name_and_github() {
    let job = JobName::new("task-ten-301-1118-101500");

    assert_eq!(
        run_token_secret(&job).as_str(),
        "task-ten-301-1118-101500-github"
    );
}

#[test]
fn reads_the_key_without_the_line_end_and_hides_it_in_debug() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("key");
    fs::write(&path, "key-for-tests\n").unwrap();

    let key = RunCredentialKey::read(&path).unwrap();

    let uid = JobUid::new("0f3c1c9e-5a7b-4e2d-9c1a-7b6d5e4f3a21");
    assert_eq!(
        key.placeholder(&uid),
        "tbxph_ETORSF6tK5Aq-PPCa0hncaILEOC-43LTqxG9R15MW38"
    );
    assert!(!format!("{key:?}").contains("key-for-tests"));
}

#[test]
fn an_empty_or_missing_key_file_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let empty = dir.path().join("empty");
    fs::write(&empty, "\n").unwrap();

    assert!(matches!(
        RunCredentialKey::read(&empty),
        Err(RunCredentialsError::Empty { .. })
    ));
    assert!(matches!(
        RunCredentialKey::read(&dir.path().join("missing")),
        Err(RunCredentialsError::Read { .. })
    ));
}

#[test]
fn the_injector_token_matches_only_itself_and_hides_in_debug() {
    let token = InjectorToken::new("injector-token-for-tests");

    assert!(token.matches("injector-token-for-tests"));
    assert!(!token.matches("injector-token-for-test"));
    assert!(!format!("{token:?}").contains("injector-token"));
}
