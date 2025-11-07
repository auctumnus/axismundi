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

mod users;

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
    
    let (secure_user_routes, normal_user_routes) = users::create_router();

    let secure_routes = Router::<AppState>::new()
        .merge(secure_user_routes);

    let normal_routes = Router::<AppState>::new()
        .route("/", get(home))
        .nest_service("/static", ServeDir::new("frontend/dist"))
        .nest_service("/assets", ServeDir::new("assets"))
        .merge(normal_user_routes);

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

fn render_template<T: Template>(template: T) -> Html<String> {
    template.render().map_or_else(
        |e| {
            tracing::error!("Template rendering error: {}", e);
            Html("500 Internal Server Error".to_string())
        },
        Html,
    )
}

pub fn render_result<T: Template>(current_user: Option<User>, res: Result<T, AppError>) -> (StatusCode, Html<String>) {
    match res {
        Ok(t) => {
            let html = render_template(t);
            (StatusCode::OK, html)
        }
        Err(e) => {
            let html = render_template(ErrorTemplate { current_user, error: e.clone() });
            (StatusCode::OK, html)
        }
    }
}

async fn home(s: Session) -> Html<String> {
    let current_user = s.user().cloned();
    render_template(HomeTemplate { current_user })
}
