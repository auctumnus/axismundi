use askama::Template;
use axum::{
    Form, Router,
    extract::Path,
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use chrono::NaiveDate;
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    attempt,
    controller::html::{okay, render_template},
    err::AppError,
    get_user,
    model::{
        translatable::{Translatable, TranslatableRepository, TranslatableWithMeta},
        translatable_of_the_day::{PeekRow, TotdEntry, TranslatableOfTheDayRepository},
        user_tags::UserTagRepository,
        users::User,
    },
    pagination::{PaginatedRequest, PaginatedResponse, PaginationTemplate},
    util::{AppState, extract_session::Session},
};

const ADMIN_PREVIEW_SIZE: i32 = 3;
const ADMIN_UPCOMING_PAGE_SIZE: i32 = 20;

pub fn create_router() -> (Router<AppState>, Router<AppState>) {
    let secure_routes = Router::<AppState>::new()
        .route("/translatable/{slug}/schedule", post(schedule_submit))
        .route("/translatable/{slug}/unschedule", post(unschedule_submit));

    let normal_routes = Router::<AppState>::new()
        .route("/translatable-of-the-day", get(view_today))
        .route("/translatable-of-the-day/archive", get(view_archive))
        .route("/translatable-of-the-day/admin", get(admin_dashboard))
        .route(
            "/translatable-of-the-day/admin/upcoming",
            get(admin_upcoming),
        )
        .route("/translatable/{slug}/schedule", get(schedule_form))
        .route("/translatable/{slug}/unschedule", get(unschedule_form));

    (secure_routes, normal_routes)
}

#[derive(Template)]
#[template(path = "translatable_of_the_day/view.html")]
struct ViewTodayTemplate {
    current_user: Option<User>,
    entry: Option<TotdEntry>,
}

async fn view_today(s: Session, totd: TranslatableOfTheDayRepository) -> (StatusCode, Response) {
    let entry = attempt!(s, totd.today(s.user()).await);
    let template = ViewTodayTemplate {
        current_user: s.user().cloned(),
        entry,
    };
    okay(render_template(template))
}

#[derive(Template)]
#[template(path = "translatable_of_the_day/archive.html")]
struct ArchiveTemplate {
    current_user: Option<User>,
    archive: PaginatedResponse<TotdEntry>,
}

async fn view_archive(
    s: Session,
    totd: TranslatableOfTheDayRepository,
    pagination: PaginatedRequest,
) -> (StatusCode, Response) {
    let archive = attempt!(s, totd.archive(pagination, s.user()).await);
    let template = ArchiveTemplate {
        current_user: s.user().cloned(),
        archive,
    };
    okay(render_template(template))
}

#[derive(Template)]
#[template(path = "translatable_of_the_day/admin.html")]
#[allow(dead_code)]
struct AdminTemplate {
    current_user: Option<User>,
    queue: PaginatedResponse<Translatable>,
    upcoming_preview: Vec<PeekRow>,
    error: Option<AppError>,
    today: NaiveDate,
}

async fn admin_dashboard(
    s: Session,
    totd: TranslatableOfTheDayRepository,
    user_tags: UserTagRepository,
    pagination: PaginatedRequest,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    if !is_staff(&user, &user_tags).await {
        return (
            StatusCode::SEE_OTHER,
            Redirect::to("/home").into_response(),
        );
    }

    let queue = attempt!(s, totd.queue(pagination).await);
    let preview = attempt!(
        s,
        totd.peek_upcoming(PaginatedRequest::first(ADMIN_PREVIEW_SIZE), Some(&user))
            .await
    );

    let template = AdminTemplate {
        current_user: Some(user),
        queue,
        upcoming_preview: preview.items,
        error: None,
        today: chrono::Utc::now().date_naive(),
    };
    okay(render_template(template))
}

#[derive(Template)]
#[template(path = "translatable_of_the_day/admin_upcoming.html")]
#[allow(dead_code)]
struct AdminUpcomingTemplate {
    current_user: Option<User>,
    upcoming: PaginatedResponse<PeekRow>,
    pagination: PaginationTemplate,
    today: NaiveDate,
}

async fn admin_upcoming(
    s: Session,
    totd: TranslatableOfTheDayRepository,
    user_tags: UserTagRepository,
    pagination: PaginatedRequest,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    if !is_staff(&user, &user_tags).await {
        return (
            StatusCode::SEE_OTHER,
            Redirect::to("/home").into_response(),
        );
    }

    // default to a larger page size than PaginatedRequest's default of 10
    let effective_pagination = if pagination.limit == 10 {
        PaginatedRequest {
            limit: ADMIN_UPCOMING_PAGE_SIZE,
            offset: pagination.offset,
        }
    } else {
        pagination.clone()
    };

    let upcoming = attempt!(
        s,
        totd.peek_upcoming(effective_pagination.clone(), Some(&user))
            .await
    );

    let pagination_template = PaginationTemplate::from_paginated_response(
        "/translatable-of-the-day/admin/upcoming",
        &upcoming,
        &effective_pagination,
        (),
    );

    let template = AdminUpcomingTemplate {
        current_user: Some(user),
        upcoming,
        pagination: pagination_template,
        today: chrono::Utc::now().date_naive(),
    };
    okay(render_template(template))
}

#[derive(Template)]
#[template(path = "translatable_of_the_day/schedule.html")]
#[allow(dead_code)]
struct ScheduleFormTemplate {
    current_user: Option<User>,
    translatable_with_meta: TranslatableWithMeta,
    today: NaiveDate,
    previous_date: String,
    error: Option<AppError>,
}

