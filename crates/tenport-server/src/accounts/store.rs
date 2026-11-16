use sqlx::SqliteConnection;

use super::model::OrganizationAdded;

#[cfg(test)]
pub struct Organization {
    pub id: String,
    pub name: String,
    pub created_by: String,
}

pub async fn insert_organization(
    conn: &mut SqliteConnection,
    added: &OrganizationAdded,
    at: &str,
    by: &str,
) -> Result<(), sqlx::Error> {
    let id = added.id.as_str();
    sqlx::query!(
        "insert into organizations (id, name, created_at, created_by) values (?, ?, ?, ?)",
        id,
        added.name,
        at,
        by,
    )
    .execute(conn)
    .await?;
    Ok(())
}

#[cfg(test)]
pub async fn organization_by_name(
    conn: &mut SqliteConnection,
    name: &str,
) -> Result<Option<Organization>, sqlx::Error> {
    sqlx::query_as!(
        Organization,
        "select id, name, created_by from organizations where name = ?",
        name,
    )
    .fetch_optional(conn)
    .await
}
