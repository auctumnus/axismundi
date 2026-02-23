use askama::Template;
use axum::{
    Form, Router,
    extract::{Path, State},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use reqwest::StatusCode;
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    attempt,
    controller::html::{okay, render_template},
    err::AppError,
    get_user,
    model::{
        contribution_stats::{ContributionStatsRepository, ContributionsSearch},
        language_invites::PermissionLevel,
        language_permissions::LanguagePermissionRepository,
        languages::{Language, LanguageRepository},
        users::{User, UserRepository},
    },
    pagination::PaginatedRequest,
    util::{AppState, extract_session::Session},
};

pub fn create_router() -> (Router<AppState>, Router<AppState>) {
    let secure_routes = Router::<AppState>::new()
        .route(
            "/languages/{code}/permissions/{id}/delete",
            post(delete_permission_submit),
        )
        .route(
            "/languages/{code}/permissions/{id}/edit",
            post(edit_permission_submit),
        );
    let normal_routes = Router::<AppState>::new()
        .route("/languages/{code}/contributors", get(search_contributors))
        .route(
            "/languages/{code}/permissions/{id}/delete",
            get(delete_permission_form),
        )
        .route(
            "/languages/{code}/permissions/{id}/edit",
            get(edit_permission_form),
        );

    (secure_routes, normal_routes)
}

struct ContributorWithStats {
    user: User,
    permission: PermissionLevel,
    permission_id: Option<Uuid>,
    word_count: i64,
    translation_count: i64,
    can_edit: bool,
    can_delete: bool,
    created_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Template)]
#[template(path = "languages/permissions/contributors.html")]
struct SearchContributorsTemplate {
    current_user: Option<User>,
    language: Language,
    contributors: Vec<ContributorWithStats>,
    user_has_permission: bool,
    previous_query: ContributionsSearch,
    #[allow(dead_code)]
    previous_pagination: PaginatedRequest,
}

#[allow(clippy::match_same_arms)] // easier to read like this
async fn search_contributors(
    s: Session,
    languages: LanguageRepository,
    permissions: LanguagePermissionRepository,
    contribution_stats: ContributionStatsRepository,
    Path(code): Path<String>,
    axum::extract::Query(search): axum::extract::Query<ContributionsSearch>,
    axum::extract::Query(pagination): axum::extract::Query<PaginatedRequest>,
) -> (StatusCode, Response) {
    let language = attempt!(s, languages.find_by_code(&code).await);

    // Get current user's permission level
    let current_user_permission = if let Some(user) = s.user() {
        permissions
            .find_by_user_and_language(user.id, language.id)
            .await
            .ok()
            .flatten()
            .map(|p| p.permission)
    } else {
        None
    };

    let user_has_permission = if let Some(user) = s.user() {
        permissions
            .has_permission(user.id, language.id, PermissionLevel::Editor)
            .await
            .unwrap_or(false)
    } else {
        false
    };

    let contributor_records = attempt!(
        s,
        contribution_stats
            .search_top_contributors(&language.id, &search, &pagination,)
            .await
    );

    let current_user_id = s.user().map(|u| u.id);
    let mut contributors = Vec::new();
    for record in contributor_records.items {
        let user = record.0;
        let target_permission = record.2;
        let permission_id = record.3;

        // Determine if current user can edit/delete this contributor's permission
        let (can_edit, can_delete) = if let Some(current_perm) = current_user_permission {
            // Check if trying to modify own permission
            let is_self = current_user_id == Some(user.id);

            // Owner cannot delete/edit their own permission
            // Users can delete their own permission (except Owner)
            if is_self {
                let can_delete_self = current_perm != PermissionLevel::Owner;
                (false, can_delete_self)
            } else {
                // Check permission table from language_permissions.rs
                let can_modify = match (current_perm, target_permission) {
                    (PermissionLevel::Owner, PermissionLevel::Owner) => false,
                    (PermissionLevel::Owner, _) => true,
                    (PermissionLevel::Admin, PermissionLevel::Editor) => true,
                    (PermissionLevel::Admin, PermissionLevel::Viewer) => true,
                    _ => false,
                };
                (can_modify, can_modify)
            }
        } else {
            (false, false)
        };

        contributors.push(ContributorWithStats {
            user,
            permission: target_permission,
            permission_id,
            word_count: record.1.word_count,
            translation_count: record.1.translation_count,
            can_edit: can_edit && permission_id.is_some(),
            can_delete: can_delete && permission_id.is_some(),
            created_at: record.4,
        });
    }

    let template = SearchContributorsTemplate {
        current_user: s.user().cloned(),
        language,
        contributors,
        user_has_permission,
        previous_query: search,
        previous_pagination: pagination,
    };

    let body = render_template(template);
    okay(body)
}

// Delete permission handlers

#[derive(Template)]
#[template(path = "languages/permissions/delete.html")]
struct DeletePermissionTemplate {
    current_user: Option<User>,
    language: Language,
    permission: crate::model::language_permissions::LanguagePermission,
    target_user: User,
    user_has_permission: bool,
    will_create_audit_log: bool,
}

