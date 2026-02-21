use askama::Template;
use axum::{
    Form, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    attempt,
    controller::html::{okay, render_generic_error, render_template},
    err::AppError,
    get_user,
    model::{
        language_invites::{
            CreateLanguageInvite, InviteSearch, LanguageInvite, LanguageInviteRepository,
            PermissionLevel,
        },
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
            "/languages/{code}/invites/new",
            post(create_invitation_submit),
        )
        .route(
            "/languages/{code}/invites/{id}/revoke",
            post(revoke_invitation_submit),
        )
        .route(
            "/languages/{code}/invites/accept",
            post(accept_invitation_submit),
        )
        .route(
            "/languages/{code}/invites/dismiss",
            post(dismiss_invitation_submit),
        );

    let normal_routes = Router::<AppState>::new()
        .route("/languages/{code}/invites", get(list_invites))
        .route("/languages/{code}/invites/new", get(new_invitation_form))
        .route(
            "/languages/{code}/invites/{id}/revoke",
            get(revoke_invitation_form),
        );

    (secure_routes, normal_routes)
}

#[derive(Template)]
#[template(path = "languages/invites/new.html")]
struct NewInvitationTemplate {
    current_user: Option<User>,
    language: Language,
    error: Option<AppError>,
    can_grant_owner: bool,
    user_has_permission: bool,
    will_create_audit_log: bool,
}

async fn new_invitation_form(
    s: Session,
    State(state): State<AppState>,
    languages: LanguageRepository,
    permissions: LanguagePermissionRepository,
    Path(code): Path<String>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&code).await);

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
                .has_permission(user.id, language.id, PermissionLevel::Admin)
                .await
        );

    if !user_has_permission && !can_grant_owner {
        return render_generic_error(
            s,
            crate::err::forbidden("Only admins and owners can invite users"),
        )
        .await;
    }

    let will_create_audit_log =
        crate::util::will_create_audit_log_for_language(&state, &user, language.id).await;

    let template = NewInvitationTemplate {
        current_user: Some(user),
        language,
        error: None,
        can_grant_owner,
        user_has_permission: user_has_permission || can_grant_owner,
        will_create_audit_log,
    };

    okay(render_template(template))
}

#[derive(Deserialize)]
struct NewInvitationFormData {
    recipient: String,
    permissions: PermissionLevel,
}

async fn create_invitation_submit(
    s: Session,
    State(state): State<AppState>,
    languages: LanguageRepository,
    invites: LanguageInviteRepository,
    permissions: LanguagePermissionRepository,
    Path(code): Path<String>,
    Form(form): Form<NewInvitationFormData>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&code).await);

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
                .has_permission(user.id, language.id, PermissionLevel::Admin)
                .await
        );

    if !user_has_permission && !can_grant_owner {
        return render_generic_error(
            s,
            crate::err::forbidden("Only admins and owners can invite users"),
        )
        .await;
    }

    let will_create_audit_log =
        crate::util::will_create_audit_log_for_language(&state, &user, language.id).await;

    match invites
        .create(
            CreateLanguageInvite {
                language: code.clone(),
                recipient: form.recipient.clone(),
                permissions: form.permissions,
            },
            user.id,
        )
        .await
    {
        Ok(_) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!("/languages/{}/invites", code)).into_response(),
        ),
        Err(e) => {
            let template = NewInvitationTemplate {
                current_user: Some(user),
                language,
                error: Some(e),
                can_grant_owner,
                user_has_permission: user_has_permission || can_grant_owner,
                will_create_audit_log,
            };
            (StatusCode::BAD_REQUEST, render_template(template))
        }
    }
}

struct InviteWithUsers {
    invite: LanguageInvite,
    inviting_user: User,
    invited_user: User,
    can_revoke: bool,
}

#[derive(Template)]
#[template(path = "languages/invites/list.html")]
struct ListInvitesTemplate {
    current_user: Option<User>,
    language: Language,
    invites: Vec<InviteWithUsers>,
    user_has_permission: bool,
}

