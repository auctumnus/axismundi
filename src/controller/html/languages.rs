use askama::Template;
use axum::{Router, response::{Html, IntoResponse, Redirect, Response}, routing::{get, post}};
use reqwest::StatusCode;
use serde::Deserialize;

use crate::{attempt, controller::html::{okay, render_template}, err::AppError, get_user, model::{languages::{CreateLanguage, Language, LanguageRepository}, users::{User, UserRepository}}, util::{AppState, extract_session::Session}};

pub fn create_router() -> (Router<AppState>, Router<AppState>) {
    let secure_routes = Router::<AppState>::new()
        .route("/new-language", post(new_language_submit))
        .route("/languages/{code}/edit", post(edit_language_submit));
    let normal_routes = Router::<AppState>::new()
        .route("/new-language", get(new_language_form))
        .route("/languages/{code}", get(view_language))
        .route("/languages/{code}/edit", get(edit_language_form));

    (secure_routes, normal_routes)
}


#[derive(Template)]
#[template(path = "languages/new.html")]
#[allow(dead_code)]
struct NewLanguageFormTemplate {
    current_user: Option<User>,
    error: Option<AppError>,
    previous_code: String,
    previous_name: String,
    previous_description: String,
}

async fn new_language_form(s: Session) -> (StatusCode, Response) {
    let user = get_user!(s);

    let template = NewLanguageFormTemplate {
        current_user: Some(user),
        error: None,
        previous_code: String::new(),
        previous_name: String::new(),
        previous_description: String::new(),
    };

    okay(render_template(template))
}

#[derive(Deserialize)]
struct NewLanguageFormData {
    code: String,
    name: String,
    description: String,
}

async fn new_language_submit(s: Session, languages: LanguageRepository, form: axum::Form<NewLanguageFormData>) -> (StatusCode, Response) {
    let user = get_user!(s);

    match languages.create(&user, CreateLanguage {
        code: form.code.clone(),
        name: form.name.clone(),
        description: form.description.clone(),
        private: false,
    }).await {
        Ok(lang) => (StatusCode::SEE_OTHER, Redirect::to(&format!("/languages/{}", lang.code)).into_response()),
        Err(e) => {
            let template = NewLanguageFormTemplate {
                current_user: Some(user),
                error: Some(e),
                previous_code: form.code.clone(),
                previous_name: form.name.clone(),
                previous_description: form.description.clone(),
            };

            let body = render_template(template);
            (StatusCode::BAD_REQUEST, body)
        }
    }
}

#[derive(Template)]
#[template(path = "languages/view.html")]
struct ViewLanguageTemplate {
    current_user: Option<User>,
    language: Language,
    owner: User,
    contributor_count: i64,
}

async fn view_language(s: Session, languages: LanguageRepository, _users: UserRepository, axum::extract::Path(code): axum::extract::Path<String>) -> (StatusCode, Response) {
    let language = attempt!(s, languages.find_by_code(&code).await);
    let owner = attempt!(s, languages.find_owner(language.id).await);
    let contributor_count = attempt!(s, languages.count_contributors(language.id).await);

    let template = ViewLanguageTemplate {
        current_user: s.user().cloned(),
        language,
        owner,
        contributor_count,
    };

    let body = render_template(template);
    okay(body)
}

#[derive(Template)]
#[template(path = "languages/edit.html")]
#[allow(dead_code)]
struct EditLanguageFormTemplate {
    current_user: Option<User>,
    language: Language,
    error: Option<AppError>,
    previous_code: String,
    previous_name: String,
    previous_description: String,
}

async fn edit_language_form(s: Session, languages: LanguageRepository, axum::extract::Path(code): axum::extract::Path<String>) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&code).await);

    let template = EditLanguageFormTemplate {
        current_user: Some(user),
        language: language.clone(),
        error: None,
        previous_code: language.code,
        previous_name: language.name,
        previous_description: language.description,
    };

    okay(render_template(template))
}

#[derive(Deserialize)]
struct EditLanguageFormData {
    code: String,
    name: String,
    description: String,
}

async fn edit_language_submit(s: Session, languages: LanguageRepository, axum::extract::Path(code): axum::extract::Path<String>, form: axum::Form<EditLanguageFormData>) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&code).await);

    let updates = crate::model::languages::UpdateLanguage {
        code: if form.code != language.code { Some(form.code.clone()) } else { None },
        name: if form.name != language.name { Some(form.name.clone()) } else { None },
        description: if form.description != language.description { Some(form.description.clone()) } else { None },
        private: None,
    };

    match languages.update(&user, language.id, updates).await {
        Ok(lang) => (StatusCode::SEE_OTHER, Redirect::to(&format!("/languages/{}", lang.code)).into_response()),
        Err(e) => {
            let template = EditLanguageFormTemplate {
                current_user: Some(user),
                language: language.clone(),
                error: Some(e),
                previous_code: form.code.clone(),
                previous_name: form.name.clone(),
                previous_description: form.description.clone(),
            };

            let body = render_template(template);
            (StatusCode::BAD_REQUEST, body)
        }
    }
}