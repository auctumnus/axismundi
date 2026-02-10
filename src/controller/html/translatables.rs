use askama::Template;
use axum::{
    Form, Router,
    extract::Path,
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use serde::Deserialize;

use crate::{
    attempt,
    controller::html::{TranslatableWithLiked, okay, render_template},
    err::AppError,
    get_user,
    model::{
        translatable::{
            CreateTranslatable, Translatable, TranslatableRepository, TranslatableSearch,
            UpdateTranslatable,
        },
        translations::{TranslationRepository, TranslationWithLanguageAndContributor},
        users::{User, UserRepository},
    },
    pagination::PaginatedRequest,
    util::{AppState, extract_session::Session},
};

pub fn create_router() -> (Router<AppState>, Router<AppState>) {
    let secure_routes = Router::<AppState>::new()
        .route("/new-translatable", post(new_translatable_submit))
        .route("/translatable/{slug}/edit", post(edit_translatable_submit));

    let normal_routes = Router::<AppState>::new()
        .route("/new-translatable", get(new_translatable_form))
        .route("/translatables", get(search_translatables))
        .route("/translatable/{slug}", get(view_translatable))
        .route("/translatable/{slug}/edit", get(edit_translatable_form));

    (secure_routes, normal_routes)
}

#[derive(Template)]
#[template(path = "translatables/new.html")]
#[allow(dead_code)]
struct NewTranslatableTemplate {
    current_user: Option<User>,
    error: Option<AppError>,
    previous_title: String,
    previous_english: String,
}

async fn new_translatable_form(s: Session) -> (StatusCode, Response) {
    let Some(user) = s.user().cloned() else {
        return (
            StatusCode::UNAUTHORIZED,
            Redirect::to("/login").into_response(),
        );
    };

    let template = NewTranslatableTemplate {
        current_user: Some(user),
        error: None,
        previous_title: String::new(),
        previous_english: String::new(),
    };

    (StatusCode::OK, render_template(template).into_response())
}

#[derive(Deserialize)]
struct NewTranslatableFormData {
    title: String,
    english: String,
}

async fn new_translatable_submit(
    s: Session,
    translatables: TranslatableRepository,
    Form(form): Form<NewTranslatableFormData>,
) -> (StatusCode, Response) {
    let user = get_user!(s);

    let translatable = translatables
        .create(
            &user,
            CreateTranslatable {
                title: form.title.clone(),
                english: form.english.clone(),
                source_name: None,
                source_url: None,
                source_content: None,
                source_language: None,
            },
        )
        .await;

    match translatable {
        Ok(translatable) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!("/translatable/{}", translatable.slug)).into_response(),
        ),
        Err(e) => {
            let status_code = e.status_code;
            let template = NewTranslatableTemplate {
                current_user: Some(user),
                error: Some(e),
                previous_title: form.title.clone(),
                previous_english: form.english.clone(),
            };

            let body = render_template(template);
            (status_code, body)
        }
    }
}

#[derive(Template)]
#[template(path = "translatables/search.html")]
#[allow(dead_code)]
struct SearchTranslatablesTemplate {
    current_user: Option<User>,
    error: Option<AppError>,
    previous_query: String,
    results: Option<Vec<TranslatableWithLiked>>,
}

async fn search_translatables(
    s: Session,
    translatables: TranslatableRepository,
    axum::extract::Query(query): axum::extract::Query<TranslatableSearch>,
) -> (StatusCode, Response) {
    let current_user = s.user().cloned();

    // Clean up query: trim and convert empty strings to None
    let query = TranslatableSearch {
        q: query.q.and_then(|q| {
            let trimmed = q.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }),
        source_language: query.source_language,
        created_by: query.created_by,
        created_before: query.created_before,
        created_after: query.created_after,
    };

    let results = match translatables
        .search(PaginatedRequest::default(), query.clone())
        .await
    {
        Ok(res) => {
            let mut translatables_with_liked = Vec::with_capacity(res.items.len());
            for translatable in res.items {
                let is_liked = if let Some(ref cu) = current_user {
                    translatables
                        .is_liked(&translatable.id, &cu.id)
                        .await
                        .unwrap_or(false)
                } else {
                    false
                };
                translatables_with_liked.push(TranslatableWithLiked {
                    translatable,
                    is_liked,
                });
            }
            Some(translatables_with_liked)
        }
        Err(e) => {
            let status_code = e.status_code;
            let template = SearchTranslatablesTemplate {
                current_user,
                error: Some(e),
                previous_query: query.q.clone().unwrap_or_default(),
                results: None,
            };
            let body = render_template(template);
            return (status_code, body);
        }
    };

    let template = SearchTranslatablesTemplate {
        current_user,
        error: None,
        previous_query: query.q.unwrap_or_default(),
        results,
    };

    let body = render_template(template);
    okay(body)
}


