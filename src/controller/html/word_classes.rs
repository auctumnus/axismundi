use askama::Template;
use axum::{Router, extract::Path, response::{Html, IntoResponse, Redirect, Response}, routing::{get, post}};
use reqwest::StatusCode;
use serde::Deserialize;

use crate::{
    attempt, controller::html::{okay, render_generic_error, render_template}, err::AppError, get_user, model::{
        language_invites::PermissionLevel,
        language_permissions::LanguagePermissionRepository,
        languages::{Language, LanguageRepository},
        users::User,
        word_classes::{CreateWordClass, UpdateWordClass, WordClass, WordClassRepository},
    },
    util::{AppState, extract_session::Session},
};

pub fn create_router() -> (Router<AppState>, Router<AppState>) {
    let secure_routes = Router::<AppState>::new()
        .route("/languages/{code}/new-word-class", post(new_word_class_submit))
        .route("/languages/{code}/word-classes/{abbreviation}/edit", post(edit_word_class_submit))
        .route("/languages/{code}/word-classes/{abbreviation}/delete", post(delete_word_class_submit));

    let normal_routes = Router::<AppState>::new()
        .route("/languages/{code}/word-classes", get(list_word_classes))
        .route("/languages/{code}/new-word-class", get(new_word_class_form))
        .route("/languages/{code}/word-classes/{abbreviation}", get(view_word_class))
        .route("/languages/{code}/word-classes/{abbreviation}/edit", get(edit_word_class_form));

    (secure_routes, normal_routes)
}

#[derive(Template)]
#[template(path = "word_classes/list.html")]
struct ListWordClassesTemplate {
    current_user: Option<User>,
    language: Language,
    word_classes: Vec<WordClass>,
    user_has_permission: bool,
    can_edit_language: bool,
}

async fn list_word_classes(
    s: Session,
    languages: LanguageRepository,
    word_classes: WordClassRepository,
    permissions: LanguagePermissionRepository,
    Path(code): Path<String>,
) -> (StatusCode, Response) {
    let language = attempt!(s, languages.find_by_code(&code).await);
    let classes = attempt!(s, word_classes.list_all(language.id).await);

    let user_has_permission = if let Some(user) = s.user() {
        permissions
            .has_permission(user.id, language.id, PermissionLevel::Editor)
            .await
            .unwrap_or(false)
    } else {
        false
    };

    let template = ListWordClassesTemplate {
        current_user: s.user().cloned(),
        language,
        word_classes: classes,
        user_has_permission,
        can_edit_language: user_has_permission,
    };

    let body = render_template(template);
    okay(body)
}

#[derive(Template)]
#[template(path = "word_classes/new.html")]
#[allow(dead_code)]
struct NewWordClassFormTemplate {
    current_user: Option<User>,
    language: Language,
    error: Option<AppError>,
    previous_name: String,
    previous_abbreviation: String,
    user_has_permission: bool,
    can_edit_language: bool,
}

async fn new_word_class_form(
    s: Session,
    languages: LanguageRepository,
    permissions: LanguagePermissionRepository,
    Path(code): Path<String>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&code).await);

    let user_has_permission = permissions
        .has_permission(user.id, language.id, PermissionLevel::Editor)
        .await
        .unwrap_or(false);

    let template = NewWordClassFormTemplate {
        current_user: Some(user),
        language,
        error: None,
        previous_name: String::new(),
        previous_abbreviation: String::new(),
        user_has_permission,
        can_edit_language: user_has_permission,
    };

    okay(render_template(template))
}

#[derive(Deserialize)]
struct NewWordClassFormData {
    name: String,
    abbreviation: String,
}

async fn new_word_class_submit(
    s: Session,
    languages: LanguageRepository,
    word_classes: WordClassRepository,
    permissions: LanguagePermissionRepository,
    Path(code): Path<String>,
    form: axum::Form<NewWordClassFormData>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&code).await);

    let user_has_permission = permissions
        .has_permission(user.id, language.id, PermissionLevel::Editor)
        .await
        .unwrap_or(false);

    match word_classes
        .create(
            &user,
            &code,
            CreateWordClass {
                name: form.name.clone(),
                abbreviation: form.abbreviation.clone(),
            },
        )
        .await
    {
        Ok(_) => (StatusCode::SEE_OTHER, Redirect::to(&format!("/languages/{}/word-classes", code)).into_response()),
        Err(e) => {
            let template = NewWordClassFormTemplate {
                current_user: Some(user),
                language,
                error: Some(e),
                previous_name: form.name.clone(),
                previous_abbreviation: form.abbreviation.clone(),
                user_has_permission,
                can_edit_language: user_has_permission,
            };

            let body = render_template(template);
            (StatusCode::BAD_REQUEST, body)
        }
    }
}

