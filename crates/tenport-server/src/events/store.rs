use sqlx::SqliteConnection;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub id: String,
    pub kind: String,
    pub subject: String,
    pub actor: String,
    pub at: String,
    pub body: String,
}

pub async fn insert(conn: &mut SqliteConnection, row: &Row) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        "insert into events (id, type, subject, actor, at, body) values (?, ?, ?, ?, ?, ?)
         on conflict (id) do nothing",
        row.id,
        row.kind,
        row.subject,
        row.actor,
        row.at,
        row.body,
    )
    .execute(conn)
    .await?;
    Ok(done.rows_affected() == 1)
}

pub async fn find(conn: &mut SqliteConnection, id: &str) -> Result<Option<Row>, sqlx::Error> {
    sqlx::query_as!(
        Row,
        r#"select id, type as "kind", subject, actor, at, body from events where id = ?"#,
        id,
    )
    .fetch_optional(conn)
    .await
}

#[cfg(test)]
pub async fn count(conn: &mut SqliteConnection) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!("select count(*) from events")
        .fetch_one(conn)
        .await
}
