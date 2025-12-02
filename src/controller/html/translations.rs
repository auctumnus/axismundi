use std::thread::current;

use askama::Template;
use axum::{
    Form, Router, extract::Path, http::StatusCode, response::{Html, IntoResponse, Redirect, Response}, routing::{get, post}
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    attempt, controller::html::{okay, render_generic_error, render_template}, err::{AppError, bad_request, not_found}, get_user, model::{
        language_invites::PermissionLevel, language_permissions::LanguagePermissionRepository, languages::{Language, LanguageRepository, LanguageSearch}, translatable::{Translatable, TranslatableRepository}, translations::{CreateTranslation, Translation, TranslationRepository, UpdateTranslation}, users::{User, UserRepository}
    }, pagination::PaginatedRequest, util::{AppState, extract_session::Session}
};

pub fn create_router() -> (Router<AppState>, Router<AppState>) {
    let secure_routes = Router::<AppState>::new()
        .route("/translatable/{slug}/new-translation", post(new_translation_submit))
        .route("/translatable/{slug}/edit-translation", post(edit_translation_submit));

    let normal_routes = Router::<AppState>::new()
        .route("/translatable/{slug}/new-translation", get(new_translation_step_1_form))
        .route("/translatable/{slug}/translation/{code}", get(view_translation))
        .route("/translatable/{slug}/edit-translation/{code}", get(edit_translation_form));

    (secure_routes, normal_routes)
}

// Step 1: Select language
#[derive(Template)]
#[template(path = "translations/new-1.html")]
#[allow(dead_code)]
struct NewTranslationStep1Template {
    current_user: Option<User>,
    error: Option<AppError>,
    translatable: Translatable,
    translatable_id: String,
    available_languages: Vec<Language>,
    previous_language_code: String,
    can_edit_translatable: bool,
    language: Option<Language>,
    can_edit_language: bool,
}

async fn new_translation_step_1_form(
    s: Session,
    translatables: TranslatableRepository,
    languages: LanguageRepository,
    Path(slug): Path<String>,
) -> (StatusCode, Response) {
    let user = get_user!(s);

    let translatable = attempt!(s, translatables.find_by_slug(&slug).await);

    let available_languages = attempt!(s, languages.find_all_by_user(user.id).await);

    let can_edit_translatable = translatable.created_by == user.id;

    let template = NewTranslationStep1Template {
        current_user: Some(user),
        error: None,
        translatable: translatable.clone(),
        translatable_id: translatable.slug,
        available_languages,
        previous_language_code: String::new(),
        can_edit_translatable,
        language: None,
        can_edit_language: false,
    };

    okay(render_template(template))
}

// Step 2: Enter translation text
#[derive(Template)]
#[template(path = "translations/new-2.html")]
#[allow(dead_code)]
struct NewTranslationStep2Template {
    current_user: Option<User>,
    error: Option<AppError>,
    translatable: Translatable,
    language: Language,
    previous_translated_text: String,
    can_edit_translatable: bool,
    can_edit_language: bool,
}

#[derive(Deserialize)]
struct NewTranslationFormData {
    step: u8,
    language_code: Option<String>,
    language_id: Option<Uuid>,
    translated_text: Option<String>,
}