async fn list_invites(
    s: Session,
    languages: LanguageRepository,
    invites: LanguageInviteRepository,
    users: UserRepository,
    permissions: LanguagePermissionRepository,
    Path(code): Path<String>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&code).await);

    let user_has_permission = attempt!(
        s,
        permissions
            .has_permission(user.id, language.id, PermissionLevel::Admin)
            .await
    );

    if !user_has_permission {
        return render_generic_error(
            s,
            crate::err::forbidden("Only admins and owners can view invites"),
        )
        .await;
    }

    let is_owner = attempt!(
        s,
        permissions
            .has_permission(user.id, language.id, PermissionLevel::Owner)
            .await
    );

    let invites_response = attempt!(
        s,
        invites
            .search(
                &user,
                language.id,
                PaginatedRequest {
                    limit: 100,
                    offset: 0,
                },
                InviteSearch {
                    sender: None,
                    recipient: None,
                    created_before: None,
                    created_after: None,
                    accepted_before: None,
                    accepted_after: None,
                }
            )
            .await
    );

    let mut invites_with_users = Vec::new();
    for invite in invites_response.items {
        let inviting_user = attempt!(s, users.find_by_id(invite.sender).await);
        let invited_user = attempt!(s, users.find_by_id(invite.recipient).await);

        // Can revoke if owner, or if admin and it's their own invite
        let can_revoke = is_owner || (user_has_permission && invite.sender == user.id);

        invites_with_users.push(InviteWithUsers {
            invite,
            inviting_user,
            invited_user,
            can_revoke,
        });
    }

    let template = ListInvitesTemplate {
        current_user: Some(user),
        language,
        invites: invites_with_users,
        user_has_permission,
    };

    okay(render_template(template))
}

#[derive(Template)]
#[template(path = "languages/invites/revoke.html")]
struct RevokeInvitationTemplate {
    current_user: Option<User>,
    language: Language,
    invite: LanguageInvite,
    invited_user: User,
    user_has_permission: bool,
    will_create_audit_log: bool,
}

async fn revoke_invitation_form(
    s: Session,
    State(state): State<AppState>,
    languages: LanguageRepository,
    invites: LanguageInviteRepository,
    users: UserRepository,
    permissions: LanguagePermissionRepository,
    Path((code, invite_id)): Path<(String, Uuid)>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&code).await);
    let invite = attempt!(s, invites.find_by_id(invite_id).await);

    if invite.language != language.id {
        return render_generic_error(s, crate::err::not_found("Invitation not found")).await;
    }

    let is_admin_or_mod = crate::util::is_admin_or_mod(&state, user.id)
        .await
        .unwrap_or(false);

    let user_has_permission = is_admin_or_mod
        || attempt!(
            s,
            permissions
                .has_permission(user.id, language.id, PermissionLevel::Admin)
                .await
        );

    let is_owner = is_admin_or_mod
        || attempt!(
            s,
            permissions
                .has_permission(user.id, language.id, PermissionLevel::Owner)
                .await
        );

    let can_revoke = is_owner || (user_has_permission && invite.sender == user.id);

    if !can_revoke {
        return render_generic_error(
            s,
            crate::err::forbidden("You cannot revoke this invitation"),
        )
        .await;
    }

    let invited_user = attempt!(s, users.find_by_id(invite.recipient).await);

    let will_create_audit_log =
        crate::util::will_create_audit_log_for_language(&state, &user, language.id).await;

    let template = RevokeInvitationTemplate {
        current_user: Some(user),
        language,
        invite,
        invited_user,
        user_has_permission: user_has_permission || is_owner,
        will_create_audit_log,
    };

    okay(render_template(template))
}

async fn revoke_invitation_submit(
    s: Session,
    languages: LanguageRepository,
    invites: LanguageInviteRepository,
    permissions: LanguagePermissionRepository,
    Path((code, invite_id)): Path<(String, Uuid)>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&code).await);
    let invite = attempt!(s, invites.find_by_id(invite_id).await);

    if invite.language != language.id {
        return render_generic_error(s, crate::err::not_found("Invitation not found")).await;
    }

    let user_has_permission = attempt!(
        s,
        permissions
            .has_permission(user.id, language.id, PermissionLevel::Admin)
            .await
    );

    let is_owner = attempt!(
        s,
        permissions
            .has_permission(user.id, language.id, PermissionLevel::Owner)
            .await
    );

    let can_revoke = is_owner || (user_has_permission && invite.sender == user.id);

    if !can_revoke {
        return render_generic_error(
            s,
            crate::err::forbidden("You cannot revoke this invitation"),
        )
        .await;
    }

    attempt!(
        s,
        invites.delete(&user, language.id, invite.recipient).await
    );

    (
        StatusCode::SEE_OTHER,
        Redirect::to(&format!("/languages/{}/invites", code)).into_response(),
    )
}

async fn accept_invitation_submit(
    s: Session,
    languages: LanguageRepository,
    invites: LanguageInviteRepository,
    Path(code): Path<String>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&code).await);

    match invites.accept(&user, language.id).await {
        Ok(_) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!("/languages/{}", code)).into_response(),
        ),
        Err(e) => render_generic_error(s, e).await,
    }
}

async fn dismiss_invitation_submit(
    s: Session,
    languages: LanguageRepository,
    invites: LanguageInviteRepository,
    Path(code): Path<String>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&code).await);

    match invites.delete(&user, language.id, user.id).await {
        Ok(()) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!("/languages/{}", code)).into_response(),
        ),
        Err(e) => render_generic_error(s, e).await,
    }
}
