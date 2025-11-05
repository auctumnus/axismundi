use std::sync::Arc;

use crate::{
    ErrorTemplate,
    err::AppError,
    model::users::User,
    util::{AppState, extract_session::Session},
};
use askama::Template;
use axum::{Router, http::StatusCode, response::Html, routing::get};
use governor::middleware::NoOpMiddleware;
use tower_governor::governor::GovernorConfig;
use tower_http::services::ServeDir;

pub fn create_html_controller() -> Router<AppState> {
    let secure_governor = Arc::new(GovernorConfig::<_, NoOpMiddleware>::secure());
    let normal_governor = Arc::new(GovernorConfig::<_, NoOpMiddleware>::default());

    let secure_limiter = secure_governor.limiter().clone();
    let normal_limiter = normal_governor.limiter().clone();
    let interval = std::time::Duration::from_secs(60);

    std::thread::spawn(move || {
        loop {
            std::thread::sleep(interval);
            secure_limiter.retain_recent();
            normal_limiter.retain_recent();
        }
    });

    let secure_routes = Router::<AppState>::new();

    let normal_routes = Router::<AppState>::new()
        .route("/", get(home))
        .route("/about", get(about))
        .route("/contact", get(contact))
        .nest_service("/static", ServeDir::new("frontend/dist"))
        .nest_service("/assets", ServeDir::new("assets"));

    Router::<AppState>::new()
        .merge(secure_routes)
        .merge(normal_routes)
}

#[derive(Template)]
#[template(path = "home.html")]
#[allow(dead_code)]
struct HomeTemplate {
    current_user: Option<User>,
}

#[derive(Template)]
#[template(path = "about.html")]
struct AboutTemplate;

#[derive(Template)]
#[template(path = "contact.html")]
struct ContactTemplate;

#[derive(Template)]
#[template(path = "login/form.html")]
#[allow(dead_code)]
struct LoginFormTemplate {
    error: Option<AppError>,
}

fn render_template<T: Template>(template: T) -> Html<String> {
    template.render().map_or_else(
        |e| {
            tracing::error!("Template rendering error: {}", e);
            Html("500 Internal Server Error".to_string())
        },
        Html,
    )
}

#[allow(dead_code)]
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

#[allow(dead_code)]
fn login_form() -> Html<String> {
    render_template(LoginFormTemplate { error: None })
}