#[derive(Template)]
#[template(path = "translatables/view.html")]
struct ViewTranslatableTemplate {
    current_user: Option<User>,
    translatable: Translatable,
    creator: User,
    translations: Vec<TranslationWithLanguageAndContributor>,
    is_liked: bool,
    can_edit_translatable: bool,
}

async fn view_translatable(
    s: Session,
    translatables: TranslatableRepository,
    translations: TranslationRepository,
    languages: crate::model::languages::LanguageRepository,
    users: UserRepository,
    Path(slug): Path<String>,
) -> (StatusCode, Response) {
    println!("Viewing translatable: {}", slug);
    let translatable = attempt!(s, translatables.find_by_slug(&slug).await);
    println!("Found translatable: {:?}", translatable);
    let creator = attempt!(s, users.find_by_id(translatable.created_by).await);

    // Fetch the 3 most recent translations for this translatable
    let translations_list = attempt!(
        s,
        translations
            .list_by_translatable(
                translatable.id,
                PaginatedRequest {
                    limit: 3,
                    offset: 0
                }
            )
            .await
    );

    println!("Found translations: {:?}", translations_list);

    // For each translation, fetch the language and contributor
    let mut translations_with_info = Vec::new();
    for translation in translations_list.items {
        let translation = attempt!(s, translations.materialize(translation, s.user()).await);

        translations_with_info.push(translation);
    }

    // Check if the user has liked this translatable
    let is_liked = if let Some(user) = s.user() {
        translatables
            .is_liked(&translatable.id, &user.id)
            .await
            .unwrap_or(false)
    } else {
        false
    };

    // Check if the user can edit this translatable (only creator can edit)
    let can_edit_translatable = s
        .user()
        .is_some_and(|u| u.id == translatable.created_by);

    let template = ViewTranslatableTemplate {
        current_user: s.user().cloned(),
        translatable,
        creator,
        translations: translations_with_info,
        is_liked,
        can_edit_translatable,
    };
    okay(render_template(template))
}

#[derive(Template)]
#[template(path = "translatables/edit.html")]
#[allow(dead_code)]
struct EditTranslatableTemplate {
    current_user: Option<User>,
    translatable: Translatable,
    error: Option<AppError>,
    previous_title: String,
    previous_english: String,
}

async fn edit_translatable_form(
    s: Session,
    translatables: TranslatableRepository,
    Path(slug): Path<String>,
) -> (StatusCode, Response) {
    let user = get_user!(s);

    let translatable = attempt!(s, translatables.find_by_slug(&slug).await);

    let template = EditTranslatableTemplate {
        current_user: Some(user),
        translatable: translatable.clone(),
        error: None,
        previous_title: translatable.title,
        previous_english: translatable.english,
    };

    okay(render_template(template))
}

#[derive(Deserialize)]
struct EditTranslatableFormData {
    title: String,
    english: String,
}

async fn edit_translatable_submit(
    s: Session,
    translatables: TranslatableRepository,
    Path(slug): Path<String>,
    Form(form): Form<EditTranslatableFormData>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let translatable = attempt!(s, translatables.find_by_slug(&slug).await);

    let updates = UpdateTranslatable {
        slug: None,
        title: if form.title == translatable.title {
            None
        } else {
            Some(form.title.clone())
        },
        english: if form.english == translatable.english {
            None
        } else {
            Some(form.english.clone())
        },
        source_name: None,
        source_url: None,
        source_content: None,
        source_language: None,
    };

    match translatables.update(&user, translatable.id, updates).await {
        Ok(result) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!("/translatable/{}", result.slug)).into_response(),
        ),
        Err(e) => {
            let template = EditTranslatableTemplate {
                current_user: Some(user),
                translatable: translatable.clone(),
                error: Some(e),
                previous_title: form.title.clone(),
                previous_english: form.english.clone(),
            };

            let body = render_template(template);
            (StatusCode::BAD_REQUEST, body)
        }
    }
}