#[derive(Deserialize)]
struct ScheduleFormQuery {
    date: Option<NaiveDate>,
}

async fn schedule_form(
    s: Session,
    translatables: TranslatableRepository,
    user_tags: UserTagRepository,
    axum::extract::Path(slug): axum::extract::Path<String>,
    axum::extract::Query(query): axum::extract::Query<ScheduleFormQuery>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    if !is_staff(&user, &user_tags).await {
        return (
            StatusCode::SEE_OTHER,
            Redirect::to("/home").into_response(),
        );
    }

    let translatable = attempt!(s, translatables.find_by_slug(&slug).await);
    let translatable_with_meta = attempt!(
        s,
        translatables.materialize(translatable, Some(&user)).await
    );

    let template = ScheduleFormTemplate {
        current_user: Some(user),
        translatable_with_meta,
        today: chrono::Utc::now().date_naive(),
        previous_date: query.date.map(|d| d.to_string()).unwrap_or_default(),
        error: None,
    };
    okay(render_template(template))
}

#[derive(Deserialize)]
struct ScheduleSubmitForm {
    date: NaiveDate,
}

async fn schedule_submit(
    s: Session,
    totd: TranslatableOfTheDayRepository,
    translatables: TranslatableRepository,
    user_tags: UserTagRepository,
    axum::extract::Path(slug): axum::extract::Path<String>,
    Form(form): Form<ScheduleSubmitForm>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    if !is_staff(&user, &user_tags).await {
        return (
            StatusCode::SEE_OTHER,
            Redirect::to("/home").into_response(),
        );
    }

    let translatable = attempt!(s, translatables.find_by_slug(&slug).await);

    match totd.schedule(&user, form.date, translatable.id).await {
        Ok(_) => (
            StatusCode::SEE_OTHER,
            Redirect::to("/translatable-of-the-day/admin").into_response(),
        ),
        Err(error) => {
            let translatable_with_meta = attempt!(
                s,
                translatables.materialize(translatable, Some(&user)).await
            );
            let template = ScheduleFormTemplate {
                current_user: Some(user),
                translatable_with_meta,
                today: chrono::Utc::now().date_naive(),
                previous_date: form.date.to_string(),
                error: Some(error),
            };
            (StatusCode::BAD_REQUEST, render_template(template))
        }
    }
}

#[derive(Template)]
#[template(path = "translatable_of_the_day/unschedule.html")]
#[allow(dead_code)]
struct UnscheduleFormTemplate {
    current_user: Option<User>,
    entry: TotdEntry,
    error: Option<AppError>,
}

async fn unschedule_form(
    s: Session,
    totd: TranslatableOfTheDayRepository,
    translatables: TranslatableRepository,
    user_tags: UserTagRepository,
    axum::extract::Path(slug): axum::extract::Path<String>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    if !is_staff(&user, &user_tags).await {
        return (
            StatusCode::SEE_OTHER,
            Redirect::to("/home").into_response(),
        );
    }

    let translatable = attempt!(s, translatables.find_by_slug(&slug).await);
    let scheduled_date = attempt!(s, totd.scheduled_date_for(translatable.id).await);
    let Some(date) = scheduled_date else {
        return attempt!(
            s,
            Err::<(StatusCode, Response), _>(crate::err::bad_request(format!(
                "translatable '{slug}' isn't scheduled"
            )))
        );
    };

    let entry = attempt!(s, totd.for_date(date, Some(&user)).await);
    let Some(entry) = entry else {
        return attempt!(
            s,
            Err::<(StatusCode, Response), _>(crate::err::not_found(format!(
                "TotD scheduled for {date}"
            )))
        );
    };

    let template = UnscheduleFormTemplate {
        current_user: Some(user),
        entry,
        error: None,
    };
    okay(render_template(template))
}

async fn unschedule_submit(
    s: Session,
    totd: TranslatableOfTheDayRepository,
    translatables: TranslatableRepository,
    user_tags: UserTagRepository,
    axum::extract::Path(slug): axum::extract::Path<String>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    if !is_staff(&user, &user_tags).await {
        return (
            StatusCode::SEE_OTHER,
            Redirect::to("/home").into_response(),
        );
    }

    let translatable = attempt!(s, translatables.find_by_slug(&slug).await);

    match totd.unschedule(&user, translatable.id).await {
        Ok(_) => (
            StatusCode::SEE_OTHER,
            Redirect::to("/translatable-of-the-day/admin").into_response(),
        ),
        Err(error) => {
            let scheduled_date = attempt!(s, totd.scheduled_date_for(translatable.id).await);
            let Some(date) = scheduled_date else {
                return attempt!(s, Err::<(StatusCode, Response), _>(error));
            };
            let entry = attempt!(s, totd.for_date(date, Some(&user)).await);
            let Some(entry) = entry else {
                return attempt!(s, Err::<(StatusCode, Response), _>(error));
            };
            let template = UnscheduleFormTemplate {
                current_user: Some(user),
                entry,
                error: Some(error),
            };
            (StatusCode::BAD_REQUEST, render_template(template))
        }
    }
}

async fn is_staff(user: &User, user_tags: &UserTagRepository) -> bool {
    user_tags.is_admin(user.id).await.unwrap_or(false)
        || user_tags.is_moderator(user.id).await.unwrap_or(false)
}