async fn new_translation_submit(
    s: Session,
    translatables: TranslatableRepository,
    languages: LanguageRepository,
    translations: TranslationRepository,
    permissions: LanguagePermissionRepository,
    Path(slug): Path<String>,
    Form(form): Form<NewTranslationFormData>,
) -> (StatusCode, Response) {
    let user = get_user!(s);

    let translatable = attempt!(s, translatables
        .find_by_slug(&slug)
        .await);

    let can_edit_translatable = translatable.created_by == user.id;

    if form.step == 1 {
        let Some(language_code) = form.language_code.clone() else {
            let error = bad_request("Language code is required");

            let available_languages = attempt!(s, languages.find_all_by_user(user.id).await);
            let template = NewTranslationStep1Template {
                current_user: Some(user),
                error: Some(error),
                translatable: translatable.clone(),
                translatable_id: translatable.slug,
                available_languages,
                previous_language_code: String::new(),
                can_edit_translatable,
                language: None,
                can_edit_language: false,
            };

            return (StatusCode::BAD_REQUEST, render_template(template))
        };

        let Ok(language) = languages
            .find_by_code(&language_code)
            .await
        else {
            let error = bad_request("Language not found");

            let available_languages = attempt!(s, languages.find_all_by_user(user.id).await);
            let template = NewTranslationStep1Template {
                current_user: Some(user),
                error: Some(error),
                translatable: translatable.clone(),
                translatable_id: translatable.slug,
                available_languages,
                previous_language_code: language_code,
                can_edit_translatable,
                language: None,
                can_edit_language: false,
            };

            return (StatusCode::BAD_REQUEST, render_template(template))
        };

        let can_edit_language = permissions
            .has_permission(user.id, language.id, PermissionLevel::Editor)
            .await
            .unwrap_or(false);

        let template = NewTranslationStep2Template {
            current_user: Some(user),
            error: None,
            translatable,
            language,
            previous_translated_text: String::new(),
            can_edit_translatable,
            can_edit_language,
        };

        okay(render_template(template))
    } else if form.step == 2 {
        let Some(language_id) = form.language_id else {
            return render_generic_error(s, bad_request("Language ID is required")).await;
        };

        let language = attempt!(s, languages
            .find_by_id(language_id)
            .await);

        let can_edit_language = permissions
            .has_permission(user.id, language.id, PermissionLevel::Editor)
            .await
            .unwrap_or(false);

        let Some(translated_text) = form.translated_text.clone() else {
            let error = bad_request("Translated text is required");

            let language = attempt!(s, languages
                .find_by_id(form.language_id.unwrap())
                .await);

            let template = NewTranslationStep2Template {
                current_user: Some(user),
                error: Some(error),
                translatable,
                language,
                previous_translated_text: String::new(),
                can_edit_translatable,
                can_edit_language,
            };

            return (StatusCode::BAD_REQUEST, render_template(template))
        };


        let create_result = translations
            .create(
                &user,
                translatable.id,
                language.id,
                CreateTranslation {
                    translated_text: translated_text.clone(),
                    translator_name: None,
                    translator_url: None,
                    ipa: None,
                    gloss: None,
                    notes: None,
                },
            )
            .await;

        match create_result {
            Ok(_) => {
                let redirect_url = format!(
                    "/translatable/{}/translation/{}",
                    translatable.slug, language.code
                );
                (StatusCode::SEE_OTHER, Redirect::to(&redirect_url).into_response())
            }
            Err(e) => {
                let template = NewTranslationStep2Template {
                    current_user: Some(user),
                    error: Some(e),
                    translatable,
                    language,
                    previous_translated_text: translated_text,
                    can_edit_translatable,
                    can_edit_language,
                };

                (StatusCode::BAD_REQUEST, render_template(template))
            }
        }
    } else {
        let error = bad_request("Invalid form submission");

        let available_languages = attempt!(s, languages.find_all_by_user(user.id).await);
        let template = NewTranslationStep1Template {
            current_user: Some(user),
            error: Some(error),
            translatable: translatable.clone(),
            translatable_id: translatable.slug,
            available_languages,
            previous_language_code: String::new(),
            can_edit_translatable,
            language: None,
            can_edit_language: false,
        };

        (StatusCode::BAD_REQUEST, render_template(template))
    }
}

// View translation
#[derive(Template)]
#[template(path = "translations/view.html")]
struct ViewTranslationTemplate {
    current_user: Option<User>,
    translatable: Translatable,
    translatable_creator: User,
    language: Language,
    translation: Translation,
    translation_creator: User,
    current_user_has_permission: bool,
    can_edit_translatable: bool,
    can_edit_language: bool,
    can_edit_translation: bool,
}

async fn view_translation(
    s: Session,
    translatables: TranslatableRepository,
    languages: LanguageRepository,
    translations: TranslationRepository,
    permissions: LanguagePermissionRepository,
    users: UserRepository,
    Path((slug, code)): Path<(String, String)>,
) -> (StatusCode, Response) {
    let translatable = attempt!(s, translatables.find_by_slug(&slug).await);
    let language = attempt!(s, languages.find_by_code(&code).await);
    let translation = attempt!(s, translations
        .find_by_translatable_and_language(translatable.id, language.id)
        .await);

    let translatable_creator = attempt!(s, users.find_by_id(translatable.created_by).await);
    let translation_creator = attempt!(s, users.find_by_id(translation.created_by).await);

    let current_user_has_permission = if let Some(current_user) = s.user() {
        attempt!(s, permissions
            .find_by_user_and_language(current_user.id, language.id)
            .await).is_some()
    } else {
        false
    };

    let can_edit_translatable = if let Some(current_user) = s.user() {
        translatable.created_by == current_user.id
    } else {
        false
    };

    let can_edit_language = if let Some(current_user) = s.user() {
        permissions
            .has_permission(current_user.id, language.id, PermissionLevel::Editor)
            .await
            .unwrap_or(false)
    } else {
        false
    };

    let can_edit_translation = current_user_has_permission;

    let template = ViewTranslationTemplate {
        current_user: s.user().cloned(),
        translatable,
        translatable_creator,
        language,
        translation,
        translation_creator,
        current_user_has_permission,
        can_edit_translatable,
        can_edit_language,
        can_edit_translation,
    };

    let body = render_template(template);
    okay(body)
}

