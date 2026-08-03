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
        word_categories::{
            CreateWordCategory, UpdateWordCategory, WordCategory, WordCategoryRepository,
        },
        words::{WordRepository, WordSearch, WordWithMeta},
    },
    pagination::PaginatedRequest,
    util::{AppState, extract_session::Session},
};

pub fn create_router() -> (Router<AppState>, Router<AppState>) {
    let secure_routes = Router::<AppState>::new()
        .route(
            "/languages/{code}/new-word-category",
            post(new_word_category_submit),
        )
        .route(
            "/languages/{code}/word-categories/{abbreviation}/edit",
            post(edit_word_category_submit),
        )
        .route(
            "/languages/{code}/word-categories/{abbreviation}/delete",
            post(delete_word_category_submit),
        );

    let normal_routes = Router::<AppState>::new()
        .route(
            "/languages/{code}/word-categories",
            get(list_word_categories),
        )
        .route(
            "/languages/{code}/new-word-category",
            get(new_word_category_form),
        )
        .route(
            "/languages/{code}/word-categories/{abbreviation}",
            get(view_word_category),
        )
        .route(
            "/languages/{code}/word-categories/{abbreviation}/edit",
            get(edit_word_category_form),
        )
        .route(
            "/languages/{code}/word-categories/{abbreviation}/delete",
            get(delete_word_category_form),
        );

    (secure_routes, normal_routes)
}

struct WordCategoryWithCreator {
    word_category: WordCategory,
    creator: User,
}

#[derive(Template)]
#[template(path = "word_categories/list.html")]
struct ListWordCategoriesTemplate {
    current_user: Option<User>,
    language: Language,
    word_categories: Vec<WordCategoryWithCreator>,
    user_has_permission: bool,
    can_edit_language: bool,
}

async fn list_word_categories(
    s: Session,
    languages: LanguageRepository,
    word_categories: WordCategoryRepository,
    permissions: LanguagePermissionRepository,
    users: UserRepository,
    Path(code): Path<String>,
) -> (StatusCode, Response) {
    let language = attempt!(s, languages.find_by_code(&code).await);
    let categories = attempt!(s, word_categories.list_all(language.id).await);

    let user_has_permission = if let Some(user) = s.user() {
        permissions
            .has_permission(user.id, language.id, PermissionLevel::Editor)
            .await
            .unwrap_or(false)
    } else {
        false
    };

    let mut categories_with_creators = Vec::with_capacity(categories.len());
    for wc in categories {
        let creator = attempt!(s, users.find_by_id(wc.created_by).await);
        categories_with_creators.push(WordCategoryWithCreator {
            word_category: wc,
            creator,
        });
    }

    let template = ListWordCategoriesTemplate {
        current_user: s.user().cloned(),
        language,
        word_categories: categories_with_creators,
        user_has_permission,
        can_edit_language: user_has_permission,
    };

    let body = render_template(template);
    okay(body)
}

#[derive(Template)]
#[template(path = "word_categories/new.html")]
#[allow(dead_code)]
struct NewWordCategoryFormTemplate {
    current_user: Option<User>,
    language: Language,
    error: Option<AppError>,
    previous_name: String,
    previous_abbreviation: String,
    previous_notes: String,
    user_has_permission: bool,
    can_edit_language: bool,
}

