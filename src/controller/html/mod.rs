use std::sync::Arc;

use crate::{
    ErrorTemplate,
    err::AppError,
    model::users::{User, UserRepository},
    model::languages::Language,
    util::{AppState, extract_session::Session},
};
use askama::Template;
use axum::{Router, http::StatusCode, response::{Html, IntoResponse, Redirect}, routing::get};
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
        .route("/", get(landing))
        .route("/home", get(home))
        .nest_service("/static", ServeDir::new("frontend/dist"))
        .nest_service("/assets", ServeDir::new("assets"))
        .merge(normal_user_routes);

    Router::<AppState>::new()
        .merge(secure_routes)
        .merge(normal_routes)
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

#[derive(Template)]
#[template(path = "landing.html")]
struct LandingTemplate {
    current_user: Option<User>,
}

async fn landing(s: Session) -> impl IntoResponse {
    if let Some(_user) = s.user() {
        return Redirect::to("/home").into_response();
    }

    let current_user = s.user().cloned();
    render_template(LandingTemplate { current_user }).into_response()
}

#[derive(Template)]
#[template(path = "home.html")]
struct HomeTemplate {
    current_user: Option<User>,
    languages: Vec<Language>,
}

async fn home(users: UserRepository, s: Session) -> Result<impl IntoResponse, Html<String>> {
    let Some(user) = s.user().cloned() else {
        return Ok(Redirect::to("/").into_response());
    };

    let languages = users.top_languages(user.id, 5).await.map_err(error_template(Some(&user)))?;

    let template = HomeTemplate {
        current_user: Some(user),
        languages,
    };

    let body = render_template(template);
    Ok((StatusCode::OK, body).into_response())
}

pub fn error_template(current_user: Option<&User>) -> impl FnOnce(AppError) -> Html<String> {
    move |error| render_template(ErrorTemplate { current_user: current_user.cloned(), error })
}