use askama::Template;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{Html, Redirect},
    routing::{get, post},
    Form, Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::net::SocketAddr;
use tower::ServiceBuilder;
use tower_http::{cors::CorsLayer, services::ServeDir};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
struct User {
    id: uuid::Uuid,
    name: String,
    email: String,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
struct CreateUser {
    name: String,
    email: String,
}

#[derive(Template)]
#[template(path = "home.html")]
struct HomeTemplate {
    users: Vec<User>,
}

#[derive(Template)]
#[template(path = "about.html")]
struct AboutTemplate;

#[derive(Template)]
#[template(path = "contact.html")]
struct ContactTemplate;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
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

    let app = create_router(pool);

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    tracing::debug!("listening on {}", addr);
    
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

fn create_router(pool: PgPool) -> Router {
    Router::new()
        .route("/", get(home))
        .route("/about", get(about))
        .route("/contact", get(contact))
        .route("/api/users", get(get_users).post(create_user))
        .route("/api/users/{id}", get(get_user))
        .route("/api/users-form", post(create_user_form))
        .nest_service("/static", ServeDir::new("frontend/dist"))
        .nest_service("/assets", ServeDir::new("frontend/dist/assets"))
        .layer(
            ServiceBuilder::new()
                .layer(CorsLayer::permissive())
        )
        .with_state(pool)
}

async fn home(State(pool): State<PgPool>) -> Result<Html<String>, StatusCode> {
    let users = sqlx::query_as::<_, User>(
        "SELECT id, name, email, created_at FROM users ORDER BY created_at DESC"
    )
    .fetch_all(&pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let template = HomeTemplate { users };
    let html = template.render().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Html(html))
}

async fn about() -> Result<Html<String>, StatusCode> {
    let template = AboutTemplate;
    let html = template.render().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Html(html))
}

async fn contact() -> Result<Html<String>, StatusCode> {
    let template = ContactTemplate;
    let html = template.render().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Html(html))
}

async fn get_users(
    axum::extract::State(pool): axum::extract::State<PgPool>,
) -> Result<Json<Vec<User>>, StatusCode> {
    let users = sqlx::query_as::<_, User>(
        "SELECT id, name, email, created_at FROM users ORDER BY created_at DESC"
    )
    .fetch_all(&pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(users))
}

async fn get_user(
    Path(user_id): Path<uuid::Uuid>,
    axum::extract::State(pool): axum::extract::State<PgPool>,
) -> Result<Json<User>, StatusCode> {
    let user = sqlx::query_as::<_, User>(
        "SELECT id, name, email, created_at FROM users WHERE id = $1"
    )
    .bind(user_id)
    .fetch_one(&pool)
    .await
    .map_err(|_| StatusCode::NOT_FOUND)?;

    Ok(Json(user))
}

async fn create_user(
    axum::extract::State(pool): axum::extract::State<PgPool>,
    Json(payload): Json<CreateUser>,
) -> Result<Json<User>, StatusCode> {
    let user_id = uuid::Uuid::new_v4();
    let now = chrono::Utc::now();

    let user = sqlx::query_as::<_, User>(
        "INSERT INTO users (id, name, email, created_at) VALUES ($1, $2, $3, $4) RETURNING id, name, email, created_at"
    )
    .bind(user_id)
    .bind(payload.name)
    .bind(payload.email)
    .bind(now)
    .fetch_one(&pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(user))
}

async fn create_user_form(
    State(pool): State<PgPool>,
    Form(payload): Form<CreateUser>,
) -> Result<Redirect, StatusCode> {
    let user_id = uuid::Uuid::new_v4();
    let now = chrono::Utc::now();

    sqlx::query(
        "INSERT INTO users (id, name, email, created_at) VALUES ($1, $2, $3, $4)"
    )
    .bind(user_id)
    .bind(payload.name)
    .bind(payload.email)
    .bind(now)
    .execute(&pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Redirect::to("/"))
}
