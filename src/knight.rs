// ignore unused warnings
#![warn(clippy::pedantic, clippy::style)]
#![allow(clippy::uninlined_format_args)]
use axum::{
    extract::State, http::{StatusCode}, Json, Router
};
use bollard::{query_parameters::RestartContainerOptions, Docker};
use serde::Deserialize;
use std::net::SocketAddr;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::config::CONFIG;
mod config;

#[derive(Clone, Debug)]
pub struct KnightState {
    pub docker: Docker,
    pub pool: sqlx::PgPool,
}

pub enum KnightError {
    DatabaseError(sqlx::Error),
    DockerError(bollard::errors::Error),
    Other(String),
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

    let docker = Docker::connect_with_socket_defaults()?;

    let knight_state = KnightState {
        docker,
        pool,
    };

    let app = Router::new()
        .route("/kill_maids", axum::routing::post(kill_maids))
        .route("/health_check", axum::routing::get(health_check))
        .with_state(knight_state);

    let addr = SocketAddr::from(([127, 0, 0, 1], CONFIG.knight.port));
    tracing::debug!("listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;

    axum::serve(listener, app).await?;

    Ok(())
}
#[derive(Debug, Deserialize)]
pub struct KillMaidsRequest {
    pub maid_ids: Vec<String>,
}

async fn kill_maids(
    State(state): State<KnightState>,
    Json(payload): Json<KillMaidsRequest>,
) -> Result<(), KnightError> {
    for maid in payload.maid_ids {
        if let Err(err) = state.docker.restart_container(&maid, Some(RestartContainerOptions {
            t: Some(5),
            signal: None,
        })).await {
            tracing::error!("Failed to kill maid {maid}: {err}");
        }

        sqlx::query!(
            r#"
            UPDATE maids SET state = 'dead' WHERE identity = $1
            "#,
            maid
        ).execute(&state.pool).await.map_err(KnightError::DatabaseError)?;
    }

    Ok(())
}

async fn health_check(
    State(state): State<KnightState>,
) -> (StatusCode, &'static str) {
    if let Err(err) = state.docker.ping().await {
        tracing::error!("Failed to ping docker: {err}");
        return (StatusCode::SERVICE_UNAVAILABLE, "docker unreachable");
    }

    state.pool.
}