// Edit translation
#[derive(Template)]
#[template(path = "translations/edit.html")]
#[allow(dead_code)]
struct EditTranslationTemplate {
    current_user: Option<User>,
    error: Option<AppError>,
    translatable: Translatable,
    language: Language,
    previous_translated_text: String,
    can_edit_translatable: bool,
    can_edit_language: bool,
    can_edit_translation: bool,
}

async fn edit_translation_form(
    s: Session,
    translatables: TranslatableRepository,
    languages: LanguageRepository,
    translations: TranslationRepository,
    permissions: LanguagePermissionRepository,
    Path((slug, code)): Path<(String, String)>,
) -> (StatusCode, Response) {
    let user = get_user!(s);

    let translatable = attempt!(s, translatables.find_by_slug(&slug).await);

    let language = attempt!(s, languages.find_by_code(&code).await);

    let translation = attempt!(s, translations
        .find_by_translatable_and_language(translatable.id, language.id)
        .await);

    let can_edit_translatable = translatable.created_by == user.id;

    let can_edit_language = permissions
        .has_permission(user.id, language.id, PermissionLevel::Editor)
        .await
        .unwrap_or(false);

    let can_edit_translation = permissions
        .find_by_user_and_language(user.id, language.id)
        .await
        .ok()
        .flatten()
        .is_some();

    let template = EditTranslationTemplate {
        current_user: Some(user),
        error: None,
        translatable,
        language,
        previous_translated_text: translation.translated_text.clone(),
        can_edit_translatable,
        can_edit_language,
        can_edit_translation,
    };

    okay(render_template(template))
}

#[derive(Deserialize)]
struct EditTranslationFormData {
    translated_text: String,
    language_id: Uuid,
}

async fn edit_translation_submit(
    s: Session,
    translatables: TranslatableRepository,
    languages: LanguageRepository,
    translations: TranslationRepository,
    permissions: LanguagePermissionRepository,
    Path(slug): Path<String>,
    Form(form): Form<EditTranslationFormData>,
) -> (StatusCode, Response) {
    let user = get_user!(s);

    let translatable = attempt!(s, translatables
        .find_by_slug(&slug)
        .await);

    let language = attempt!(s, languages
        .find_by_id(form.language_id)
        .await);

    let can_edit_translatable = translatable.created_by == user.id;

    let can_edit_language = permissions
        .has_permission(user.id, language.id, PermissionLevel::Editor)
        .await
        .unwrap_or(false);

    let can_edit_translation = permissions
        .find_by_user_and_language(user.id, language.id)
        .await
        .ok()
        .flatten()
        .is_some();

    let existing_translation = attempt!(s, translations
        .find_by_translatable_and_language(translatable.id, language.id)
        .await);
    let translation = translations
        .update(
            &user,
            existing_translation.id,
            UpdateTranslation {
                translated_text: Some(form.translated_text.clone()),
                translator_name: None,
                translator_url: None,
                ipa: None,
                gloss: None,
                notes: None,
            },
        )
        .await;

    match translation {
        Ok(_) => {
            let redirect_url = format!(
                "/translatable/{}/translation/{}",
                translatable.slug, language.code
            );
            (StatusCode::SEE_OTHER, Redirect::to(&redirect_url).into_response())
        }
        Err(e) => {
            let template = EditTranslationTemplate {
                current_user: Some(user),
                error: Some(e),
                translatable: translatable.clone(),
                language,
                previous_translated_text: form.translated_text.clone(),
                can_edit_translatable,
                can_edit_language,
                can_edit_translation,
            };
            let body = render_template(template);
            (StatusCode::BAD_REQUEST, body)
        }

    }
}
