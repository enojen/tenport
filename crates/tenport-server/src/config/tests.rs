use super::*;

fn config(pairs: &[(&str, &str)]) -> Result<Config, ConfigError> {
    Config::from_lookup(|name| {
        pairs
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| (*value).to_owned())
    })
}

#[test]
fn listens_on_localhost_when_no_address_is_set() {
    let config = config(&[(DATABASE, "/data/tenport.db")]).unwrap();
    assert_eq!(config.listen, "127.0.0.1:8080".parse().unwrap());
    assert_eq!(config.database, PathBuf::from("/data/tenport.db"));
}

#[test]
fn reads_the_address_that_is_set() {
    let config = config(&[(LISTEN, "0.0.0.0:3000"), (DATABASE, "tenport.db")]).unwrap();
    assert_eq!(config.listen, "0.0.0.0:3000".parse().unwrap());
}

#[test]
fn refuses_a_missing_database() {
    assert_eq!(config(&[]), Err(ConfigError::Missing { name: DATABASE }));
}

#[test]
fn refuses_an_address_without_a_port() {
    assert_eq!(
        config(&[(LISTEN, "0.0.0.0"), (DATABASE, "tenport.db")]),
        Err(ConfigError::BadAddress {
            name: LISTEN,
            value: "0.0.0.0".to_owned()
        })
    );
}

#[test]
fn runs_use_the_mounted_template_and_namespace_tenbox_by_default() {
    let config = config(&[(DATABASE, "tenport.db")]).unwrap();
    assert_eq!(
        config.run_template,
        PathBuf::from("/etc/tenport/run-template")
    );
    assert_eq!(
        config.run_services,
        PathBuf::from("/etc/tenport/run-services")
    );
    assert_eq!(config.run_namespace, "tenbox");
    assert_eq!(config.box_image, "ghcr.io/enojen/tenbox-box:latest");
}

#[test]
fn reads_the_run_settings_that_are_set() {
    let config = config(&[
        (DATABASE, "tenport.db"),
        (RUN_TEMPLATE, "/t"),
        (RUN_SERVICES, "/s"),
        (RUN_NAMESPACE, "boxes"),
        (BOX_IMAGE, "box:1"),
    ])
    .unwrap();
    assert_eq!(config.run_template, PathBuf::from("/t"));
    assert_eq!(config.run_services, PathBuf::from("/s"));
    assert_eq!(config.run_namespace, "boxes");
    assert_eq!(config.box_image, "box:1");
}

#[test]
fn has_no_archive_bucket_when_none_of_its_names_is_set() {
    let config = config(&[(DATABASE, "tenport.db")]).unwrap();
    assert_eq!(config.task_archive_bucket, None);
}

#[test]
fn reads_the_archive_bucket_when_all_its_names_are_set() {
    let config = config(&[
        (DATABASE, "tenport.db"),
        (
            TASK_ARCHIVE_BUCKET,
            "https://objects.example.test/tenbox-test",
        ),
        (TASK_ARCHIVE_BUCKET_REGION, "hel1"),
        (TASK_ARCHIVE_BUCKET_KEY_FILE, "/bucket/key"),
        (TASK_ARCHIVE_BUCKET_SECRET_FILE, "/bucket/secret"),
    ])
    .unwrap();
    assert_eq!(
        config.task_archive_bucket,
        Some(TaskArchiveBucketConfig {
            url: "https://objects.example.test/tenbox-test".to_owned(),
            region: "hel1".to_owned(),
            key_file: PathBuf::from("/bucket/key"),
            secret_file: PathBuf::from("/bucket/secret"),
        })
    );
}

#[test]
fn refuses_an_archive_bucket_that_misses_one_of_its_names() {
    assert_eq!(
        config(&[
            (DATABASE, "tenport.db"),
            (
                TASK_ARCHIVE_BUCKET,
                "https://objects.example.test/tenbox-test"
            ),
            (TASK_ARCHIVE_BUCKET_REGION, "hel1"),
            (TASK_ARCHIVE_BUCKET_KEY_FILE, "/bucket/key"),
        ]),
        Err(ConfigError::Missing {
            name: TASK_ARCHIVE_BUCKET_SECRET_FILE
        })
    );
}
