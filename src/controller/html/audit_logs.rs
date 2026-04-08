use askama::Template;
use axum::{
    Router,
    extract::{Path, Query},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    controller::html::{okay, render_template},
    err::AppError,
    model::{
        audit_log::{
            AuditActionType, AuditLog, AuditLogFilter, AuditLogRepository, AuditableResource,
        },
        user_tags::UserTagRepository,
        users::{User, UserRepository},
    },
    pagination::{PaginatedRequest, PaginatedResponse},
    util::{AppState, BackQuery, extract_session::Session},
};

pub fn create_router() -> (Router<AppState>, Router<AppState>) {
    let secure_routes = Router::new();
    let normal_routes = Router::new()
        .route("/admin/audit-log", get(search_audit_log))
        .route("/admin/audit-log/{id}", get(view_audit_log));

    (secure_routes, normal_routes)
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
struct AuditLogSearchQuery {
    username: Option<String>,
    action: Option<AuditActionType>,
    resource_type: Option<AuditableResource>,
    resource_id: Option<Uuid>,
}

#[derive(Template)]
#[template(path = "audit_log/search.html")]
struct SearchAuditLogTemplate {
    current_user: Option<User>,
    error: Option<AppError>,
    previous_query: AuditLogSearchQuery,
    previous_pagination: PaginatedRequest,
    results: Option<PaginatedResponse<AuditLog>>,
}

async fn search_audit_log(
    s: Session,
    audit_logs: AuditLogRepository,
    user_tags: UserTagRepository,
    users: UserRepository,
    Query(query): Query<AuditLogSearchQuery>,
    Query(pagination): Query<PaginatedRequest>,
) -> (StatusCode, Response) {
    let current_user = match s.user() {
        Some(u) => u.clone(),
        None => {
            return (
                StatusCode::SEE_OTHER,
                axum::response::Redirect::to("/login").into_response(),
            );
        }
    };

    // Check if user is mod or admin
    let is_admin = user_tags.is_admin(current_user.id).await.unwrap_or(false);
    let is_moderator = user_tags
        .is_moderator(current_user.id)
        .await
        .unwrap_or(false);

    if !(is_admin || is_moderator) {
        return (
            StatusCode::SEE_OTHER,
            axum::response::Redirect::to("/home").into_response(),
        );
    }

    // Resolve username to user_id if provided
    let user_id = if let Some(username) = &query.username {
        match users.find_by_username(username).await {
            Ok(user) => Some(user.id),
            Err(_) => None, // Username not found, will return no results
        }
    } else {
        None
    };

    let filter = AuditLogFilter {
        user_id,
        action: query.action,
        resource_type: query.resource_type,
        resource_id: query.resource_id,
    };

    let results = match audit_logs
        .search(&current_user, pagination.clone(), filter)
        .await
    {
        Ok(res) => Some(res),
        Err(e) => {
            let template = SearchAuditLogTemplate {
                current_user: Some(current_user),
                error: Some(e),
                previous_query: query,
                previous_pagination: pagination,
                results: None,
            };
            return (StatusCode::BAD_REQUEST, render_template(template));
        }
    };

    let template = SearchAuditLogTemplate {
        current_user: Some(current_user),
        error: None,
        previous_query: query,
        previous_pagination: pagination,
        results,
    };

    okay(render_template(template))
}

#[derive(Template)]
#[template(path = "audit_log/view.html")]
struct ViewAuditLogTemplate {
    current_user: Option<User>,
    entry: AuditLog,
    back: String,
}

async fn view_audit_log(
    s: Session,
    audit_logs: AuditLogRepository,
    user_tags: UserTagRepository,
    Path(id): Path<Uuid>,
    Query(back_query): Query<BackQuery>,
) -> (StatusCode, Response) {
    let current_user = match s.user() {
        Some(u) => u.clone(),
        None => {
            return (
                StatusCode::SEE_OTHER,
                axum::response::Redirect::to("/login").into_response(),
            );
        }
    };

    // Check if user is mod or admin
    let is_admin = user_tags.is_admin(current_user.id).await.unwrap_or(false);
    let is_moderator = user_tags
        .is_moderator(current_user.id)
        .await
        .unwrap_or(false);

    if !(is_admin || is_moderator) {
        return (
            StatusCode::SEE_OTHER,
            axum::response::Redirect::to("/home").into_response(),
        );
    }

    let entry = match audit_logs.find_by_id(&current_user, id).await {
        Ok(entry) => entry,
        Err(e) => {
            return crate::controller::html::render_generic_error(s, e).await;
        }
    };

    let template = ViewAuditLogTemplate {
        current_user: Some(current_user),
        entry,
        back: back_query.back.unwrap_or_default(),
    };

    okay(render_template(template))
}
