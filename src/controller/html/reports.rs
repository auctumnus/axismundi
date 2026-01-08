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
    controller::html::{okay, render_template, render_generic_error},
    err::AppError,
    get_user,
    model::{
        languages::{Language, LanguageRepository},
        reports::{Report, ReportSearch, ReportRepository, ReportableResource, ResolutionStatus, ReportPriority, CreateReport},
        translatable::{Translatable, TranslatableRepository},
        translations::{Translation, TranslationRepository},
        user_tags::UserTagRepository,
        users::{User, UserRepository},
        words::{Word, WordRepository},
    },
    pagination::{PaginatedRequest, PaginatedResponse},
    util::{AppState, extract_session::Session, ensure_verified},
};

pub fn create_router() -> (Router<AppState>, Router<AppState>) {
    let secure_routes = Router::new()
        .route("/report/new", post(create_report_submit))
        .route("/admin/reports/{id}/edit", post(edit_report_submit));
    let normal_routes = Router::new()
        .route("/admin/reports", get(search_reports))
        .route("/admin/reports/{id}", get(view_report))
        .route("/admin/reports/{id}/edit", get(edit_report_form))
        .route("/report/new", get(new_report_form))
        .route("/my-reports", get(my_reports));

    (secure_routes, normal_routes)
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ReportableResourceData {
    User { user: User },
    Language { language: Language },
    Word { word: Word },
    Translation { translation: Translation },
    Translatable { translatable: Translatable },
    // WordRelation, Invite, and Permission are more complex and less commonly reported
    // For now, we'll handle them as "other"
    Other,
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
    resource_type_str: String,
    resource_id: Uuid,
    previous_reason: Option<String>,
    resource_data: Option<ReportableResourceData>,
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
    users: UserRepository,
    languages: LanguageRepository,
    words: WordRepository,
    translations: TranslationRepository,
    translatables: TranslatableRepository,
    Query(query): Query<NewReportQuery>,
) -> (StatusCode, Response) {
    let user = get_user!(s);

    // Fetch the resource data based on type
    let resource_data = match query.resource_type {
        ReportableResource::User => {
            match users.find_by_id(query.resource_id).await {
                Ok(u) => Some(ReportableResourceData::User { user: u }),
                Err(e) => return render_generic_error(s, e).await,
            }
        }
        ReportableResource::Language => {
            match languages.find_by_id(query.resource_id).await {
                Ok(l) => Some(ReportableResourceData::Language { language: l }),
                Err(e) => return render_generic_error(s, e).await,
            }
        }
        ReportableResource::Word => {
            match words.find_by_id(query.resource_id).await {
                Ok(w) => Some(ReportableResourceData::Word { word: w }),
                Err(e) => return render_generic_error(s, e).await,
            }
        }
        ReportableResource::Translation => {
            match translations.find_by_id(query.resource_id).await {
                Ok(t) => Some(ReportableResourceData::Translation { translation: t }),
                Err(e) => return render_generic_error(s, e).await,
            }
        }
        ReportableResource::Translatable => {
            match translatables.find_by_id(query.resource_id).await {
                Ok(t) => Some(ReportableResourceData::Translatable { translatable: t }),
                Err(e) => return render_generic_error(s, e).await,
            }
        }
        // For complex resources we don't have preview cards for
        _ => Some(ReportableResourceData::Other),
    };

    let template = NewReportTemplate {
        current_user: Some(user),
        error: None,
        resource_type_str: resource_type_to_string(query.resource_type),
        resource_id: query.resource_id,
        previous_reason: None,
        resource_data,
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
                resource_type_str: resource_type_to_string(form.resource_type),
                resource_id: form.resource_id,
                previous_reason: Some(form.reason.clone()),
                resource_data: None, // Skip fetching on error
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
                resource_type_str: resource_type_to_string(form.resource_type),
                resource_id: form.resource_id,
                previous_reason: Some(form.reason.clone()),
                resource_data: None, // Skip fetching on error
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
    resource_data: Option<ReportableResourceData>,
    reporter: Option<User>,
}

async fn view_report(
    s: Session,
    reports: ReportRepository,
    user_tags: UserTagRepository,
    users: UserRepository,
    languages: LanguageRepository,
    words: WordRepository,
    translations: TranslationRepository,
    translatables: TranslatableRepository,
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

    // find_by_id already handles permission checking:
    // - Mods/admins can see any report with all fields
    // - Users can only see their own reports with fields hidden
    let report = match reports.find_by_id(&current_user, id).await {
        Ok(report) => report,
        Err(e) => {
            return crate::controller::html::render_generic_error(s, e).await;
        }
    };

    // Fetch the reporter user if they exist
    let reporter = if let Some(reporter_id) = report.reporter {
        users.find_by_id(reporter_id).await.ok()
    } else {
        None
    };

    // Fetch the resource data based on type
    let resource_data = match report.resource_type {
        ReportableResource::User => {
            users.find_by_id(report.resource_id).await.ok().map(|u| ReportableResourceData::User { user: u })
        }
        ReportableResource::Language => {
            languages.find_by_id(report.resource_id).await.ok().map(|l| ReportableResourceData::Language { language: l })
        }
        ReportableResource::Word => {
            words.find_by_id(report.resource_id).await.ok().map(|w| ReportableResourceData::Word { word: w })
        }
        ReportableResource::Translation => {
            translations.find_by_id(report.resource_id).await.ok().map(|t| ReportableResourceData::Translation { translation: t })
        }
        ReportableResource::Translatable => {
            translatables.find_by_id(report.resource_id).await.ok().map(|t| ReportableResourceData::Translatable { translatable: t })
        }
        _ => Some(ReportableResourceData::Other),
    };

    let template = ViewReportTemplate {
        current_user: Some(current_user),
        report,
        resource_data,
        reporter,
    };

    okay(render_template(template))
}

#[derive(Template)]
#[template(path = "reports/edit.html")]
struct EditReportTemplate {
    current_user: Option<User>,
    error: Option<AppError>,
    report: Report,
    resource_data: Option<ReportableResourceData>,
    reporter: Option<User>,
}

async fn edit_report_form(
    s: Session,
    reports: ReportRepository,
    user_tags: UserTagRepository,
    users: UserRepository,
    languages: LanguageRepository,
    words: WordRepository,
    translations: TranslationRepository,
    translatables: TranslatableRepository,
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

    // Fetch the reporter user if they exist
    let reporter = if let Some(reporter_id) = report.reporter {
        users.find_by_id(reporter_id).await.ok()
    } else {
        None
    };

    // Fetch the resource data based on type
    let resource_data = match report.resource_type {
        ReportableResource::User => {
            users.find_by_id(report.resource_id).await.ok().map(|u| ReportableResourceData::User { user: u })
        }
        ReportableResource::Language => {
            languages.find_by_id(report.resource_id).await.ok().map(|l| ReportableResourceData::Language { language: l })
        }
        ReportableResource::Word => {
            words.find_by_id(report.resource_id).await.ok().map(|w| ReportableResourceData::Word { word: w })
        }
        ReportableResource::Translation => {
            translations.find_by_id(report.resource_id).await.ok().map(|t| ReportableResourceData::Translation { translation: t })
        }
        ReportableResource::Translatable => {
            translatables.find_by_id(report.resource_id).await.ok().map(|t| ReportableResourceData::Translatable { translatable: t })
        }
        _ => Some(ReportableResourceData::Other),
    };

    let template = EditReportTemplate {
        current_user: Some(current_user),
        error: None,
        report,
        resource_data,
        reporter,
    };

    okay(render_template(template))
}

use crate::model::reports::UpdateReportModerator;

#[derive(Debug, Deserialize)]
struct EditReportFormData {
    priority: Option<String>,
    resolution_status: Option<String>,
    resolution_note: Option<String>,
    resolution_status_hidden: Option<String>,
    resolution_note_hidden: Option<String>,
}

async fn edit_report_submit(
    s: Session,
    reports: ReportRepository,
    user_tags: UserTagRepository,
    users: UserRepository,
    languages: LanguageRepository,
    words: WordRepository,
    translations: TranslationRepository,
    translatables: TranslatableRepository,
    Path(id): Path<Uuid>,
    Form(form): Form<EditReportFormData>,
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

    // Parse the form data
    let priority = form.priority.and_then(|p| match p.as_str() {
        "low" => Some(ReportPriority::Low),
        "medium" => Some(ReportPriority::Medium),
        "high" => Some(ReportPriority::High),
        "urgent" => Some(ReportPriority::Urgent),
        _ => None,
    });

    let resolution_status = form.resolution_status.and_then(|s| match s.as_str() {
        "pending" => Some(ResolutionStatus::Pending),
        "in_progress" => Some(ResolutionStatus::InProgress),
        "dismissed" => Some(ResolutionStatus::Dismissed),
        "action_taken" => Some(ResolutionStatus::ActionTaken),
        _ => None,
    });

    let update_req = UpdateReportModerator {
        priority,
        resolution_status,
        resolution_note: form.resolution_note,
        resolution_status_hidden: form.resolution_status_hidden.map(|v| v == "true"),
        resolution_note_hidden: form.resolution_note_hidden.map(|v| v == "true"),
    };

    match reports.update(&current_user, id, update_req).await {
        Ok(_) => {
            (
                StatusCode::SEE_OTHER,
                Redirect::to(&format!("/admin/reports/{}", id)).into_response(),
            )
        }
        Err(e) => {
            // Re-fetch the report to show the form again with error
            let report = match reports.find_by_id(&current_user, id).await {
                Ok(r) => r,
                Err(_) => {
                    return crate::controller::html::render_generic_error(s, e).await;
                }
            };

            // Fetch the reporter user if they exist
            let reporter = if let Some(reporter_id) = report.reporter {
                users.find_by_id(reporter_id).await.ok()
            } else {
                None
            };

            // Fetch the resource data based on type
            let resource_data = match report.resource_type {
                ReportableResource::User => {
                    users.find_by_id(report.resource_id).await.ok().map(|u| ReportableResourceData::User { user: u })
                }
                ReportableResource::Language => {
                    languages.find_by_id(report.resource_id).await.ok().map(|l| ReportableResourceData::Language { language: l })
                }
                ReportableResource::Word => {
                    words.find_by_id(report.resource_id).await.ok().map(|w| ReportableResourceData::Word { word: w })
                }
                ReportableResource::Translation => {
                    translations.find_by_id(report.resource_id).await.ok().map(|t| ReportableResourceData::Translation { translation: t })
                }
                ReportableResource::Translatable => {
                    translatables.find_by_id(report.resource_id).await.ok().map(|t| ReportableResourceData::Translatable { translatable: t })
                }
                _ => Some(ReportableResourceData::Other),
            };

            let template = EditReportTemplate {
                current_user: Some(current_user),
                error: Some(e),
                report,
                resource_data,
                reporter,
            };

            (StatusCode::BAD_REQUEST, render_template(template))
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
struct MyReportsSearchQuery {
    text_query: Option<String>,
    resource_type: Option<ReportableResource>,
    resource_id: Option<Uuid>,
    resolution_status: Option<ResolutionStatus>,
}

#[derive(Template)]
#[template(path = "reports/my_reports.html")]
struct MyReportsTemplate {
    current_user: Option<User>,
    error: Option<AppError>,
    previous_query: MyReportsSearchQuery,
    previous_pagination: PaginatedRequest,
    results: Option<PaginatedResponse<Report>>,
}

async fn my_reports(
    s: Session,
    reports: ReportRepository,
    Query(query): Query<MyReportsSearchQuery>,
    Query(pagination): Query<PaginatedRequest>,
) -> (StatusCode, Response) {
    let user = get_user!(s);

    let search = ReportSearch {
        text_query: query.text_query.clone(),
        resource_type: query.resource_type,
        resource_id: query.resource_id,
        reporter: None, // This will be filled by search_own
        resolution_status: query.resolution_status,
        priority: None, // Users can't filter by priority
    };

    let results = match reports.search_own(&user, pagination.clone(), search).await {
        Ok(res) => Some(res),
        Err(e) => {
            let template = MyReportsTemplate {
                current_user: Some(user),
                error: Some(e),
                previous_query: query,
                previous_pagination: pagination,
                results: None,
            };
            return (StatusCode::BAD_REQUEST, render_template(template));
        }
    };

    let template = MyReportsTemplate {
        current_user: Some(user),
        error: None,
        previous_query: query,
        previous_pagination: pagination,
        results,
    };

    okay(render_template(template))
}
