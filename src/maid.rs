use crate::{config::CONFIG, util::AppState};

mod config;
mod err;
mod email;
mod util;
mod tasks;

use sqlx::PgPool;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use tasks::{Maid, Task, TaskState, TaskType};

struct MaidState {
    id: String,
    pool: PgPool,
    email: std::sync::Arc<dyn email::EmailService + Send + Sync>,
    client: reqwest::Client,
}

#[derive(Debug)]
pub enum MaidError {
    DatabaseError(sqlx::Error),
    EmailError(String),
    KnightError,
    Other(String),
}

async fn work(state: MaidState) -> Result<(), MaidError> {
    // try to update the first task in ready state to active, taking it for this maid
    let task = sqlx::query_as!(
        Task,
        r#"
        UPDATE tasks
        SET state = 'active',
            started_at = NOW(),
            taken_by = $1
        WHERE id = (
            SELECT id FROM tasks
            WHERE state = 'ready'
            ORDER BY scheduled_at ASC
            FOR UPDATE SKIP LOCKED
            LIMIT 1
        )
        RETURNING id, type as "type: TaskType", state as "state: TaskState", payload, scheduled_at, started_at, taken_by
        "#,
        state.id
    ).fetch_optional(&state.pool).await
        .map_err(MaidError::DatabaseError)?;

    let Some(task) = task else {
        // no tasks available
        return Ok(());
    };

    // ... do the task ...

    Ok(())
}

async fn gossip(state: MaidState) -> Result<(), MaidError> {
    // tell the central server we're alive
    sqlx::query!(
        r#"
        UPDATE maids
        SET checked_in_at = NOW()
        WHERE identity = $1
        "#,
        state.id
    ).execute(&state.pool).await
    .map_err(MaidError::DatabaseError)?;

    let stalled_maids = sqlx::query_as!(
        Maid,
        r#"
        SELECT id, identity, started_at, checked_in_at, state as "state: tasks::MaidState"
        FROM maids
        WHERE checked_in_at < NOW() - INTERVAL '1 minute' AND state = 'alive'
        "#
    ).fetch_all(&state.pool).await
    .map_err(MaidError::DatabaseError)?;

    state.client.post(format!("{}/kill_maids", CONFIG.maid.knight_url))
        .json(&serde_json::json!({
            "maid_ids": stalled_maids.iter().map(|m| m.id.to_string()).collect::<Vec<_>>(),
        }))
        .send()
        .await
        .map_err(|e| MaidError::KnightError)?;

    todo!()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "axismundi=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // this can't be static like the S3 config, because it has special behavior
    // on Drop
    let pool = PgPool::connect(&CONFIG.database_url).await?;

    // read hostname from /etc/hostname
    let hostname = tokio::fs::read_to_string("/etc/hostname").await?;

    sqlx::query!(
        r#"
        INSERT INTO maids (identity, checked_in_at)
        VALUES ($1, NOW())
        "#,
        hostname.trim()
    ).execute(&pool).await?;

    let client = reqwest::Client::new();

    let maid_state = MaidState {
        id: hostname.trim().to_string(),
        pool,
        email: std::sync::Arc::new(email::make_email_service(&CONFIG.resend)),
        client,
    };

    Ok(())
}