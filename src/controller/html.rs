use std::sync::Arc;

use crate::{
    ErrorTemplate,
    err::{AppError, AppResult, bad_request, not_found},
    model::{
        session::{SessionObj, SessionRepository},
        user::{CreateUser, User, UserRepository},
    },
    util::{
        AppState,
        extract_session::{SESSION_COOKIE_NAME, Session},
        s3::S3,
    },
};
use askama::Template;
use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::Html,
    routing::{get, post, put},
};
use axum_extra::extract::{CookieJar, Multipart, cookie::Cookie};
use chrono::{DateTime, Utc};
use governor::middleware::NoOpMiddleware;
use serde::{Deserialize, Serialize};
use tower_governor::governor::GovernorConfig;
use tower_http::services::ServeDir;

pub fn create_html_controller() -> Router<AppState> {
    Router::new()
        .route("/", get(home))
        .route("/about", get(about))
        .route("/contact", get(contact))
        .nest_service("/static", ServeDir::new("frontend/dist"))
        .nest_service("/assets", ServeDir::new("frontend/dist/assets"))
}

#[derive(Template)]
#[template(path = "home.html")]
struct HomeTemplate {
    current_user: Option<User>,
}

#[derive(Template)]
#[template(path = "about.html")]
struct AboutTemplate;

#[derive(Template)]
#[template(path = "contact.html")]
struct ContactTemplate;

fn render_template<T: Template>(template: T) -> Html<String> {
    template.render().map_or_else(
        |e| {
            tracing::error!("Template rendering error: {}", e);
            Html("500 Internal Server Error".to_string())
        },
        Html,
    )
}

fn render_result<T: Template>(res: Result<T, AppError>) -> (Html<String>, StatusCode) {
    match res {
        Ok(t) => {
            let html = render_template(t);
            (html, StatusCode::OK)
        }
        Err(e) => {
            let html = render_template(ErrorTemplate { error: e.clone() });
            (html, e.status_code)
        }
    }
}

async fn home(s: Session) -> Html<String> {
    let current_user = s.user().cloned();
    render_template(HomeTemplate { current_user })
}

async fn about() -> Html<String> {
    render_template(AboutTemplate)
}

async fn contact() -> Html<String> {
    render_template(ContactTemplate)
}