#[derive(Template)]
#[template(path = "word_classes/view.html")]
struct ViewWordClassTemplate {
    current_user: Option<User>,
    language: Language,
    word_class: WordClass,
    user_has_permission: bool,
    can_edit_language: bool,
}

async fn view_word_class(
    s: Session,
    languages: LanguageRepository,
    word_classes: WordClassRepository,
    permissions: LanguagePermissionRepository,
    Path((code, abbreviation)): Path<(String, String)>,
) -> (StatusCode, Response) {
    let language = attempt!(s, languages.find_by_code(&code).await);
    let word_class = attempt!(s, word_classes.find_by_abbreviation(&language.id, &abbreviation).await);

    let user_has_permission = if let Some(user) = s.user() {
        permissions
            .has_permission(user.id, language.id, PermissionLevel::Editor)
            .await
            .unwrap_or(false)
    } else {
        false
    };

    let template = ViewWordClassTemplate {
        current_user: s.user().cloned(),
        language,
        word_class,
        user_has_permission,
        can_edit_language: user_has_permission,
    };

    let body = render_template(template);
    okay(body)
}

#[derive(Template)]
#[template(path = "word_classes/edit.html")]
#[allow(dead_code)]
struct EditWordClassFormTemplate {
    current_user: Option<User>,
    language: Language,
    word_class: WordClass,
    error: Option<AppError>,
    previous_name: String,
    previous_abbreviation: String,
    user_has_permission: bool,
    can_edit_language: bool,
}

async fn edit_word_class_form(
    s: Session,
    languages: LanguageRepository,
    word_classes: WordClassRepository,
    permissions: LanguagePermissionRepository,
    Path((code, abbreviation)): Path<(String, String)>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&code).await);
    let word_class = attempt!(s, word_classes
        .find_by_abbreviation(&language.id, &abbreviation)
        .await);

    let user_has_permission = permissions
        .has_permission(user.id, language.id, PermissionLevel::Editor)
        .await
        .unwrap_or(false);

    let template = EditWordClassFormTemplate {
        current_user: Some(user),
        language,
        previous_name: word_class.name.clone(),
        previous_abbreviation: word_class.abbreviation.clone(),
        word_class,
        error: None,
        user_has_permission,
        can_edit_language: user_has_permission,
    };

    okay(render_template(template))
}

#[derive(Deserialize)]
struct EditWordClassFormData {
    name: String,
    abbreviation: String,
}

async fn edit_word_class_submit(
    s: Session,
    languages: LanguageRepository,
    word_classes: WordClassRepository,
    permissions: LanguagePermissionRepository,
    Path((code, abbreviation)): Path<(String, String)>,
    form: axum::Form<EditWordClassFormData>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&code).await);
    let word_class = attempt!(s, word_classes
        .find_by_abbreviation(&language.id, &abbreviation)
        .await);

    let user_has_permission = permissions
        .has_permission(user.id, language.id, PermissionLevel::Editor)
        .await
        .unwrap_or(false);

    let updates = UpdateWordClass {
        name: if form.name != word_class.name {
            Some(form.name.clone())
        } else {
            None
        },
        abbreviation: if form.abbreviation != word_class.abbreviation {
            Some(form.abbreviation.clone())
        } else {
            None
        },
    };

    match word_classes
        .update(&user, word_class.id, updates)
        .await
    {
        Ok(updated) => (StatusCode::SEE_OTHER, Redirect::to(&format!("/languages/{}/word-classes/{}", code, updated.abbreviation)).into_response()),
        Err(e) => {
            let template = EditWordClassFormTemplate {
                current_user: Some(user),
                language,
                word_class: word_class.clone(),
                error: Some(e),
                previous_name: form.name.clone(),
                previous_abbreviation: form.abbreviation.clone(),
                user_has_permission,
                can_edit_language: user_has_permission,
            };

            let body = render_template(template);
            (StatusCode::BAD_REQUEST, body)
        }
    }
}

async fn delete_word_class_submit(
    s: Session,
    languages: LanguageRepository,
    word_classes: WordClassRepository,
    Path((code, abbreviation)): Path<(String, String)>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&code).await);
    let word_class = attempt!(s, word_classes
        .find_by_abbreviation(&language.id, &abbreviation)
        .await);

    match word_classes
        .delete(&user, word_class.id)
        .await
    {
        Ok(_) => (StatusCode::SEE_OTHER, Redirect::to(&format!("/languages/{}/word-classes", code)).into_response()),
        Err(e) => render_generic_error(s, e).await
    }
}
