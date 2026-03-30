use askama::Template;
use axum::{
    Form, Router,
    extract::Path,
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use axum_extra::{TypedHeader, headers::UserAgent};
use serde::Deserialize;

use crate::{
    attempt,
    controller::html::{TranslatableWithMeta, okay, render_generic_error, render_template},
    embed::{EmbedTarget, GenericEmbed, render_embed, truncate_description},
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
        .route("/translatable/{slug}/edit", post(edit_translatable_submit))
        .route("/translatable/{slug}/edit-source", post(edit_source_submit))
        .route(
            "/translatable/{slug}/clear-source",
            post(clear_source_submit),
        )
        .route("/translatable/{slug}/delete", post(delete_translatable_submit));

    let normal_routes = Router::<AppState>::new()
        .route("/new-translatable", get(new_translatable_form))
        .route("/translatables", get(search_translatables))
        .route("/translatable/{slug}", get(view_translatable))
        .route("/translatable/{slug}/edit", get(edit_translatable_form))
        .route("/translatable/{slug}/edit-source", get(edit_source_form))
        .route(
            "/translatable/{slug}/clear-source",
            get(clear_source_form),
        )
        .route("/translatable/{slug}/delete", get(delete_translatable_form));

    (secure_routes, normal_routes)
}

#[derive(Template)]
#[template(path = "translatables/new.html")]
#[allow(dead_code)]
struct NewTranslatableTemplate {
    current_user: Option<User>,
    error: Option<AppError>,
    previous_title: String,
    previous_description: String,
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
        previous_description: String::new(),
        previous_english: String::new(),
    };

    (StatusCode::OK, render_template(template).into_response())
}

#[derive(Deserialize)]
struct NewTranslatableFormData {
    title: String,
    description: Option<String>,
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
                description: form.description.clone(),
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
                previous_description: form.description.clone().unwrap_or_default(),
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
    results: Option<Vec<TranslatableWithMeta>>,
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
                translatables_with_liked.push(attempt!(
                    s,
                    translatables.materialize(translatable, current_user.as_ref()).await
                ));
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
    translatable_with_meta: TranslatableWithMeta,
    translations: Vec<TranslationWithLanguageAndContributor>,
    can_edit_translatable: bool,
    json_ld: String,
    rendered_description: Option<String>,
}

async fn view_translatable(
    s: Session,
    user_agent: Option<TypedHeader<UserAgent>>,
    translatables: TranslatableRepository,
    translations: TranslationRepository,
    users: UserRepository,
    Path(slug): Path<String>,
) -> (StatusCode, Response) {
    let translatable = attempt!(s, translatables.find_by_slug(&slug).await);

    if let Some(ua) = user_agent
        && ua.as_str().to_lowercase().contains("discordbot")
    {
        let creator = attempt!(s, users.find_by_id(translatable.created_by).await);
        return okay(
            render_embed(
                EmbedTarget::Discord,
                GenericEmbed {
                    title: translatable.title.clone(),
                    description: format!(
                        "{}\n\n⭐️ {}",
                        truncate_description(&translatable.english),
                        translatable.like_count
                    ),
                    author: Some(creator),
                    color: None,
                    url: format!(
                        "{}/translatable/{}",
                        &crate::CONFIG.public_url_base,
                        translatable.slug
                    ),
                    image: None,
                },
            )
            .await
            .into_response(),
        );
    }

    let can_edit_translatable = s.user().is_some_and(|u| u.id == translatable.created_by);

    let json_ld = attempt!(
        s,
        serde_json::to_string(&attempt!(s, translatables.as_json_ld(&translatable).await))
            .map_err(Into::into)
    );

    let rendered_description = if let Some(description) = &translatable.description {
        crate::md::render_md(description).ok()
    } else {
        None
    };

    let translatable_with_meta = attempt!(
        s,
        translatables.materialize(translatable, s.user()).await
    );

    // Fetch the 3 most recent translations for this translatable
    let translations_list = attempt!(
        s,
        translations
            .list_by_translatable(
                translatable_with_meta.translatable.id,
                PaginatedRequest {
                    limit: 3,
                    offset: 0
                }
            )
            .await
    );

    let mut translations_with_info = Vec::new();
    for translation in translations_list.items {
        let translation = attempt!(s, translations.materialize(translation, s.user()).await);
        translations_with_info.push(translation);
    }

    let template = ViewTranslatableTemplate {
        current_user: s.user().cloned(),
        translatable_with_meta,
        translations: translations_with_info,
        can_edit_translatable,
        json_ld,
        rendered_description,
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
    previous_description: String,
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
        previous_description: translatable.description.unwrap_or_default(),
        previous_english: translatable.english,
    };

    okay(render_template(template))
}

