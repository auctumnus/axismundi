use askama::Template;
use axum::{
    Router,
    extract::{Path, Query},
    http::StatusCode,
    response::{IntoResponse, Response, Redirect},
    routing::{get, post},
    Form,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    controller::html::{okay, render_template},
    err::AppError,
    get_user,
    model::{
        reports::{Report, ReportSearch, ReportRepository, ReportableResource, ResolutionStatus, ReportPriority, CreateReport},
        user_tags::UserTagRepository,
        users::{User, UserRepository},
    },
    pagination::{PaginatedRequest, PaginatedResponse},
    util::{AppState, extract_session::Session, ensure_verified},
};

pub fn create_router() -> (Router<AppState>, Router<AppState>) {
    let secure_routes = Router::new()
        .route("/report/new", post(create_report_submit));
    let normal_routes = Router::new()
        .route("/admin/reports", get(search_reports))
        .route("/admin/reports/{id}", get(view_report))
        .route("/report/new", get(new_report_form));

    (secure_routes, normal_routes)
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
struct ReportSearchQuery {
    text_query: Option<String>,
    resource_type: Option<ReportableResource>,
    resource_id: Option<Uuid>,
    username: Option<String>,
    resolution_status: Option<ResolutionStatus>,
    priority: Option<ReportPriority>,
}

#[derive(Template)]
#[template(path = "reports/search.html")]
struct SearchReportsTemplate {
    current_user: Option<User>,
    error: Option<AppError>,
    previous_query: ReportSearchQuery,
    previous_pagination: PaginatedRequest,
    results: Option<PaginatedResponse<Report>>,
}

async fn search_reports(
    s: Session,
    reports: ReportRepository,
    user_tags: UserTagRepository,
    users: UserRepository,
    Query(query): Query<ReportSearchQuery>,
    Query(pagination): Query<PaginatedRequest>,
) -> (StatusCode, Response) {
    let current_user = match s.user() {
        Some(u) => u.clone(),
        None => {
            return (
                StatusCode::SEE_OTHER,
                axum::response::Redirect::to("/login").into_response()
            )
        }
    };

    // Check if user is mod or admin
    let is_admin = user_tags.is_admin(current_user.id).await.unwrap_or(false);
    let is_moderator = user_tags.is_moderator(current_user.id).await.unwrap_or(false);

    if !(is_admin || is_moderator) {
        return (
            StatusCode::SEE_OTHER,
            axum::response::Redirect::to("/home").into_response()
        );
    }

    // Resolve username to user_id if provided
    let reporter = if let Some(username) = &query.username {
        match users.find_by_username(username).await {
            Ok(user) => Some(user.id),
            Err(_) => None, // Username not found, will return no results
        }
    } else {
        None
    };

    let search = ReportSearch {
        text_query: query.text_query.clone(),
        resource_type: query.resource_type,
        resource_id: query.resource_id,
        reporter,
        resolution_status: query.resolution_status,
        priority: query.priority,
    };

    let results = match reports.search(&current_user, pagination.clone(), search).await {
        Ok(res) => Some(res),
        Err(e) => {
            let template = SearchReportsTemplate {
                current_user: Some(current_user),
                error: Some(e),
                previous_query: query,
                previous_pagination: pagination,
                results: None,
            };
            return (StatusCode::BAD_REQUEST, render_template(template));
        }
    };

    let template = SearchReportsTemplate {
        current_user: Some(current_user),
        error: None,
        previous_query: query,
        previous_pagination: pagination,
        results,
    };

    okay(render_template(template))
}

#[derive(Debug, Deserialize)]
struct NewReportQuery {
    resource_type: ReportableResource,
    resource_id: Uuid,
}

#[derive(Template)]
#[template(path = "reports/new.html")]
struct NewReportTemplate {
    current_user: Option<User>,
    error: Option<AppError>,
    resource_type: ReportableResource,
    resource_type_str: String,
    resource_id: Uuid,
    previous_reason: Option<String>,
}

fn resource_type_to_string(resource_type: ReportableResource) -> String {
    match resource_type {
        ReportableResource::User => "user".to_string(),
        ReportableResource::Language => "language".to_string(),
        ReportableResource::Word => "word".to_string(),
        ReportableResource::Translation => "translation".to_string(),
        ReportableResource::Translatable => "translatable".to_string(),
        ReportableResource::WordRelation => "word_relation".to_string(),
        ReportableResource::Invite => "invite".to_string(),
        ReportableResource::Permission => "permission".to_string(),
    }
}

async fn new_report_form(
    s: Session,
    Query(query): Query<NewReportQuery>,
) -> (StatusCode, Response) {
    let user = get_user!(s);

    let template = NewReportTemplate {
        current_user: Some(user),
        error: None,
        resource_type: query.resource_type,
        resource_type_str: resource_type_to_string(query.resource_type),
        resource_id: query.resource_id,
        previous_reason: None,
    };

    okay(render_template(template))
}

#[derive(Debug, Deserialize)]
struct CreateReportFormData {
    resource_type: ReportableResource,
    resource_id: Uuid,
    reason: String,
}

async fn create_report_submit(
    s: Session,
    reports: ReportRepository,
    Form(form): Form<CreateReportFormData>,
) -> (StatusCode, Response) {
    let user = get_user!(s);

    match ensure_verified(&user) {
        Ok(_) => {},
        Err(e) => {
            let template = NewReportTemplate {
                current_user: Some(user),
                error: Some(e),
                resource_type: form.resource_type,
                resource_type_str: resource_type_to_string(form.resource_type),
                resource_id: form.resource_id,
                previous_reason: Some(form.reason.clone()),
            };
            return (StatusCode::BAD_REQUEST, render_template(template));
        }
    }

    let create_req = CreateReport {
        resource_type: form.resource_type,
        resource_id: form.resource_id,
        reason: form.reason.clone(),
    };

    match reports.create(&user, create_req).await {
        Ok(_) => {
            (
                StatusCode::SEE_OTHER,
                Redirect::to("/home?report_submitted=true").into_response(),
            )
        }
        Err(e) => {
            let template = NewReportTemplate {
                current_user: Some(user),
                error: Some(e),
                resource_type: form.resource_type,
                resource_type_str: resource_type_to_string(form.resource_type),
                resource_id: form.resource_id,
                previous_reason: Some(form.reason.clone()),
            };
            (StatusCode::BAD_REQUEST, render_template(template))
        }
    }
}

#[derive(Template)]
#[template(path = "reports/view.html")]
struct ViewReportTemplate {
    current_user: Option<User>,
    report: Report,
}

async fn view_report(
    s: Session,
    reports: ReportRepository,
    user_tags: UserTagRepository,
    Path(id): Path<Uuid>,
) -> (StatusCode, Response) {
    let current_user = match s.user() {
        Some(u) => u.clone(),
        None => {
            return (
                StatusCode::SEE_OTHER,
                axum::response::Redirect::to("/login").into_response()
            )
        }
    };

    // Check if user is mod or admin
    let is_admin = user_tags.is_admin(current_user.id).await.unwrap_or(false);
    let is_moderator = user_tags.is_moderator(current_user.id).await.unwrap_or(false);

    if !(is_admin || is_moderator) {
        return (
            StatusCode::SEE_OTHER,
            axum::response::Redirect::to("/home").into_response()
        );
    }

    let report = match reports.find_by_id(&current_user, id).await {
        Ok(report) => report,
        Err(e) => {
            return crate::controller::html::render_generic_error(s, e).await;
        }
    };

    let template = ViewReportTemplate {
        current_user: Some(current_user),
        report,
    };

    okay(render_template(template))
}
