use askama::Template;
use axum::{
    Form, Router,
    extract::{Path, Query},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use axum_extra::{TypedHeader, headers::UserAgent};
use chrono::{NaiveDate, Utc};
use futures::TryFutureExt;
use serde::Deserialize;

use crate::{
    attempt,
    controller::html::{TranslatableWithMeta, okay, render_generic_error, render_template},
    embed::{EmbedTarget, GenericEmbed, render_embed, truncate_description},
    err::{AppError, bad_request},
    get_user,
    model::{
        translatable::{
            CreateTranslatable, Translatable, TranslatableRepository, TranslatableSearch,
            UpdateTranslatable,
        },
        translatable_of_the_day::TranslatableOfTheDayRepository,
        translations::{TranslationRepository, TranslationWithLanguageAndContributor},
        users::{User, UserRepository},
    },
    pagination::PaginatedRequest,
    util::{
        AppState, BackQuery, ListHeaderKind,
        extract_session::Session,
        search_template::{SearchTemplateArgs, make_search_layout},
    },
};

pub fn create_router() -> (Router<AppState>, Router<AppState>) {
    let secure_routes = Router::<AppState>::new()
        .route("/new-translatable", post(new_translatable_submit))
        .route("/translatable/{slug}/edit", post(edit_translatable_submit))
        .route("/translatable/{slug}/edit-source", post(edit_source_submit))
        .route(
            "/translatable/{slug}/delete",
            post(delete_translatable_submit),
        )
        .route("/translatable/{slug}/publish", post(publish_submit))
        .route("/translatable/{slug}/unpublish", post(unpublish_submit));

    let normal_routes = Router::<AppState>::new()
        .route("/new-translatable", get(new_translatable_form))
        .route("/translatables", get(search_translatables))
        .route("/translatable/{slug}", get(view_translatable))
        .route("/translatable/{slug}/edit", get(edit_translatable_form))
        .route("/translatable/{slug}/edit-source", get(edit_source_form))
        .route("/translatable/{slug}/delete", get(delete_translatable_form));

    (secure_routes, normal_routes)
}

#[derive(Template)]
#[template(path = "translatables/fragments/card.html")]
pub struct PreviewCard<'a> {
    pub translatable_with_meta: TranslatableWithMeta,
    pub back_url: &'a str,
}

#[derive(Template)]
#[template(path = "translatables/fragments/list_header.html")]
#[allow(dead_code)]
pub struct Header<'a> {
    pub current_user: Option<&'a User>,
    pub kind: ListHeaderKind,
}

#[derive(Template)]
#[template(path = "translatables/new.html")]
#[allow(dead_code)]
struct NewTranslatableTemplate {
    current_user: Option<User>,
    is_staff: bool,
    error: Option<AppError>,
    previous_title: String,
    previous_description: String,
    previous_english: String,
    previous_as_draft: bool,
    previous_schedule_date: String,
    today: NaiveDate,
}

#[derive(Deserialize, Default)]
struct NewTranslatableQuery {
    #[serde(default)]
    as_draft: Option<String>,
}

async fn new_translatable_form(
    s: Session,
    Query(q): Query<NewTranslatableQuery>,
) -> (StatusCode, Response) {
    let Some(user) = s.user().cloned() else {
        return (
            StatusCode::UNAUTHORIZED,
            Redirect::to("/login").into_response(),
        );
    };

    let is_staff = user.is_admin() || user.is_moderator();
    let template = NewTranslatableTemplate {
        current_user: Some(user),
        is_staff,
        error: None,
        previous_title: String::new(),
        previous_description: String::new(),
        previous_english: String::new(),
        previous_as_draft: is_staff && q.as_draft.is_some(),
        previous_schedule_date: String::new(),
        today: Utc::now().date_naive(),
    };

    (StatusCode::OK, render_template(template).into_response())
}

#[derive(Deserialize)]
struct NewTranslatableFormData {
    title: String,
    description: Option<String>,
    english: String,
    /// Staff-only checkbox. Ignored for non-staff submitters.
    as_draft: Option<String>,
    /// Staff-only: when set together with `as_draft`, schedule the newly
    /// created draft for this date as the TotD. Ignored for non-staff or
    /// when empty.
    schedule_date: Option<String>,
}

