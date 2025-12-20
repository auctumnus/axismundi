use std::default;

use askama::Template;
use axum::{Router, response::{Html, IntoResponse, Redirect, Response}, routing::{get, post}};
use reqwest::StatusCode;
use serde::Deserialize;

use crate::{attempt, controller::html::{okay, render_template}, err::AppError, get_user, md::render_md, model::{definitions::{Definition, DefinitionRepository}, language_invites::PermissionLevel, language_permissions::LanguagePermissionRepository, languages::{CreateLanguage, Language, LanguageRepository}, translatable::TranslatableRepository, translations::TranslationRepository, users::{User, UserRepository}, words::{Word, WordRepository, WordSearch}}, pagination::PaginatedRequest, util::{AppState, extract_session::Session}};

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

struct WordWithMeta {
    word: Word,
    first_definition: Option<Definition>,
    creator: User,
}

struct TranslationWithAuthor {
    translation: crate::model::translations::Translation,
    translatable: crate::model::translatable::Translatable,
    author: User,
}

#[derive(Template)]
#[template(path = "languages/view.html")]
struct ViewLanguageTemplate {
    current_user: Option<User>,
    recent_words: Vec<WordWithMeta>,
    recent_translations: Vec<TranslationWithAuthor>,
    language: Language,
    owner: User,
    contributor_count: i64,
    rendered_description: String,
    can_edit_language: bool,
    can_delete_language: bool,
    is_liked: bool,
}

#[axum::debug_handler(state=AppState)]
async fn view_language(s: Session, languages: LanguageRepository, definitions: DefinitionRepository, users: UserRepository, words: WordRepository, translations: TranslationRepository, translatables: TranslatableRepository, permissions: LanguagePermissionRepository, axum::extract::Path(code): axum::extract::Path<String>) -> (StatusCode, Response) {
    let language = attempt!(s, languages.find_by_code(&code).await);
    let owner = attempt!(s, languages.find_owner(language.id).await);
    let contributor_count = attempt!(s, languages.count_contributors(language.id).await);
    let rendered_description = attempt!(s, languages.render_description(&language).await);
    let recent_words = attempt!(s, words.search(&language.id, PaginatedRequest {
        limit: 5,
        offset: 0,
    }, WordSearch {
        ..Default::default()
    }).await);

    let recent_translations = attempt!(s, translations.list_by_language(language.id, PaginatedRequest {
        limit: 5,
        offset: 0,
    }).await);

    let can_edit_language = if let Some(user) = s.user() {
        permissions
            .has_permission(user.id, language.id, PermissionLevel::Editor)
            .await
            .unwrap_or(false)
    } else {
        false
    };

    let can_delete_language = if let Some(user) = s.user() {
        permissions
            .has_permission(user.id, language.id, PermissionLevel::Owner)
            .await
            .unwrap_or(false)
    } else {
        false
    };

    let is_liked = if let Some(user) = s.user() {
        languages.is_liked(&language.id, &user.id).await.unwrap_or(false)
    } else {
        false
    };

    // Fetch authors for each word
    let mut words_with_meta = Vec::new();
    for word in recent_words.items {
        let creator = attempt!(s, words.find_creator(&word.id).await);
        let first_definition = attempt!(s, definitions.get_first_by_word(&word.id).await);
        words_with_meta.push(WordWithMeta { word, first_definition, creator });
    }

    // Fetch authors and translatables for each translation
    let mut translations_with_authors = Vec::new();
    for translation in recent_translations.items {
        let author = attempt!(s, users.find_by_id(translation.created_by).await);
        let translatable = attempt!(s, translatables.find_by_id(translation.translatable).await);
        translations_with_authors.push(TranslationWithAuthor {
            translation,
            translatable,
            author
        });
    }

    let template = ViewLanguageTemplate {
        current_user: s.user().cloned(),
        recent_words: words_with_meta,
        recent_translations: translations_with_authors,
        language,
        owner,
        contributor_count,
        rendered_description,
        can_edit_language,
        can_delete_language,
        is_liked,
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
    can_edit_language: bool,
    can_delete_language: bool,
}

async fn edit_language_form(s: Session, languages: LanguageRepository, permissions: LanguagePermissionRepository, axum::extract::Path(code): axum::extract::Path<String>) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&code).await);

    let can_edit_language = permissions
        .has_permission(user.id, language.id, PermissionLevel::Editor)
        .await
        .unwrap_or(false);

    let can_delete_language = permissions
        .has_permission(user.id, language.id, PermissionLevel::Owner)
        .await
        .unwrap_or(false);

    let template = EditLanguageFormTemplate {
        current_user: Some(user),
        language: language.clone(),
        error: None,
        previous_code: language.code,
        previous_name: language.name,
        previous_description: language.description,
        can_edit_language,
        can_delete_language,
    };

    okay(render_template(template))
}

#[derive(Deserialize)]
struct EditLanguageFormData {
    code: String,
    name: String,
    description: String,
}

async fn edit_language_submit(s: Session, languages: LanguageRepository, permissions: LanguagePermissionRepository, axum::extract::Path(code): axum::extract::Path<String>, form: axum::Form<EditLanguageFormData>) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&code).await);

    let can_edit_language = permissions
        .has_permission(user.id, language.id, PermissionLevel::Editor)
        .await
        .unwrap_or(false);

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
                can_delete_language: permissions
                    .has_permission(user.id, language.id, PermissionLevel::Owner)
                    .await
                    .unwrap_or(false),
                current_user: Some(user),
                language: language.clone(),
                error: Some(e),
                previous_code: form.code.clone(),
                previous_name: form.name.clone(),
                previous_description: form.description.clone(),
                can_edit_language,
            };

            let body = render_template(template);
            (StatusCode::BAD_REQUEST, body)
        }
    }
}