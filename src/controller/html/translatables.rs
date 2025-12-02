use askama::Template;
use axum::{
    Form, Router, extract::Path, http::StatusCode, response::{Html, IntoResponse, Redirect, Response}, routing::{get, post}
};
use serde::Deserialize;

use crate::{
    attempt, controller::html::{okay, render_generic_error, render_template}, err::AppError, get_user, model::{
        languages::Language,
        translatable::{CreateTranslatable, Translatable, TranslatableRepository, TranslatableSearch, UpdateTranslatable},
        translations::TranslationRepository,
        users::{User, UserRepository},
    }, pagination::PaginatedRequest, util::{AppState, extract_session::Session}
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
        return (StatusCode::UNAUTHORIZED, Redirect::to("/login").into_response());
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
        Ok(translatable) => (StatusCode::SEE_OTHER, Redirect::to(&format!("/translatable/{}", translatable.slug)).into_response()),
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
    results: Option<Vec<Translatable>>,
}

async fn search_translatables(
    s: Session,
    translatables: TranslatableRepository,
    axum::extract::Query(query): axum::extract::Query<TranslatableSearch>,
) -> (StatusCode, Response) {
    let current_user = s.user().cloned();

    if query.q.as_ref().is_none_or(|q| q.is_empty()) {
        let template = SearchTranslatablesTemplate {
            current_user,
            error: None,
            previous_query: query.q.clone().unwrap_or_default(),
            results: None,
        };
        let body = render_template(template);
        return okay(body);
    }

    let results = match translatables
        .search(PaginatedRequest::default(), query.clone())
        .await
    {
        Ok(res) => Some(res.items),
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

#[derive(Clone)]
struct TranslationWithLanguageAndContributor {
    language: Language,
    contributor: Option<User>,
}

#[derive(Template)]
#[template(path = "translatables/view.html")]
struct ViewTranslatableTemplate {
    current_user: Option<User>,
    translatable: Translatable,
    creator: User,
    translations: Vec<TranslationWithLanguageAndContributor>,
}

async fn view_translatable(
    s: Session,
    translatables: TranslatableRepository,
    translations: TranslationRepository,
    languages: crate::model::languages::LanguageRepository,
    users: UserRepository,
    Path(slug): Path<String>,
) -> (StatusCode, Response) {
    let translatable = attempt!(s, translatables.find_by_slug(&slug).await);
    let creator = attempt!(s, users.find_by_id(translatable.created_by).await);

    // Fetch all translations for this translatable
    let translations_list = attempt!(s, translations
        .list_by_translatable(translatable.id, PaginatedRequest { limit: 100, offset: 0 })
        .await);

    // For each translation, fetch the language and contributor
    let mut translations_with_info = Vec::new();
    for translation in translations_list.items {
        let language = attempt!(s, languages.find_by_id(translation.language).await);
        let contributor = users.find_by_id(translation.created_by).await.ok();

        translations_with_info.push(TranslationWithLanguageAndContributor {
            language,
            contributor,
        });
    }

    let template = ViewTranslatableTemplate {
        current_user: s.user().cloned(),
        translatable,
        creator,
        translations: translations_with_info,
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
        title: if form.title != translatable.title {
            Some(form.title.clone())
        } else {
            None
        },
        english: if form.english != translatable.english {
            Some(form.english.clone())
        } else {
            None
        },
        source_name: None,
        source_url: None,
        source_content: None,
        source_language: None,
    };

    match translatables.update(&user, translatable.id, updates).await {
        Ok(updated) => (StatusCode::SEE_OTHER, Redirect::to(&format!("/translatable/{}", updated.slug)).into_response()),
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
