// ignore unused warnings
#![warn(clippy::pedantic, clippy::style)]
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::too_many_lines)]
use askama::Template;
use axum::{
    Router,
    http::{HeaderMap, StatusCode},
};
use sqlx::PgPool;
use std::net::SocketAddr;
use tower::ServiceBuilder;
use tower_http::cors::CorsLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use util::AppState;

use crate::{
    config::CONFIG, email::MockEmailService, model::users::User, util::extract_session::Session,
};
mod config;
mod controller;
mod email;
mod err;
mod md;
mod model;
mod pagination;
mod util;

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

    let email_service = std::sync::Arc::new(MockEmailService::new());

    let app_state = AppState {
        pool: pool.clone(),
        email_service,
    };

    let app = create_router(app_state);

    let addr = SocketAddr::from(([127, 0, 0, 1], CONFIG.port));
    tracing::debug!("listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;

    Ok(())
}

fn create_router(app_state: AppState) -> Router {
    Router::new()
        .nest("/api", controller::api::create_api_controller())
        .merge(controller::html::create_html_controller())
        .fallback(fallback)
        .layer(ServiceBuilder::new().layer(CorsLayer::permissive()))
        .with_state(app_state)
}

#[derive(Template)]
#[template(path = "error.html")]
struct ErrorTemplate {
    current_user: Option<User>,
    error: err::AppError,
}

async fn fallback(sess: Session, req_headers: HeaderMap) -> (StatusCode, HeaderMap, String) {
    let content_type = req_headers
        .get(axum::http::header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    // this is so ugly but if i just do `impl IntoResponse` rust will infer
    // the type to always be `text/html` and i want to avoid that
    match content_type {
        s if s.contains("text/html") => {
            let template = ErrorTemplate {
                current_user: sess.user().cloned(),
                error: err::AppError::new("Not Found".to_owned(), StatusCode::NOT_FOUND),
            };
            let mut headers = HeaderMap::new();
            headers.insert(
                axum::http::header::CONTENT_TYPE,
                "text/html".parse().unwrap(),
            );
            (
                StatusCode::NOT_FOUND,
                headers,
                template.render().unwrap_or_else(|e| {
                    tracing::error!("Template rendering error: {}", e);
                    "500 Internal Server Error".to_owned()
                }),
            )
        }
        _ => {
            let mut headers = HeaderMap::new();
            headers.insert(
                axum::http::header::CONTENT_TYPE,
                "text/plain".parse().unwrap(),
            );
            (StatusCode::NOT_FOUND, headers, "not found".to_owned())
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use axum::routing::RouterIntoService;

    use super::*;

    // Re-export test utilities from controller::api::tests
    pub(crate) use crate::controller::api::tests::make_authed_user;

    pub(crate) async fn test_app() -> Result<RouterIntoService<axum::body::Body>, sqlx::Error> {
        let pool = PgPool::connect(&CONFIG.database_url).await?;
        let email_service = std::sync::Arc::new(email::MockEmailService::new());
        let app_state = AppState {
            pool,
            email_service,
        };
        let app = create_router(app_state).into_service();

        Ok(app)
    }

    pub(crate) async fn test_app_with_admin_and_email_service(
        email_service: &std::sync::Arc<crate::email::MockEmailService>,
    ) -> (RouterIntoService<axum::body::Body>, String) {
        let pool = PgPool::connect(&CONFIG.database_url).await.unwrap();
        let email_service_clone = email_service.clone();
        let email_service_trait: std::sync::Arc<dyn crate::email::EmailService> =
            email_service_clone.clone();
        let app_state = AppState {
            pool: pool.clone(),
            email_service: email_service_trait,
        };
        let app = create_router(app_state).into_service();

        let username = crate::tests::random_name();
        let token = crate::tests::make_authed_user(&username, &app, email_service_clone).await;

        let id = sqlx::query_scalar!("select id from users where username = $1", username)
            .fetch_one(&pool)
            .await
            .unwrap();

        sqlx::query!(
            "insert into user_tags (user_id, tag, hidden) values ($1, 'admin', false)",
            id
        )
        .execute(&pool)
        .await
        .unwrap();
        (app, token)
    }

    pub(crate) async fn test_app_with_email_service(
        email_service: &std::sync::Arc<dyn crate::email::EmailService>,
    ) -> Result<RouterIntoService<axum::body::Body>, sqlx::Error> {
        let pool = PgPool::connect(&CONFIG.database_url).await?;
        let email_service = email_service.clone();
        let app_state = AppState {
            pool,
            email_service,
        };
        let app = create_router(app_state).into_service();

        Ok(app)
    }

    pub(crate) async fn response_to_value(body: axum::body::Body) -> serde_json::Value {
        let bytes = axum::body::to_bytes(body, usize::MAX).await.unwrap();
        serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()
    }

    pub(crate) fn random_name() -> String {
        format!(
            "{}_{}",
            random_word::get(random_word::Lang::En),
            nanoid::nanoid!(4).replace('_', "")
        )
        .replace('-', "")
        .to_lowercase()
    }

    pub(crate) fn random_code() -> String {
        // Use lowercase letters and numbers only to ensure it matches USERNAME_REGEX
        const ALPHABET: [char; 36] = [
            'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm',
            'n', 'o', 'p', 'q', 'r', 's', 't', 'u', 'v', 'w', 'x', 'y', 'z',
            '0', '1', '2', '3', '4', '5', '6', '7', '8', '9',
        ];
        nanoid::nanoid!(8, &ALPHABET)
    }
}
