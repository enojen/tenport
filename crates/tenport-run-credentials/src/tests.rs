use super::*;

const KEY: &str = "key-for-tests";
const JOB_UID: &str = "0f3c1c9e-5a7b-4e2d-9c1a-7b6d5e4f3a21";

#[test]
fn makes_the_placeholder_that_the_tenbox_server_makes_for_the_same_key_and_job() {
    assert_eq!(
        run_placeholder(KEY, JOB_UID),
        "tbxph_ETORSF6tK5Aq-PPCa0hncaILEOC-43LTqxG9R15MW38"
    );
}

#[test]
fn another_job_or_another_key_gives_another_placeholder() {
    let placeholder = run_placeholder(KEY, JOB_UID);

    assert_ne!(run_placeholder(KEY, "another-uid"), placeholder);
    assert_ne!(run_placeholder("another-key", JOB_UID), placeholder);
}