async fn edit_translatable_submit(
    s: Session,
    translatables: TranslatableRepository,
    Path(slug): Path<String>,
    Form(updates): Form<UpdateTranslatable>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let translatable = attempt!(s, translatables.find_by_slug(&slug).await);

    match translatables.update(&user, translatable.id, updates.clone()).await {
        Ok(result) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!("/translatable/{}", result.slug)).into_response(),
        ),
        Err(e) => {
            let template = EditTranslatableTemplate {
                current_user: Some(user),
                translatable: translatable.clone(),
                error: Some(e),
                previous_title: updates.title.unwrap_or_default(),
                previous_description: updates.description.unwrap_or_default(),
                previous_english: updates.english.unwrap_or_default(),
            };

            let body = render_template(template);
            (StatusCode::BAD_REQUEST, body)
        }
    }
}

#[derive(Template)]
#[template(path = "translatables/edit-source.html")]
#[allow(dead_code)]
struct EditSourceTemplate {
    current_user: Option<User>,
    translatable: Translatable,
    error: Option<AppError>,
    previous_source_name: String,
    previous_source_url: String,
    previous_source_language: String,
    previous_source_content: String,
}

async fn edit_source_form(
    s: Session,
    translatables: TranslatableRepository,
    Path(slug): Path<String>,
) -> (StatusCode, Response) {
    let user = get_user!(s);

    let translatable = attempt!(s, translatables.find_by_slug(&slug).await);

    let template = EditSourceTemplate {
        current_user: Some(user),
        translatable: translatable.clone(),
        error: None,
        previous_source_name: translatable.source_name.unwrap_or_default(),
        previous_source_url: translatable.source_url.unwrap_or_default(),
        previous_source_language: translatable.source_language.unwrap_or_default(),
        previous_source_content: translatable.source_content.unwrap_or_default(),
    };

    okay(render_template(template))
}

#[derive(Deserialize)]
struct EditSourceFormData {
    source_name: String,
    source_url: String,
    source_language: String,
    source_content: String,
}

async fn edit_source_submit(
    s: Session,
    translatables: TranslatableRepository,
    Path(slug): Path<String>,
    Form(form): Form<EditSourceFormData>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let translatable = attempt!(s, translatables.find_by_slug(&slug).await);

    let to_opt = |s: &str| {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    };

    let updates = UpdateTranslatable {
        title: None,
        english: None,
        source_name: to_opt(&form.source_name),
        source_url: to_opt(&form.source_url),
        source_content: to_opt(&form.source_content),
        source_language: to_opt(&form.source_language),
        description: None,
    };

    match translatables.update(&user, translatable.id, updates).await {
        Ok(result) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!("/translatable/{}", result.slug)).into_response(),
        ),
        Err(e) => {
            let template = EditSourceTemplate {
                current_user: Some(user),
                translatable: translatable.clone(),
                error: Some(e),
                previous_source_name: form.source_name.clone(),
                previous_source_url: form.source_url.clone(),
                previous_source_language: form.source_language.clone(),
                previous_source_content: form.source_content.clone(),
            };

            let body = render_template(template);
            (StatusCode::BAD_REQUEST, body)
        }
    }
}

#[derive(Template)]
#[template(path = "translatables/clear-source.html")]
#[allow(dead_code)]
struct ClearSourceTemplate {
    current_user: Option<User>,
    translatable: Translatable,
    will_create_audit_log: bool,
}

async fn clear_source_form(
    s: Session,
    translatables: TranslatableRepository,
    Path(slug): Path<String>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let translatable = attempt!(s, translatables.find_by_slug(&slug).await);

    let template = ClearSourceTemplate {
        current_user: Some(user),
        translatable,
        will_create_audit_log: false,
    };

    okay(render_template(template))
}

async fn clear_source_submit(
    s: Session,
    translatables: TranslatableRepository,
    Path(slug): Path<String>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let translatable = attempt!(s, translatables.find_by_slug(&slug).await);

    match translatables.clear_source(&user, translatable.id).await {
        Ok(result) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!("/translatable/{}", result.slug)).into_response(),
        ),
        Err(e) => render_generic_error(s, e).await,
    }
}

#[derive(Template)]
#[template(path = "translatables/delete.html")]
#[allow(dead_code)]
struct DeleteTranslatableTemplate {
    current_user: Option<User>,
    translatable: Translatable,
    will_create_audit_log: bool,
}

async fn delete_translatable_form(
    s: Session,
    translatables: TranslatableRepository,
    Path(slug): Path<String>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let translatable = attempt!(s, translatables.find_by_slug(&slug).await);

    let template = DeleteTranslatableTemplate {
        current_user: Some(user),
        translatable,
        will_create_audit_log: false,
    };

    okay(render_template(template))
}

async fn delete_translatable_submit(
    s: Session,
    translatables: TranslatableRepository,
    Path(slug): Path<String>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let translatable = attempt!(s, translatables.find_by_slug(&slug).await);

    match translatables.delete(&user, translatable).await {
        Ok(_) => (
            StatusCode::SEE_OTHER,
            Redirect::to("/translatables").into_response(),
        ),
        Err(e) => render_generic_error(s, e).await,
    }
}
