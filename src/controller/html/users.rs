use askama::Template;
use axum::{Router, extract::Query, response::{Html, IntoResponse, Redirect, Response}, routing::{get, post}};
use axum_extra::extract::{CookieJar, cookie::Cookie};
use reqwest::StatusCode;
use serde::Deserialize;
use uuid::Uuid;

use crate::{controller::html::{okay, render_generic_error, render_template, LanguagesWithContributors}, err::AppError, model::{contribution_stats::ContributionStatsRepository, email_verification_tokens::EmailVerificationTokenRepository, languages::{LanguageRepository, LanguageSearch}, sessions::SessionRepository, translatable::{TranslatableRepository, TranslatableSearch}, user_activities::UserActivityRepository, users::{CreateUser, UpdateUser, User, UserRepository}}, pagination::PaginatedRequest, util::{AppState, extract_session::{SESSION_COOKIE_NAME, Session}}};

pub fn create_router() -> (Router<AppState>, Router<AppState>) {
    let secure_routes = Router::<AppState>::new()
        .route("/verify/{user_id}", get(verify));
    let normal_routes = Router::<AppState>::new()
        .route("/login", get(login_form).post(login_submit))
        .route("/register", get(signup_form).post(signup_submit))
        .route("/resend-verification/{token_id}", get(resend_verification_form).post(resend_verification_submit))
        .route("/settings", get(settings_form).post(settings_submit))
        .route("/logout", get(logout_form).post(logout_submit))
        .route("/users/{username}", get(profile));

    (secure_routes, normal_routes)
}

pub async fn render_login_form(s: Session, error: Option<AppError>, redirect_url: Option<String>) -> (StatusCode, Response) {
    let current_user = s.user().cloned();
    okay(render_template(LoginFormTemplate { current_user, error, redirect_url }))
}

#[derive(Template)]
#[template(path = "login/form.html")]
#[allow(dead_code)]
struct LoginFormTemplate {
    current_user: Option<User>,
    error: Option<AppError>,
    redirect_url: Option<String>,
}

async fn login_form(s: Session) -> (StatusCode, Response) {
    render_login_form(s, None, None).await
}

#[derive(Deserialize)]
struct LoginFormData {
    email: String,
    password: String,
}

#[derive(Deserialize)]
struct LoginQuery {
    redirect: Option<String>,
}

const ALLOWED_REDIRECTS: &[&str] = &["settings"];

async fn login_submit(jar: CookieJar, s: Session, sessions: SessionRepository, query: Query<LoginQuery>, form: axum::Form<LoginFormData>) -> (CookieJar, (StatusCode, Response)) {
    match sessions.login(&form.email, &form.password).await {
        Ok((token, _)) => {
            let jar = jar.add(
                Cookie::build((SESSION_COOKIE_NAME, token.clone()))
                    .path("/")
                    .http_only(true)
                    .secure(true),
            );

            let redirect = if ALLOWED_REDIRECTS.contains(&query.redirect.as_deref().unwrap_or("")) {
                Redirect::to(&format!("/{}", query.redirect.as_deref().unwrap()))
            } else {
                Redirect::to("/")
            };

            (jar, (StatusCode::SEE_OTHER, redirect.into_response()))
        }
        Err(e) => {
            let current_user = s.user().cloned();
            let body = render_template(LoginFormTemplate {
                current_user,
                error: Some(e),
                redirect_url: query.redirect.clone(),
            });
            (jar, (StatusCode::UNAUTHORIZED, body))
        }
    }
}

#[derive(Template)]
#[template(path = "signup/form.html")]
#[allow(dead_code)]
struct SignupFormTemplate {
    current_user: Option<User>,
    error: Option<AppError>,
    previous_input: Option<SignupFormData>,
}

