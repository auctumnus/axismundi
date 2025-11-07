use askama::Template;
use axum::{Router, extract::Query, response::{Html, Redirect}, routing::{get, post}};
use axum_extra::extract::{CookieJar, cookie::Cookie};
use reqwest::StatusCode;
use serde::Deserialize;
use uuid::Uuid;

use crate::{controller::html::{render_result, render_template}, err::AppError, model::{email_verification_tokens::EmailVerificationTokenRepository, sessions::SessionRepository, users::{CreateUser, User, UserRepository}}, util::{AppState, extract_session::{SESSION_COOKIE_NAME, Session}}};

pub fn create_router() -> (Router<AppState>, Router<AppState>) {
    let secure_routes = Router::<AppState>::new()
        .route("/login", post(login_submit))
        .route("/register", post(signup_submit))
        .route("/resend-verification/{token_id}", post(resend_verification_submit))
        .route("/verify/{user_id}", get(verify));
    let normal_routes = Router::<AppState>::new()
        .route("/login", get(login_form))
        .route("/register", get(signup_form))
        .route("/resend-verification/{token_id}", get(resend_verification_form));

    (secure_routes, normal_routes)
}


#[derive(Template)]
#[template(path = "login/form.html")]
#[allow(dead_code)]
struct LoginFormTemplate {
    current_user: Option<User>,
    error: Option<AppError>,
}

async fn login_form(s: Session) -> Html<String> {
    let current_user = s.user().cloned();
    render_template(LoginFormTemplate { current_user, error: None })
}

#[derive(Deserialize)]
struct LoginFormData {
    email: String,
    password: String,
}

async fn login_submit(jar: CookieJar, s: Session, sessions: SessionRepository, form: axum::Form<LoginFormData>) -> Result<(CookieJar, Redirect), (StatusCode, Html<String>)>{
    sessions.login(&form.email, &form.password).await
        .map(|(token, _)| {
            let jar = jar.add(
                Cookie::build((SESSION_COOKIE_NAME, token.clone()))
                    .path("/")
                    .http_only(true)
                    .secure(true),
            );

            (jar, Redirect::to("/"))
        })
        .map_err(|e| {
            let current_user = s.user().cloned();
            let body = render_template(LoginFormTemplate {
                current_user,
                error: Some(e),
            });
            (StatusCode::UNAUTHORIZED, body)
        })
}

#[derive(Template)]
#[template(path = "signup/form.html")]
#[allow(dead_code)]
struct SignupFormTemplate {
    current_user: Option<User>,
    error: Option<AppError>,
}

async fn signup_form(s: Session) -> Html<String> {
    let current_user = s.user().cloned();
    render_template(SignupFormTemplate { current_user, error: None })
}

#[derive(Deserialize)]
struct SignupFormData {
    username: String,
    email: String,
    password: String,
}

#[derive(Template)]
#[template(path = "signup/success.html")]
#[allow(dead_code)]
struct SignupSuccessTemplate {
    current_user: Option<User>,
    token_id: Uuid,
}

async fn signup_submit(users: UserRepository, form: axum::Form<SignupFormData>) -> (StatusCode, Html<String>) {
    users.create(CreateUser {
        email: form.email.clone(),
        password: form.password.clone(),
        username: form.username.clone(),
        display_name: None,
        description: None,
        pronouns: None,
        gender: None,
    }).await
        .map_or_else(|e| {
            let body = render_template(SignupFormTemplate {
                current_user: None,
                error: Some(e),
            });
            (StatusCode::BAD_REQUEST, body)
        }, |(_, token)| {
            (StatusCode::OK, render_template(SignupSuccessTemplate { current_user: None, token_id: token.id }))
        })
}

#[derive(Template)]
#[template(path = "signup/resend-verification.html")]
#[allow(dead_code)]
struct ResendVerificationTemplate {
    current_user: Option<User>,
    token_id: Uuid,
    error: Option<AppError>,
}

async fn resend_verification_form(s: Session, path: axum::extract::Path<Uuid>) -> Html<String> {
    let current_user = s.user().cloned();
    let token_id = *path;
    render_template(ResendVerificationTemplate { current_user, token_id, error: None })
}

#[derive(Deserialize)]
struct ResendVerificationFormData {
    token_id: Uuid,
}

async fn resend_verification_submit(tokens: EmailVerificationTokenRepository, form: axum::Form<ResendVerificationFormData>) -> (StatusCode, Html<String>) {
    tokens.resend(form.token_id).await
        .map_or_else(|e| {
            let body = render_template(ResendVerificationTemplate {
                current_user: None,
                token_id: form.token_id,
                error: Some(e),
            });
            (StatusCode::BAD_REQUEST, body)
        }, |token| {
            (StatusCode::OK, render_template(ResendVerificationTemplate {
                current_user: None,
                token_id: token.id,
                error: None,
            }))
        })
}

#[derive(Template)]
#[template(path = "signup/verified.html")]
struct VerifiedTemplate {
    current_user: Option<User>,
}

#[derive(Deserialize)]
pub(crate) struct VerifyEmail {
    token: String,
    email: String,
}

#[axum::debug_handler(state=AppState)]
async fn verify(users: UserRepository, path: axum::extract::Path<Uuid>, Query(verify): Query<VerifyEmail>) -> (StatusCode, Html<String>) {
    let res = users.verify(*path, &verify.email, &verify.token).await
        .map(|_| VerifiedTemplate { current_user: None });

    render_result(None, res)
}