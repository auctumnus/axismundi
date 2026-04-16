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
    controller::html::{self, okay, render_template},
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
    util::{
        AppState,
        extract_session::Session,
        search_template::{SearchTemplateArgs, make_search_layout},
    },
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

pub struct ContributorWithStats {
    pub user: User,
    pub permission: PermissionLevel,
    pub permission_id: Option<Uuid>,
    pub word_count: i64,
    pub translation_count: i64,
    pub can_edit: bool,
    pub can_delete: bool,
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Template)]
#[template(path = "languages/permissions/fragments/list_header.html")]
struct Header;

#[derive(Template)]
#[template(path = "languages/permissions/fragments/query.html")]
struct QueryTemplate {
    query: ContributionsSearch,
}

#[derive(Template)]
#[template(path = "languages/permissions/fragments/card.html")]
pub struct ContributorCard {
    pub contributor: ContributorWithStats,
    pub base_url: String,
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
    let search_action = format!("/languages/{}/contributors", language.code);
    let base_url = format!("/languages/{}", language.code);

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

    let can_edit_language = if let Some(user) = s.user() {
        permissions
            .has_permission(user.id, language.id, PermissionLevel::Editor)
            .await
            .unwrap_or(false)
    } else {
        false
    };

    let current_user_id = s.user().map(|u| u.id);

    let results = contribution_stats
        .search_top_contributors(&language.id, &search, &pagination)
        .await
        .map(|response| {
            let items = response
                .items
                .into_iter()
                .map(|record| {
                    let user = record.0;
                    let target_permission = record.2;
                    let permission_id = record.3;

                    let (can_edit, can_delete) = if let Some(current_perm) = current_user_permission
                    {
                        let is_self = current_user_id == Some(user.id);

                        if is_self {
                            let can_delete_self = current_perm != PermissionLevel::Owner;
                            (false, can_delete_self)
                        } else {
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

                    ContributorWithStats {
                        user,
                        permission: target_permission,
                        permission_id,
                        word_count: record.1.word_count,
                        translation_count: record.1.translation_count,
                        can_edit: can_edit && permission_id.is_some(),
                        can_delete: can_delete && permission_id.is_some(),
                        created_at: record.4,
                    }
                })
                .collect();
            crate::pagination::PaginatedResponse {
                items,
                total: response.total,
                offset: response.offset,
                limit: response.limit,
                has_more: response.has_more,
            }
        });

    let render_item = |contributor: &ContributorWithStats| ContributorCard {
        contributor: ContributorWithStats {
            user: contributor.user.clone(),
            permission: contributor.permission,
            permission_id: contributor.permission_id,
            word_count: contributor.word_count,
            translation_count: contributor.translation_count,
            can_edit: contributor.can_edit,
            can_delete: contributor.can_delete,
            created_at: contributor.created_at,
        },
        base_url: base_url.clone(),
    };

    let breadcrumbs = html::languages::Breadcrumb {
        language: &language,
    };
    let footer = html::languages::Footer {
        can_edit_language,
        language: &language,
    };

    let template = make_search_layout(SearchTemplateArgs {
        current_user: s.user().cloned(),
        header: Header,
        query_template: QueryTemplate {
            query: search.clone(),
        },
        query: search,
        results,
        pagination,
        search_name: "contributors",
        search_action,
        render_item,
    })
    .with_breadcrumbs(breadcrumbs)
    .with_footer(footer);

    let status = template.status();
    (status, render_template(template))
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