async fn new_translatable_submit(
    s: Session,
    translatables: TranslatableRepository,
    Form(form): Form<NewTranslatableFormData>,
) -> (StatusCode, Response) {
    let user = get_user!(s);

    let is_staff = user.is_admin() || user.is_moderator();
    let as_draft = is_staff && form.as_draft.is_some();

    let schedule_date_raw = form
        .schedule_date
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let render_form_error =
        |user: User, error: AppError, status: StatusCode, schedule_raw: &str| {
            let template = NewTranslatableTemplate {
                current_user: Some(user),
                is_staff,
                error: Some(error),
                previous_title: form.title.clone(),
                previous_description: form.description.clone().unwrap_or_default(),
                previous_english: form.english.clone(),
                previous_as_draft: as_draft,
                previous_schedule_date: schedule_raw.to_string(),
                today: Utc::now().date_naive(),
            };
            (status, render_template(template))
        };

    let schedule_date = if is_staff && as_draft {
        match schedule_date_raw
            .map(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d"))
            .transpose()
        {
            Ok(d) => d,
            Err(_) => {
                return render_form_error(
                    user,
                    bad_request("invalid schedule date"),
                    StatusCode::BAD_REQUEST,
                    schedule_date_raw.unwrap_or(""),
                );
            }
        }
    } else {
        None
    };

    let payload = CreateTranslatable {
        title: form.title.clone(),
        english: form.english.clone(),
        source_name: None,
        source_url: None,
        source_content: None,
        source_language: None,
        description: form.description.clone(),
        as_draft,
    };

    let translatable = if let Some(date) = schedule_date {
        translatables.create_and_schedule(&user, payload, date).await
    } else {
        translatables.create(&user, payload).await
    };

    match translatable {
        Ok(translatable) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!("/translatable/{}", translatable.slug)).into_response(),
        ),
        Err(e) => {
            let status_code = e.status_code;
            render_form_error(
                user,
                e,
                status_code,
                schedule_date_raw.unwrap_or(""),
            )
        }
    }
}

async fn search_translatables(
    s: Session,
    translatables: TranslatableRepository,
    pagination: PaginatedRequest,
    axum::extract::Query(query): axum::extract::Query<TranslatableSearch>,
) -> (StatusCode, Response) {
    let current_user = s.user().cloned();

    let back_url = crate::util::back_url("/translatables", &pagination, &query);

    let results = translatables
        .search(pagination.clone(), query.clone(), s.user())
        .and_then(|response| {
            response.try_map_async(|translatable| translatables.materialize(translatable, s.user()))
        })
        .await;

    let render_item = |translatable_with_meta: &TranslatableWithMeta| PreviewCard {
        translatable_with_meta: translatable_with_meta.clone(),
        back_url: &back_url,
    };

    let header = Header {
        current_user: current_user.as_ref(),
        kind: ListHeaderKind::Search,
    };

    let template = make_search_layout(SearchTemplateArgs {
        current_user: current_user.clone(),
        header,
        query_template: query.clone(),
        query,
        results,
        pagination,
        search_name: "translatables",
        search_action: "/translatables",
        render_item,
    });

    let status = template.status();

    (status, render_template(template))
}

#[derive(Template)]
#[template(path = "translatables/view.html")]
struct ViewTranslatableTemplate {
    current_user: Option<User>,
    translatable_with_meta: TranslatableWithMeta,
    translations: Vec<TranslationWithLanguageAndContributor>,
    can_edit_translatable: bool,
    is_admin_or_mod: bool,
    scheduled_date: Option<NaiveDate>,
    json_ld: String,
    rendered_description: Option<String>,
    back: String,
}

async fn view_translatable(
    s: Session,
    user_agent: Option<TypedHeader<UserAgent>>,
    translatables: TranslatableRepository,
    translations: TranslationRepository,
    totd: TranslatableOfTheDayRepository,
    users: UserRepository,
    Path(slug): Path<String>,
    Query(back_query): Query<BackQuery>,
) -> (StatusCode, Response) {
    let translatable = attempt!(s, translatables.find_by_slug_for(&slug, s.user()).await);

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

    let rendered_description = if translatable.description.is_empty() {
        None
    } else {
        crate::md::render_md(&translatable.description).ok()
    };

    let translatable_with_meta =
        attempt!(s, translatables.materialize(translatable, s.user()).await);

    let is_admin_or_mod = s
        .user()
        .is_some_and(|u| u.is_admin() || u.is_moderator());

    let scheduled_date = if is_admin_or_mod && translatable_with_meta.translatable.is_draft() {
        attempt!(
            s,
            totd.scheduled_date_for(translatable_with_meta.translatable.id)
                .await
        )
    } else {
        None
    };

    // Fetch the 3 most recent translations for this translatable
    let translations = attempt!(
        s,
        translations
            .list_by_translatable(
                translatable_with_meta.translatable.id,
                PaginatedRequest {
                    limit: 3,
                    offset: 0
                }
            )
            .and_then(|response| {
                response
                    .try_map_async(|translation| translations.materialize(translation, s.user()))
            })
            .await
    );

    let template = ViewTranslatableTemplate {
        current_user: s.user().cloned(),
        translatable_with_meta,
        translations: translations.items,
        can_edit_translatable,
        is_admin_or_mod,
        scheduled_date,
        json_ld,
        rendered_description,
        back: back_query.back.unwrap_or_default(),
    };
    okay(render_template(template))
}

