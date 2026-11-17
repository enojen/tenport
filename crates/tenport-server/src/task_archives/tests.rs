use std::fs;

use super::*;

const URL: &str = "https://objects.example.test/tenbox-test";

fn bucket_at(url: &str) -> Result<TaskArchiveBucket, TaskArchivesError> {
    TaskArchiveBucket::new(url, "hel1", "key-id".to_owned(), "key-secret".to_owned())
}

fn project() -> ProjectId {
    ProjectId::parse("7ihtah").unwrap()
}

fn task() -> TaskId {
    TaskId::parse("TEN-301").unwrap()
}

#[test]
fn the_urls_of_a_task_point_at_its_archive_in_the_bucket_for_a_day() {
    let urls = bucket_at(URL).unwrap().presign(&project(), &task());

    let object = format!("{URL}/tasks/7ihtah/TEN-301.tar.zst?");
    assert!(urls.get.starts_with(&object), "{}", urls.get);
    assert!(urls.put.starts_with(&object), "{}", urls.put);
    assert!(urls.get.contains("X-Amz-Expires=86400"), "{}", urls.get);
    assert!(
        urls.get.contains("X-Amz-Credential=key-id%2F"),
        "{}",
        urls.get
    );
    assert!(!urls.get.contains("key-secret"), "{}", urls.get);
    assert_ne!(urls.get, urls.put);
}

#[test]
fn a_bucket_url_with_a_trailing_slash_gives_the_same_urls() {
    let urls = bucket_at(&format!("{URL}/"))
        .unwrap()
        .presign(&project(), &task());

    assert!(
        urls.get.starts_with(&format!("{URL}/tasks/")),
        "{}",
        urls.get
    );
}

#[test]
fn a_bucket_url_without_one_bucket_name_is_refused() {
    for url in [
        "https://objects.example.test",
        "https://objects.example.test/a/b",
        "objects.example.test/a",
        "ftp://objects.example.test/a",
    ] {
        assert!(
            matches!(bucket_at(url), Err(TaskArchivesError::BadUrl { .. })),
            "{url}"
        );
    }
}

#[test]
fn the_debug_text_hides_the_key() {
    let text = format!("{:?}", bucket_at(URL).unwrap());

    assert!(text.contains("tenbox-test"), "{text}");
    assert!(
        !text.contains("key-id") && !text.contains("key-secret"),
        "{text}"
    );
}

#[test]
fn opens_the_keys_from_their_files_without_the_line_end() {
    let dir = tempfile::tempdir().unwrap();
    let key_file = dir.path().join("key");
    let secret_file = dir.path().join("secret");
    fs::write(&key_file, "key-id\n").unwrap();
    fs::write(&secret_file, "key-secret\n").unwrap();
    let config = TaskArchiveBucketConfig {
        url: URL.to_owned(),
        region: "hel1".to_owned(),
        key_file,
        secret_file,
    };

    let urls = TaskArchiveBucket::open(&config)
        .unwrap()
        .presign(&project(), &task());

    assert!(
        urls.get.contains("X-Amz-Credential=key-id%2F"),
        "{}",
        urls.get
    );
}

#[test]
fn an_empty_or_missing_key_file_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let empty = dir.path().join("empty");
    fs::write(&empty, "\n").unwrap();
    let config = |key_file: PathBuf| TaskArchiveBucketConfig {
        url: URL.to_owned(),
        region: "hel1".to_owned(),
        key_file,
        secret_file: empty.clone(),
    };

    assert!(matches!(
        TaskArchiveBucket::open(&config(empty.clone())),
        Err(TaskArchivesError::Empty { .. })
    ));
    assert!(matches!(
        TaskArchiveBucket::open(&config(dir.path().join("missing"))),
        Err(TaskArchivesError::Read { .. })
    ));
}