async fn signup_form(s: Session) -> (StatusCode, Response) {
    let current_user = s.user().cloned();
    okay(render_template(SignupFormTemplate { current_user, error: None, previous_input: None }))
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

async fn signup_submit(users: UserRepository, form: axum::Form<SignupFormData>) -> (StatusCode, Response) {
    let res = users.create(CreateUser {
        email: form.email.clone(),
        password: form.password.clone(),
        username: form.username.clone(),
        display_name: None,
        description: None,
        pronouns: None,
        gender: None,
    }).await;

    match res {
        Err(e) => {
            let template = SignupFormTemplate {
                current_user: None,
                error: Some(e),
                previous_input: Some(SignupFormData {
                    username: form.username.clone(),
                    email: form.email.clone(),
                    password: String::new(),
                }),
            };
            let body = render_template(template);
            (StatusCode::BAD_REQUEST, body)
        },
        Ok((_, token)) => okay(render_template(SignupSuccessTemplate { current_user: None, token_id: token.id })),
    }
}

#[derive(Template)]
#[template(path = "signup/resend-verification.html")]
#[allow(dead_code)]
struct ResendVerificationTemplate {
    current_user: Option<User>,
    token_id: Uuid,
    error: Option<AppError>,
}

async fn resend_verification_form(s: Session, path: axum::extract::Path<Uuid>) -> (StatusCode, Response) {
    let current_user = s.user().cloned();
    let token_id = *path;
    okay(render_template(ResendVerificationTemplate { current_user, token_id, error: None }))
}

#[derive(Deserialize)]
struct ResendVerificationFormData {
    token_id: Uuid,
}

async fn resend_verification_submit(tokens: EmailVerificationTokenRepository, form: axum::Form<ResendVerificationFormData>) -> (StatusCode, Response) {
    tokens.resend(form.token_id).await
        .map_or_else(|e| {
            let body = render_template(ResendVerificationTemplate {
                current_user: None,
                token_id: form.token_id,
                error: Some(e),
            });
            (StatusCode::BAD_REQUEST, body)
        }, |token| {
            okay(render_template(ResendVerificationTemplate {
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
async fn verify(users: UserRepository, path: axum::extract::Path<Uuid>, Query(verify): Query<VerifyEmail>) -> (StatusCode, Response) {
    let res = users.verify(*path, &verify.email, &verify.token).await
        .map(|_| VerifiedTemplate { current_user: None });

    match res {
        Ok(template) => {
            let body = render_template(template);
            (StatusCode::OK, body)
        },
        Err(e) => {
            let body = render_template(VerifiedTemplate { current_user: None });
            (StatusCode::BAD_REQUEST, body)
        }
    }
}

#[derive(Clone, Deserialize)]
struct SettingsFormData {
    username: Option<String>,
    display_name: Option<String>,
    description: Option<String>,
    pronouns: Option<String>,
    gender: Option<String>,
    email: Option<String>,
    current_password: Option<String>,
    new_password: Option<String>,
}

mod ma {
    macro_rules! prev {
        ($current_user:ident, $previous_input:ident, $field:ident) => {
            $previous_input.as_ref().and_then(|p| p.$field.clone()).or($current_user.as_ref().and_then(|u| u.$field.clone())).unwrap_or(String::new())
        };
    }
    pub(crate) use prev;
}

use ma::prev;

#[derive(Template)]
#[template(path = "settings.html")]
struct SettingsTemplate {
    current_user: Option<User>,
    error: Option<AppError>,
    previous_input: Option<SettingsFormData>,
    previous_username: String,
    previous_email: String,
}
async fn settings_form(s: Session) -> (StatusCode, Response) {
    let Some(user) = s.user().cloned() else {
        return (StatusCode::SEE_OTHER, Redirect::to("/").into_response());
    };

    let template = SettingsTemplate {
        previous_username: user.username.clone(),
        previous_email: user.email.clone(),
        current_user: Some(user),
        error: None,
        previous_input: None,
    };

    let body = render_template(template);
    (StatusCode::OK, body)
}

fn coalesce(in_form: &Option<String>, in_resource: &Option<String>) -> Option<String> {
    in_form.clone().and_then(|p| {
        if in_resource.as_ref().map_or(false, |r| r == &p) {
            // Value unchanged, don't update
            None
        } else {
            // Value changed (including empty string to clear the field)
            // Empty string will be converted to NULL in the SQL query
            Some(p)
        }
    })
}

async fn settings_submit(s: Session, users: UserRepository, form: axum::Form<SettingsFormData>) -> (StatusCode, Response) {
    let Some(user) = s.user().cloned() else {
        return (StatusCode::SEE_OTHER, Redirect::to("/").into_response());
    };

    println!("display name in form: {:?}", form.display_name);

    match users.update(&user, user.id, UpdateUser {
        username: coalesce(&form.username, &Some(user.username.clone())),
        email: coalesce(&form.email, &Some(user.email.clone())),
        display_name: coalesce(&form.display_name, &user.display_name),
        description: coalesce(&form.description, &user.description),
        pronouns: coalesce(&form.pronouns, &user.pronouns),
        gender: coalesce(&form.gender, &user.gender),
        current_password: form.current_password.clone(),
        new_password: form.new_password.clone(),
    }).await {
        Ok(_) => (StatusCode::SEE_OTHER, Redirect::to("/settings").into_response()),
        Err(e) => {
            let redacted = SettingsFormData {
                username: form.username.clone(),
                display_name: form.display_name.clone(),
                description: form.description.clone(),
                pronouns: form.pronouns.clone(),
                email: form.email.clone(),
                gender: form.gender.clone(),
                current_password: None,
                new_password: None,
            };

            let template = SettingsTemplate {
                previous_username: user.username.clone(),
                previous_email: user.email.clone(),
                current_user: Some(user),
                error: Some(e),
                previous_input: Some(redacted),
            };

            let body = render_template(template);
            (StatusCode::BAD_REQUEST, body)
        }
    }
}

#[derive(Template)]
#[template(path = "logout/form.html")]
struct LogoutTemplate {
    current_user: Option<User>,
    error: Option<AppError>,
}

async fn logout_form(s: Session) -> (StatusCode, Response) {
    let current_user = s.user().cloned();
    okay(render_template(LogoutTemplate { current_user, error: None }))
}

async fn logout_submit(jar: CookieJar, s: Session, sessions: SessionRepository) -> (CookieJar, (StatusCode, Response)) {
    if let Some(session) = s.session() {
        let _ = sessions.invalidate(session.clone()).await;
    }

    let jar = jar.remove(Cookie::from(SESSION_COOKIE_NAME));

    (jar, (StatusCode::SEE_OTHER, Redirect::to("/").into_response()))
}

#[derive(Template)]
#[template(path = "users/profile.html")]
struct ProfileTemplate {
    current_user: Option<User>,
    user: User,
    languages: Vec<LanguagesWithContributors>,
    activities: Vec<crate::model::user_activities::UserActivity>,
    translatables: Vec<crate::model::translatable::Translatable>,
    rendered_description: String,
}

async fn profile(
    s: Session,
    users: UserRepository,
    languages: LanguageRepository,
    translatables: TranslatableRepository,
    activities: UserActivityRepository,
    contribution_stats: ContributionStatsRepository,
    path: axum::extract::Path<String>,
) -> (StatusCode, Response) {
    let username = path.0;
    let current_user = s.user().cloned();

    let user_result = users.find_by_username(&username).await;

    let user = match user_result {
        Ok(u) => u,
        Err(e) => return render_generic_error(s, e).await,
    };

    let languages_result = languages.search(PaginatedRequest {
        limit: 5,
        offset: 0,
    }, LanguageSearch {
        owned_by: Some(username.clone()),
        ..Default::default()
    }).await;

    let mut languages_with_contributors = Vec::new();
    if let Ok(paginated) = languages_result {
        for lang in &paginated.items {
            let top_contributors = contribution_stats.get_top_contributors(&lang.id, 5).await.unwrap_or_default();
            let is_liked = if let Some(ref cu) = current_user {
                languages.is_liked(&cu.id, &lang.id).await.unwrap_or(false)
            } else {
                false
            };
            languages_with_contributors.push(LanguagesWithContributors {
                language: lang.clone(),
                top_contributors,
                is_liked,
            });
        }
    }
    let languages_list = languages_with_contributors;

    // Get user activities (limit to 5)
    let activities_result = activities.list_by_user(
        current_user.as_ref(),
        user.id,
        None,
        PaginatedRequest {
            limit: 5,
            offset: 0,
        }
    ).await;

    let activities_list = match activities_result {
        Ok(paginated) => paginated.items,
        Err(_) => Vec::new(), // If we can't fetch activities, just show empty list
    };

    let translatables = translatables.search(PaginatedRequest { limit: 5, offset: 0 }, TranslatableSearch {
        created_by: Some(username.clone()),
        ..Default::default()
    }).await;

    let translatables_list = match translatables {
        Ok(paginated) => paginated.items,
        Err(_) => Vec::new(),
    };

    let rendered_description = users.render_description(&user).await.unwrap_or_default();

    (StatusCode::OK, render_template(ProfileTemplate {
        current_user,
        user,
        languages: languages_list,
        activities: activities_list,
        translatables: translatables_list,
        rendered_description,
    }).into_response())
}