#[derive(Template)]
#[template(path = "translatables/edit.html")]
#[allow(dead_code)]
struct EditTranslatableTemplate {
    current_user: Option<User>,
    is_staff: bool,
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

    let is_staff = user.is_admin() || user.is_moderator();
    let translatable = attempt!(
        s,
        translatables.find_by_slug_for(&slug, Some(&user)).await
    );

    let template = EditTranslatableTemplate {
        current_user: Some(user),
        is_staff,
        translatable: translatable.clone(),
        error: None,
        previous_title: translatable.title,
        previous_description: translatable.description.clone(),
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
    let is_staff = user.is_admin() || user.is_moderator();
    let translatable = attempt!(
        s,
        translatables.find_by_slug_for(&slug, Some(&user)).await
    );

    match translatables
        .update(&user, translatable.id, updates.clone())
        .await
    {
        Ok(result) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!("/translatable/{}", result.slug)).into_response(),
        ),
        Err(e) => {
            let template = EditTranslatableTemplate {
                current_user: Some(user),
                is_staff,
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

async fn publish_submit(
    s: Session,
    translatables: TranslatableRepository,
    Path(slug): Path<String>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let translatable = attempt!(
        s,
        translatables.find_by_slug_for(&slug, Some(&user)).await
    );
    attempt!(s, translatables.publish(&user, translatable.id).await);
    (
        StatusCode::SEE_OTHER,
        Redirect::to(&format!("/translatable/{}", translatable.slug)).into_response(),
    )
}

async fn unpublish_submit(
    s: Session,
    translatables: TranslatableRepository,
    Path(slug): Path<String>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let translatable = attempt!(
        s,
        translatables.find_by_slug_for(&slug, Some(&user)).await
    );
    attempt!(s, translatables.unpublish(&user, translatable.id).await);
    (
        StatusCode::SEE_OTHER,
        Redirect::to(&format!("/translatable/{}", translatable.slug)).into_response(),
    )
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

    let translatable = attempt!(
        s,
        translatables.find_by_slug_for(&slug, Some(&user)).await
    );

    let template = EditSourceTemplate {
        current_user: Some(user),
        translatable: translatable.clone(),
        error: None,
        previous_source_name: translatable.source_name,
        previous_source_url: translatable.source_url,
        previous_source_language: translatable.source_language,
        previous_source_content: translatable.source_content,
    };

    okay(render_template(template))
}

async fn edit_source_submit(
    s: Session,
    translatables: TranslatableRepository,
    Path(slug): Path<String>,
    Form(form): Form<UpdateTranslatable>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let translatable = attempt!(
        s,
        translatables.find_by_slug_for(&slug, Some(&user)).await
    );

    match translatables
        .update(&user, translatable.id, form.clone())
        .await
    {
        Ok(result) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!("/translatable/{}", result.slug)).into_response(),
        ),
        Err(e) => {
            let template = EditSourceTemplate {
                current_user: Some(user),
                translatable: translatable.clone(),
                error: Some(e),
                previous_source_name: form.source_name.unwrap_or_default(),
                previous_source_url: form.source_url.unwrap_or_default(),
                previous_source_language: form.source_language.unwrap_or_default(),
                previous_source_content: form.source_content.unwrap_or_default(),
            };

            let body = render_template(template);
            (StatusCode::BAD_REQUEST, body)
        }
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
    let translatable = attempt!(
        s,
        translatables.find_by_slug_for(&slug, Some(&user)).await
    );

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
    let translatable = attempt!(
        s,
        translatables.find_by_slug_for(&slug, Some(&user)).await
    );

    match translatables.delete(&user, translatable).await {
        Ok(_) => (
            StatusCode::SEE_OTHER,
            Redirect::to("/translatables").into_response(),
        ),
        Err(e) => render_generic_error(s, e).await,
    }
}