async fn new_word_category_form(
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

    let template = NewWordCategoryFormTemplate {
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
struct NewWordCategoryFormData {
    name: String,
    abbreviation: String,
    notes: String,
}

async fn new_word_category_submit(
    s: Session,
    languages: LanguageRepository,
    word_categories: WordCategoryRepository,
    permissions: LanguagePermissionRepository,
    Path(code): Path<String>,
    form: axum::Form<NewWordCategoryFormData>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&code).await);

    let user_has_permission = permissions
        .has_permission(user.id, language.id, PermissionLevel::Editor)
        .await
        .unwrap_or(false);

    let notes = form.notes.trim().to_string();

    match word_categories
        .create(
            &user,
            &code,
            CreateWordCategory {
                name: form.name.clone(),
                abbreviation: form.abbreviation.clone(),
                notes: Some(notes),
            },
        )
        .await
    {
        Ok(_) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!("/languages/{}/word-categories", code)).into_response(),
        ),
        Err(e) => {
            let template = NewWordCategoryFormTemplate {
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
#[template(path = "word_categories/view.html")]
struct ViewWordCategoryTemplate {
    current_user: Option<User>,
    language: Language,
    word_category: WordCategory,
    rendered_notes: String,
    creator: User,
    user_has_permission: bool,
    can_edit_language: bool,
    json_ld: String,
    recent_words: Vec<WordWithMeta>,
}

async fn view_word_category(
    s: Session,
    user_agent: Option<TypedHeader<UserAgent>>,
    languages: LanguageRepository,
    word_categories: WordCategoryRepository,
    permissions: LanguagePermissionRepository,
    users: UserRepository,
    words: WordRepository,
    Path((code, abbreviation)): Path<(String, String)>,
) -> (StatusCode, Response) {
    let language = attempt!(s, languages.find_by_code(&code).await);
    let word_category = attempt!(
        s,
        word_categories
            .find_by_abbreviation(&language.id, &abbreviation)
            .await
    );
    let rendered_notes = attempt!(s, WordCategoryRepository::render_notes(&word_category));
    let creator = attempt!(s, users.find_by_id(word_category.created_by).await);

    if let Some(ua) = user_agent
        && ua.as_str().to_lowercase().contains("discordbot")
    {
        let description = if word_category.notes.is_empty() {
            String::new()
        } else {
            truncate_description(&word_category.notes)
        };
        return okay(
            render_embed(
                EmbedTarget::Discord,
                GenericEmbed {
                    title: format!("{} ({}.)", word_category.name, word_category.abbreviation),
                    description: format!(
                        "{language_name} word category\n\n{description}",
                        language_name = language.name
                    ),
                    author: Some(creator),
                    color: None,
                    url: format!(
                        "{}/languages/{}/word-categories/{}",
                        &crate::CONFIG.public_url_base,
                        language.code,
                        word_category.abbreviation
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
            word_categories.as_json_ld(&word_category, &language).await
        ))
        .map_err(Into::into)
    );

    let recent_words_page = attempt!(
        s,
        words
            .search(
                &language.id,
                PaginatedRequest {
                    limit: 5,
                    offset: 0,
                },
                WordSearch {
                    categories: vec![word_category.abbreviation.clone()],
                    ..Default::default()
                },
            )
            .await
    );

    let mut recent_words = Vec::with_capacity(recent_words_page.items.len());
    for word in recent_words_page.items {
        recent_words.push(attempt!(s, words.materialize(word, s.user()).await));
    }

    let template = ViewWordCategoryTemplate {
        current_user: s.user().cloned(),
        language,
        word_category,
        rendered_notes,
        creator,
        user_has_permission,
        can_edit_language: user_has_permission,
        json_ld,
        recent_words,
    };

    let body = render_template(template);
    okay(body)
}

#[derive(Template)]
#[template(path = "word_categories/edit.html")]
#[allow(dead_code)]
struct EditWordCategoryFormTemplate {
    current_user: Option<User>,
    language: Language,
    word_category: WordCategory,
    error: Option<AppError>,
    previous_name: String,
    previous_abbreviation: String,
    previous_notes: String,
    user_has_permission: bool,
    can_edit_language: bool,
}

async fn edit_word_category_form(
    s: Session,
    languages: LanguageRepository,
    word_categories: WordCategoryRepository,
    permissions: LanguagePermissionRepository,
    Path((code, abbreviation)): Path<(String, String)>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&code).await);
    let word_category = attempt!(
        s,
        word_categories
            .find_by_abbreviation(&language.id, &abbreviation)
            .await
    );

    let user_has_permission = permissions
        .has_permission(user.id, language.id, PermissionLevel::Editor)
        .await
        .unwrap_or(false);

    let template = EditWordCategoryFormTemplate {
        current_user: Some(user),
        language,
        previous_name: word_category.name.clone(),
        previous_abbreviation: word_category.abbreviation.clone(),
        previous_notes: word_category.notes.clone(),
        word_category,
        error: None,
        user_has_permission,
        can_edit_language: user_has_permission,
    };

    okay(render_template(template))
}

#[derive(Deserialize)]
struct EditWordCategoryFormData {
    name: String,
    abbreviation: String,
    notes: String,
}

async fn edit_word_category_submit(
    s: Session,
    languages: LanguageRepository,
    word_categories: WordCategoryRepository,
    permissions: LanguagePermissionRepository,
    Path((code, abbreviation)): Path<(String, String)>,
    form: axum::Form<EditWordCategoryFormData>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&code).await);
    let word_category = attempt!(
        s,
        word_categories
            .find_by_abbreviation(&language.id, &abbreviation)
            .await
    );

    let user_has_permission = permissions
        .has_permission(user.id, language.id, PermissionLevel::Editor)
        .await
        .unwrap_or(false);

    // An empty textarea is the empty string, not None: None means "leave unchanged",
    // so folding empty to None would make clearing the notes impossible.
    let form_notes = form.notes.trim().to_string();

    let updates = UpdateWordCategory {
        name: if form.name == word_category.name {
            None
        } else {
            Some(form.name.clone())
        },
        abbreviation: if form.abbreviation == word_category.abbreviation {
            None
        } else {
            Some(form.abbreviation.clone())
        },
        notes: if form_notes == word_category.notes {
            None
        } else {
            Some(form_notes)
        },
    };

    match word_categories
        .update(&user, word_category.id, updates)
        .await
    {
        Ok(updated_wc) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!(
                "/languages/{}/word-categories/{}",
                code, updated_wc.abbreviation
            ))
            .into_response(),
        ),
        Err(e) => {
            let template = EditWordCategoryFormTemplate {
                current_user: Some(user),
                language,
                word_category: word_category.clone(),
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
#[template(path = "word_categories/delete.html")]
struct DeleteWordCategoryTemplate {
    current_user: Option<User>,
    language: Language,
    word_category: WordCategory,
    can_edit_language: bool,
}

async fn delete_word_category_form(
    s: Session,
    languages: LanguageRepository,
    word_categories: WordCategoryRepository,
    permissions: LanguagePermissionRepository,
    Path((code, abbreviation)): Path<(String, String)>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&code).await);
    let word_category = attempt!(
        s,
        word_categories
            .find_by_abbreviation(&language.id, &abbreviation)
            .await
    );

    let user_has_permission = permissions
        .has_permission(user.id, language.id, PermissionLevel::Editor)
        .await
        .unwrap_or(false);

    let template = DeleteWordCategoryTemplate {
        current_user: Some(user),
        language,
        word_category,
        can_edit_language: user_has_permission,
    };

    okay(render_template(template))
}

async fn delete_word_category_submit(
    s: Session,
    languages: LanguageRepository,
    word_categories: WordCategoryRepository,
    Path((code, abbreviation)): Path<(String, String)>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&code).await);
    let word_category = attempt!(
        s,
        word_categories
            .find_by_abbreviation(&language.id, &abbreviation)
            .await
    );

    match word_categories.delete(&user, word_category.id).await {
        Ok(_) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!("/languages/{}/word-categories", code)).into_response(),
        ),
        Err(e) => render_generic_error(s, e).await,
    }
}
