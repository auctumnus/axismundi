use crate::{
    err::{bad_request, forbidden, not_found, unauthorized_no_session, AppResult},
    model::{
        language::LanguageRepository,
        language_invite::{CreateLanguageInvite, LanguageInvite, LanguageInviteRepository, PermissionLevel},
        language_permission::{CreateLanguagePermission, LanguagePermissionRepository},
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

#[derive(Deserialize)]
pub struct InviteRequest {
    pub permission_level: PermissionLevel,
}

pub async fn invite_user_to_language(
    s: Session,
    languages: LanguageRepository,
    permissions: LanguagePermissionRepository,
    invites: LanguageInviteRepository,
    users: UserRepository,
    Path((code, username)): Path<(String, String)>,
    Json(req): Json<InviteRequest>,
) -> ApiResponse<Json<LanguageInvite>> {
    let Some(session) = s.session() else {
        return Err(unauthorized_no_session());
    };

    let language = languages.find_by_code(&code).await?;
    let sender_perm = permissions.find_by_user_and_language(session.user_id, language.id).await?;

    let Some(sender) = sender_perm else {
        return Err(forbidden("you don't have permission to invite users"));
    };

    let recipient = users.find_by_username(&username).await?;

    // check if user already has permissions
    let existing = permissions.find_by_user_and_language(recipient.id, language.id).await?;
    if existing.is_some() {
        return Err(bad_request("user already has permissions for this language"));
    }

    // check if invite already exists
    let existing_invites = invites.list_by_language(language.id).await?;
    if existing_invites.iter().any(|i| i.recipient == recipient.id && i.accepted_at.is_none()) {
        return Err(bad_request("invite already exists for this user"));
    }

    // check permission to invite
    let can_invite = match (sender.permission, req.permission_level) {
        (PermissionLevel::Owner, _) => true,
        (PermissionLevel::Admin, PermissionLevel::Editor) => true,
        _ => false,
    };

    if !can_invite {
        return Err(forbidden("you don't have permission to send this invite"));
    }

    invites.create(
        CreateLanguageInvite {
            language: language.id,
            recipient: recipient.id,
            permissions: req.permission_level,
        },
        session.user_id,
    ).await.map(Json)
}

pub async fn delete_language_invite(
    s: Session,
    languages: LanguageRepository,
    permissions: LanguagePermissionRepository,
    invites: LanguageInviteRepository,
    users: UserRepository,
    Path((code, username)): Path<(String, String)>,
) -> ApiResponse<StatusCode> {
    let Some(session) = s.session() else {
        return Err(unauthorized_no_session());
    };

    let language = languages.find_by_code(&code).await?;
    let recipient = users.find_by_username(&username).await?;

    let existing_invites = invites.list_by_language(language.id).await?;
    let invite = existing_invites.iter().find(|i| i.recipient == recipient.id && i.accepted_at.is_none());

    let Some(invite) = invite else {
        return Err(not_found("invite not found"));
    };

    // if the recipient is deleting (rejecting) their own invite
    if session.user_id == recipient.id {
        invites.delete(invite.id).await?;
        return Ok(StatusCode::NO_CONTENT);
    }

    // otherwise check sender permissions
    let sender_perm = permissions.find_by_user_and_language(session.user_id, language.id).await?;

    let Some(sender) = sender_perm else {
        return Err(forbidden("you don't have permission to delete invites"));
    };

    let can_delete = match sender.permission {
        PermissionLevel::Owner => true,
        PermissionLevel::Admin => invite.sender != language.created_by,
        _ => false,
    };

    if !can_delete {
        return Err(forbidden("you don't have permission to delete this invite"));
    }

    invites.delete(invite.id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn accept_language_invite(
    s: Session,
    languages: LanguageRepository,
    permissions: LanguagePermissionRepository,
    invites: LanguageInviteRepository,
    Path(code): Path<String>,
) -> ApiResponse<StatusCode> {
    let Some(session) = s.session() else {
        return Err(unauthorized_no_session());
    };

    let language = languages.find_by_code(&code).await?;

    let existing_invites = invites.list_by_language(language.id).await?;
    let invite = existing_invites.iter().find(|i| i.recipient == session.user_id && i.accepted_at.is_none());

    let Some(invite) = invite else {
        return Err(not_found("no pending invite found"));
    };

    // create permission
    permissions.create(
        CreateLanguagePermission {
            language: language.id,
            user: session.user_id,
            permission: invite.permissions,
            via: Some(invite.id),
        },
        invite.sender,
    ).await?;

    // mark invite as accepted
    invites.accept(invite.id).await?;

    Ok(StatusCode::OK)
}
