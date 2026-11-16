use super::*;

async fn text(db: &Db, sql: &'static str) -> String {
    let row: (String,) = sqlx::query_as(sql).fetch_one(db).await.unwrap();
    row.0
}

#[tokio::test]
async fn creates_the_file_when_it_is_missing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tenport.db");
    open(&path).await.unwrap();
    assert!(path.exists());
}

#[tokio::test]
async fn writes_ahead_and_checks_foreign_keys() {
    let dir = tempfile::tempdir().unwrap();
    let db = open(&dir.path().join("tenport.db")).await.unwrap();
    assert_eq!(
        text(&db, "select journal_mode from pragma_journal_mode").await,
        "wal"
    );
    assert_eq!(
        text(
            &db,
            "select cast(foreign_keys as text) from pragma_foreign_keys"
        )
        .await,
        "1"
    );
}

#[tokio::test]
async fn opens_the_same_file_twice() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tenport.db");
    open(&path).await.unwrap().close().await;
    open(&path).await.unwrap();
}

#[tokio::test]
async fn names_the_path_it_cannot_open() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("missing").join("tenport.db");
    let error = open(&path).await.unwrap_err();
    assert!(error.to_string().contains("missing/tenport.db"), "{error}");
}

#[tokio::test]
async fn answers_a_ping_until_it_is_closed() {
    let dir = tempfile::tempdir().unwrap();
    let db = open(&dir.path().join("tenport.db")).await.unwrap();
    assert!(ping(&db).await.is_ok());
    db.close().await;
    assert!(ping(&db).await.is_err());
}