async fn delete_permission_form(
    s: Session,
    State(state): State<AppState>,
    languages: LanguageRepository,
    permissions: LanguagePermissionRepository,
    users: UserRepository,
    Path((code, id)): Path<(String, Uuid)>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&code).await);
    let permission = attempt!(s, permissions.find_by_id(id).await);

    if permission.language != language.id {
        return crate::controller::html::render_generic_error(
            s,
            crate::err::not_found("Permission not found"),
        )
        .await;
    }

    let target_user = attempt!(s, users.find_by_id(permission.user).await);

    let is_admin_or_mod = crate::util::is_admin_or_mod(&state, user.id)
        .await
        .unwrap_or(false);

    let user_has_permission = is_admin_or_mod
        || attempt!(
            s,
            permissions
                .has_permission(user.id, language.id, PermissionLevel::Editor)
                .await
        );

    let will_create_audit_log =
        crate::util::will_create_audit_log_for_language(&state, &user, language.id).await;

    let template = DeletePermissionTemplate {
        current_user: Some(user),
        language,
        permission,
        target_user,
        user_has_permission,
        will_create_audit_log,
    };

    okay(render_template(template))
}

async fn delete_permission_submit(
    s: Session,
    languages: LanguageRepository,
    permissions: LanguagePermissionRepository,
    Path((code, id)): Path<(String, Uuid)>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&code).await);
    let permission = attempt!(s, permissions.find_by_id(id).await);

    if permission.language != language.id {
        return crate::controller::html::render_generic_error(
            s,
            crate::err::not_found("Permission not found"),
        )
        .await;
    }

    attempt!(s, permissions.delete_checked(&user, id).await);

    (
        StatusCode::SEE_OTHER,
        Redirect::to(&format!("/languages/{}/contributors", code)).into_response(),
    )
}

// Edit permission handlers

#[derive(Template)]
#[template(path = "languages/permissions/edit.html")]
struct EditPermissionTemplate {
    current_user: Option<User>,
    language: Language,
    permission: crate::model::language_permissions::LanguagePermission,
    target_user: User,
    can_grant_owner: bool,
    user_has_permission: bool,
    error: Option<AppError>,
    will_create_audit_log: bool,
}

async fn edit_permission_form(
    s: Session,
    State(state): State<AppState>,
    languages: LanguageRepository,
    permissions: LanguagePermissionRepository,
    users: UserRepository,
    Path((code, id)): Path<(String, Uuid)>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&code).await);
    let permission = attempt!(s, permissions.find_by_id(id).await);

    if permission.language != language.id {
        return crate::controller::html::render_generic_error(
            s,
            crate::err::not_found("Permission not found"),
        )
        .await;
    }

    let target_user = attempt!(s, users.find_by_id(permission.user).await);

    let is_admin_or_mod = crate::util::is_admin_or_mod(&state, user.id)
        .await
        .unwrap_or(false);

    let can_grant_owner = is_admin_or_mod
        || attempt!(
            s,
            permissions
                .has_permission(user.id, language.id, PermissionLevel::Owner)
                .await
        );

    let user_has_permission = is_admin_or_mod
        || attempt!(
            s,
            permissions
                .has_permission(user.id, language.id, PermissionLevel::Editor)
                .await
        );

    let will_create_audit_log =
        crate::util::will_create_audit_log_for_language(&state, &user, language.id).await;

    let template = EditPermissionTemplate {
        current_user: Some(user),
        language,
        permission,
        target_user,
        can_grant_owner,
        user_has_permission,
        error: None,
        will_create_audit_log,
    };

    okay(render_template(template))
}

#[derive(Deserialize)]
struct EditPermissionFormData {
    permission: PermissionLevel,
}

async fn edit_permission_submit(
    s: Session,
    State(state): State<AppState>,
    languages: LanguageRepository,
    permissions: LanguagePermissionRepository,
    users: UserRepository,
    Path((code, id)): Path<(String, Uuid)>,
    Form(form): Form<EditPermissionFormData>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&code).await);
    let permission = attempt!(s, permissions.find_by_id(id).await);

    if permission.language != language.id {
        return crate::controller::html::render_generic_error(
            s,
            crate::err::not_found("Permission not found"),
        )
        .await;
    }

    let is_admin_or_mod = crate::util::is_admin_or_mod(&state, user.id)
        .await
        .unwrap_or(false);

    let can_grant_owner = is_admin_or_mod
        || attempt!(
            s,
            permissions
                .has_permission(user.id, language.id, PermissionLevel::Owner)
                .await
        );

    let user_has_permission = is_admin_or_mod
        || attempt!(
            s,
            permissions
                .has_permission(user.id, language.id, PermissionLevel::Editor)
                .await
        );

    let will_create_audit_log =
        crate::util::will_create_audit_log_for_language(&state, &user, language.id).await;

    match permissions
        .update_permission_checked(&user, id, form.permission)
        .await
    {
        Ok(_) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!("/languages/{}/contributors", code)).into_response(),
        ),
        Err(e) => {
            let target_user = attempt!(s, users.find_by_id(permission.user).await);
            let template = EditPermissionTemplate {
                current_user: Some(user),
                language,
                permission,
                target_user,
                can_grant_owner,
                user_has_permission,
                error: Some(e),
                will_create_audit_log,
            };
            (StatusCode::BAD_REQUEST, render_template(template))
        }
    }
}
