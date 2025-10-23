use crate::{
    err::{bad_request, forbidden, not_found, unauthorized_no_session, AppResult},
    model::{
        language::LanguageRepository,
        language_invite::PermissionLevel,
        language_permission::{LanguagePermission, LanguagePermissionRepository},
        user::UserRepository,
    },
    util::extract_session::Session,
};
use axum::{
    extract::Path,
    http::StatusCode,
    Json,
};
use serde::Deserialize;

type ApiResponse<T> = AppResult<T>;

pub async fn get_language_permissions(
    s: Session,
    languages: LanguageRepository,
    permissions: LanguagePermissionRepository,
    Path(code): Path<String>,
) -> ApiResponse<Json<Vec<LanguagePermission>>> {
    let Some(session) = s.session() else {
        return Err(unauthorized_no_session());
    };

    let language = languages.find_by_code(&code).await?;
    let user_perm = permissions.find_by_user_and_language(session.user_id, language.id).await?;

    let Some(perm) = user_perm else {
        return Err(forbidden("you don't have permission to view permissions"));
    };

    if perm.permission != PermissionLevel::Owner && perm.permission != PermissionLevel::Admin {
        return Err(forbidden("only owners and admins can view all permissions"));
    }

    permissions.list_by_language(language.id).await.map(Json)
}

pub async fn get_user_language_permissions(
    s: Session,
    languages: LanguageRepository,
    permissions: LanguagePermissionRepository,
    users: UserRepository,
    Path((code, username)): Path<(String, String)>,
) -> ApiResponse<Json<LanguagePermission>> {
    let Some(session) = s.session() else {
        return Err(unauthorized_no_session());
    };

    let language = languages.find_by_code(&code).await?;
    let user_perm = permissions.find_by_user_and_language(session.user_id, language.id).await?;

    let Some(_) = user_perm else {
        return Err(forbidden("you don't have permission to view permissions"));
    };

    let target_user = users.find_by_username(&username).await?;
    let target_perm = permissions.find_by_user_and_language(target_user.id, language.id).await?;

    target_perm.ok_or_else(|| not_found(format!("permission for user '{username}' on language '{code}'")))
        .map(Json)
}

#[derive(Deserialize)]
pub struct EditPermissionRequest {
    pub permission_level: PermissionLevel,
}

pub async fn edit_user_permissions(
    s: Session,
    languages: LanguageRepository,
    permissions: LanguagePermissionRepository,
    users: UserRepository,
    Path((code, username)): Path<(String, String)>,
    Json(req): Json<EditPermissionRequest>,
) -> ApiResponse<Json<LanguagePermission>> {
    let Some(session) = s.session() else {
        return Err(unauthorized_no_session());
    };

    let language = languages.find_by_code(&code).await?;
    let requester_perm = permissions.find_by_user_and_language(session.user_id, language.id).await?;

    let Some(requester) = requester_perm else {
        return Err(forbidden("you don't have permission to edit permissions"));
    };

    let target_user = users.find_by_username(&username).await?;
    let target_perm = permissions.find_by_user_and_language(target_user.id, language.id).await?;

    let Some(target) = target_perm else {
        return Err(bad_request("user doesn't have permissions for this language"));
    };

    // check permission table from api.md
    let can_edit = match (requester.permission, target.permission) {
        (PermissionLevel::Owner, PermissionLevel::Owner) => false,
        (PermissionLevel::Owner, _) => true,
        (PermissionLevel::Admin, PermissionLevel::Editor) => true,
        _ => false,
    };

    if !can_edit {
        return Err(forbidden("you don't have permission to edit this user's permissions"));
    }

    permissions.update_permission(target.id, req.permission_level).await.map(Json)
}

pub async fn delete_user_permissions(
    s: Session,
    languages: LanguageRepository,
    permissions: LanguagePermissionRepository,
    users: UserRepository,
    Path((code, username)): Path<(String, String)>,
) -> ApiResponse<StatusCode> {
    let Some(session) = s.session() else {
        return Err(unauthorized_no_session());
    };

    let language = languages.find_by_code(&code).await?;
    let requester_perm = permissions.find_by_user_and_language(session.user_id, language.id).await?;

    let Some(requester) = requester_perm else {
        return Err(forbidden("you don't have permission to delete permissions"));
    };

    let target_user = users.find_by_username(&username).await?;
    let target_perm = permissions.find_by_user_and_language(target_user.id, language.id).await?;

    let Some(target) = target_perm else {
        return Err(not_found("user doesn't have permissions for this language"));
    };

    // check if removing own permissions (always allowed except owner)
    if session.user_id == target_user.id {
        if requester.permission == PermissionLevel::Owner {
            return Err(forbidden("owner cannot remove their own permissions"));
        }
        permissions.delete(target.id).await?;
        return Ok(StatusCode::NO_CONTENT);
    }

    // check permission table from api.md
    let can_delete = match (requester.permission, target.permission) {
        (PermissionLevel::Owner, PermissionLevel::Owner) => false,
        (PermissionLevel::Owner, _) => true,
        (PermissionLevel::Admin, PermissionLevel::Editor) => true,
        _ => false,
    };

    if !can_delete {
        return Err(forbidden("you don't have permission to delete this user's permissions"));
    }

    permissions.delete(target.id).await?;
    Ok(StatusCode::NO_CONTENT)
}
