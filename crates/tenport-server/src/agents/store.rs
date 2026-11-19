use sqlx::{SqliteConnection, SqliteExecutor};

use super::model::{Agent, AgentAdded, AgentId, LoginChanged};
use crate::accounts::UserId;
use crate::events::Stamp;

pub async fn insert_agent(
    conn: &mut SqliteConnection,
    added: &AgentAdded,
    stamp: &Stamp<'_>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "insert into agents (id, user_id, kind, name, is_default, created_at, created_by)
         values (?, ?, ?, ?, ?, ?, ?)",
        added.agent,
        added.user,
        added.kind,
        added.name,
        added.default,
        stamp.at,
        stamp.actor,
    )
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn update_agent_login(
    conn: &mut SqliteConnection,
    changed: &LoginChanged,
    stamp: &Stamp<'_>,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query!(
        "update agents set login_changed_at = ? where id = ?",
        stamp.at,
        changed.agent,
    )
    .execute(conn)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub async fn list_agents_of_user(
    executor: impl SqliteExecutor<'_>,
    user: &UserId,
) -> Result<Vec<Agent>, sqlx::Error> {
    sqlx::query_as!(
        Agent,
        r#"select id as "id: _", kind as "kind: _", name as "name: _", is_default as "default: _",
                  login_changed_at as "login_changed: _"
           from agents where user_id = ?
           order by created_at, id"#,
        user,
    )
    .fetch_all(executor)
    .await
}

pub async fn find_agent_of_user(
    executor: impl SqliteExecutor<'_>,
    user: &UserId,
    agent: &AgentId,
) -> Result<Option<Agent>, sqlx::Error> {
    sqlx::query_as!(
        Agent,
        r#"select id as "id: _", kind as "kind: _", name as "name: _", is_default as "default: _",
                  login_changed_at as "login_changed: _"
           from agents where user_id = ? and id = ?"#,
        user,
        agent,
    )
    .fetch_optional(executor)
    .await
}

pub async fn find_default_agent_of_user(
    executor: impl SqliteExecutor<'_>,
    user: &UserId,
) -> Result<Option<AgentId>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"select id as "id: AgentId" from agents where user_id = ? and is_default"#,
        user,
    )
    .fetch_optional(executor)
    .await
}
