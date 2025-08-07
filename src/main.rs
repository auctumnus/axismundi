use askama::Template;
use axum::{
    Form, Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{Html, Redirect},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use util::AppState;
use std::net::SocketAddr;
use tower::ServiceBuilder;
use tower_http::{cors::CorsLayer, services::ServeDir};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use anyhow::Result;
mod model;
mod util;
mod email;
mod controller;

#[derive(Template)]
#[template(path = "about.html")]
struct AboutTemplate;

#[derive(Template)]
#[template(path = "contact.html")]
struct ContactTemplate;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    dotenvy::dotenv()?;
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "axismundi=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://user:password@localhost/axismundi".to_string());

    let pool = PgPool::connect(&database_url).await?;

    sqlx::migrate!("./migrations").run(&pool).await?;

    let s3_config = util::s3::S3Config::new()
        .map_err(|e| anyhow::anyhow!("Failed to initialize S3 config: {}", e))?;

    let app = create_router(pool, s3_config);

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    tracing::debug!("listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

fn create_router(pool: PgPool, s3: util::s3::S3Config) -> Router {
    Router::new()
        .nest("/api", controller::api::create_api_controller(pool.clone(), s3.clone()))
        .route("/", get(home))
        .route("/about", get(about))
        .route("/contact", get(contact))
        .nest_service("/static", ServeDir::new("frontend/dist"))
        .nest_service("/assets", ServeDir::new("frontend/dist/assets"))
        .layer(ServiceBuilder::new().layer(CorsLayer::permissive()))
        .with_state(AppState { pool, s3 })
}

async fn home(State(AppState { .. }): State<AppState>) -> Result<Html<String>, StatusCode> {
    let html = "meow".to_string();
    Ok(Html(html))
}

async fn about() -> Result<Html<String>, StatusCode> {
    let template = AboutTemplate;
    let html = template
        .render()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Html(html))
}

async fn contact() -> Result<Html<String>, StatusCode> {
    let template = ContactTemplate;
    let html = template
        .render()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Html(html))
}
