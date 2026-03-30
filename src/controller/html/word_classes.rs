use askama::Template;
use axum::{
    Router,
    extract::Path,
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use axum_extra::{TypedHeader, headers::UserAgent};
use reqwest::StatusCode;
use serde::Deserialize;

use crate::{
    attempt,
    controller::html::{okay, render_generic_error, render_template},
    embed::{EmbedTarget, GenericEmbed, render_embed, truncate_description},
    err::AppError,
    get_user,
    model::{
        language_invites::PermissionLevel,
        language_permissions::LanguagePermissionRepository,
        languages::{Language, LanguageRepository},
        users::{User, UserRepository},
        word_classes::{CreateWordClass, UpdateWordClass, WordClass, WordClassRepository},
    },
    util::{AppState, extract_session::Session},
};

pub fn create_router() -> (Router<AppState>, Router<AppState>) {
    let secure_routes = Router::<AppState>::new()
        .route(
            "/languages/{code}/new-word-class",
            post(new_word_class_submit),
        )
        .route(
            "/languages/{code}/word-classes/{abbreviation}/edit",
            post(edit_word_class_submit),
        )
        .route(
            "/languages/{code}/word-classes/{abbreviation}/delete",
            post(delete_word_class_submit),
        );

    let normal_routes = Router::<AppState>::new()
        .route("/languages/{code}/word-classes", get(list_word_classes))
        .route("/languages/{code}/new-word-class", get(new_word_class_form))
        .route(
            "/languages/{code}/word-classes/{abbreviation}",
            get(view_word_class),
        )
        .route(
            "/languages/{code}/word-classes/{abbreviation}/edit",
            get(edit_word_class_form),
        )
        .route(
            "/languages/{code}/word-classes/{abbreviation}/delete",
            get(delete_word_class_form),
        );

    (secure_routes, normal_routes)
}

struct WordClassWithCreator {
    word_class: WordClass,
    creator: User,
}

#[derive(Template)]
#[template(path = "word_classes/list.html")]
struct ListWordClassesTemplate {
    current_user: Option<User>,
    language: Language,
    word_classes: Vec<WordClassWithCreator>,
    user_has_permission: bool,
    can_edit_language: bool,
}

async fn list_word_classes(
    s: Session,
    languages: LanguageRepository,
    word_classes: WordClassRepository,
    permissions: LanguagePermissionRepository,
    users: UserRepository,
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

    let mut classes_with_creators = Vec::with_capacity(classes.len());
    for wc in classes {
        let creator = attempt!(s, users.find_by_id(wc.created_by).await);
        classes_with_creators.push(WordClassWithCreator {
            word_class: wc,
            creator,
        });
    }

    let template = ListWordClassesTemplate {
        current_user: s.user().cloned(),
        language,
        word_classes: classes_with_creators,
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
    previous_notes: String,
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
        previous_notes: String::new(),
        user_has_permission,
        can_edit_language: user_has_permission,
    };

    okay(render_template(template))
}

#[derive(Deserialize)]
struct NewWordClassFormData {
    name: String,
    abbreviation: String,
    notes: String,
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

    // Treat empty notes as None
    let notes = if form.notes.trim().is_empty() {
        None
    } else {
        Some(form.notes.clone())
    };

    match word_classes
        .create(
            &user,
            &code,
            CreateWordClass {
                name: form.name.clone(),
                abbreviation: form.abbreviation.clone(),
                notes,
            },
        )
        .await
    {
        Ok(_) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!("/languages/{}/word-classes", code)).into_response(),
        ),
        Err(e) => {
            let template = NewWordClassFormTemplate {
                current_user: Some(user),
                language,
                error: Some(e),
                previous_name: form.name.clone(),
                previous_abbreviation: form.abbreviation.clone(),
                previous_notes: form.notes.clone(),
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
    rendered_notes: String,
    creator: User,
    #[allow(dead_code)]
    user_has_permission: bool,
    can_edit_language: bool,
    json_ld: String,
}

async fn view_word_class(
    s: Session,
    user_agent: Option<TypedHeader<UserAgent>>,
    languages: LanguageRepository,
    word_classes: WordClassRepository,
    permissions: LanguagePermissionRepository,
    users: UserRepository,
    Path((code, abbreviation)): Path<(String, String)>,
) -> (StatusCode, Response) {
    let language = attempt!(s, languages.find_by_code(&code).await);
    let word_class = attempt!(
        s,
        word_classes
            .find_by_abbreviation(&language.id, &abbreviation)
            .await
    );
    let rendered_notes = attempt!(s, WordClassRepository::render_notes(&word_class));
    let creator = attempt!(s, users.find_by_id(word_class.created_by).await);

    if let Some(ua) = user_agent
        && ua.as_str().to_lowercase().contains("discordbot")
    {
        let description = if word_class.notes.is_empty() {
            String::new()
        } else {
            truncate_description(&word_class.notes)
        };
        return okay(
            render_embed(
                EmbedTarget::Discord,
                GenericEmbed {
                    title: format!("{} ({}.)", word_class.name, word_class.abbreviation),
                    description: format!(
                        "{language_name} word class\n\n{description}",
                        language_name = language.name
                    ),
                    author: Some(creator),
                    color: None,
                    url: format!(
                        "{}/languages/{}/word-classes/{}",
                        &crate::CONFIG.public_url_base,
                        language.code,
                        word_class.abbreviation
                    ),
                    image: None,
                },
            )
            .await
            .into_response(),
        );
    }

    let user_has_permission = if let Some(user) = s.user() {
        permissions
            .has_permission(user.id, language.id, PermissionLevel::Editor)
            .await
            .unwrap_or(false)
    } else {
        false
    };

    let json_ld = attempt!(
        s,
        serde_json::to_string(&attempt!(
            s,
            word_classes.as_json_ld(&word_class, &language).await
        ))
        .map_err(Into::into)
    );

    let template = ViewWordClassTemplate {
        current_user: s.user().cloned(),
        language,
        word_class,
        rendered_notes,
        creator,
        user_has_permission,
        can_edit_language: user_has_permission,
        json_ld,
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
    previous_notes: String,
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
    let word_class = attempt!(
        s,
        word_classes
            .find_by_abbreviation(&language.id, &abbreviation)
            .await
    );

    let user_has_permission = permissions
        .has_permission(user.id, language.id, PermissionLevel::Editor)
        .await
        .unwrap_or(false);

    let template = EditWordClassFormTemplate {
        current_user: Some(user),
        language,
        previous_name: word_class.name.clone(),
        previous_abbreviation: word_class.abbreviation.clone(),
        previous_notes: word_class.notes.clone(),
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
    notes: String,
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
    let word_class = attempt!(
        s,
        word_classes
            .find_by_abbreviation(&language.id, &abbreviation)
            .await
    );

    let user_has_permission = permissions
        .has_permission(user.id, language.id, PermissionLevel::Editor)
        .await
        .unwrap_or(false);

    // Treat empty notes as None for comparison
    let form_notes = if form.notes.trim().is_empty() {
        None
    } else {
        Some(form.notes.clone())
    };

    let updates = UpdateWordClass {
        name: if form.name == word_class.name {
            None
        } else {
            Some(form.name.clone())
        },
        abbreviation: if form.abbreviation == word_class.abbreviation {
            None
        } else {
            Some(form.abbreviation.clone())
        },
        notes: if form_notes.as_deref().unwrap_or("") == word_class.notes {
            None
        } else {
            form_notes
        },
    };

    match word_classes.update(&user, word_class.id, updates).await {
        Ok(updated_wc) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!(
                "/languages/{}/word-classes/{}",
                code, updated_wc.abbreviation
            ))
            .into_response(),
        ),
        Err(e) => {
            let template = EditWordClassFormTemplate {
                current_user: Some(user),
                language,
                word_class: word_class.clone(),
                error: Some(e),
                previous_name: form.name.clone(),
                previous_abbreviation: form.abbreviation.clone(),
                previous_notes: form.notes.clone(),
                user_has_permission,
                can_edit_language: user_has_permission,
            };

            let body = render_template(template);
            (StatusCode::BAD_REQUEST, body)
        }
    }
}

#[derive(Template)]
#[template(path = "word_classes/delete.html")]
struct DeleteWordClassTemplate {
    current_user: Option<User>,
    language: Language,
    word_class: WordClass,
    can_edit_language: bool,
}

async fn delete_word_class_form(
    s: Session,
    languages: LanguageRepository,
    word_classes: WordClassRepository,
    permissions: LanguagePermissionRepository,
    Path((code, abbreviation)): Path<(String, String)>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&code).await);
    let word_class = attempt!(
        s,
        word_classes
            .find_by_abbreviation(&language.id, &abbreviation)
            .await
    );

    let user_has_permission = permissions
        .has_permission(user.id, language.id, PermissionLevel::Editor)
        .await
        .unwrap_or(false);

    let template = DeleteWordClassTemplate {
        current_user: Some(user),
        language,
        word_class,
        can_edit_language: user_has_permission,
    };

    okay(render_template(template))
}

async fn delete_word_class_submit(
    s: Session,
    languages: LanguageRepository,
    word_classes: WordClassRepository,
    Path((code, abbreviation)): Path<(String, String)>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&code).await);
    let word_class = attempt!(
        s,
        word_classes
            .find_by_abbreviation(&language.id, &abbreviation)
            .await
    );

    match word_classes.delete(&user, word_class.id).await {
        Ok(_) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!("/languages/{}/word-classes", code)).into_response(),
        ),
        Err(e) => render_generic_error(s, e).await,
    }
}
