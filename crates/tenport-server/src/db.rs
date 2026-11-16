use std::path::{Path, PathBuf};
use std::time::Duration;

use sqlx::migrate::{MigrateError, Migrator};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions};

#[cfg(test)]
mod tests;

static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

pub type Db = SqlitePool;

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("cannot open the database {}: {source}", path.display())]
    Open { path: PathBuf, source: sqlx::Error },
    #[error("cannot migrate the database: {0}")]
    Migrate(#[from] MigrateError),
}

pub async fn open(path: &Path) -> Result<Db, DbError> {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .foreign_keys(true)
        .busy_timeout(BUSY_TIMEOUT);
    let db = SqlitePoolOptions::new()
        .connect_with(options)
        .await
        .map_err(|source| DbError::Open {
            path: path.to_owned(),
            source,
        })?;
    MIGRATOR.run(&db).await?;
    Ok(db)
}
