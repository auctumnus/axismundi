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
        language_families::{LanguageFamily, LanguageFamilyRepository},
        language_family_invites::{
            CreateLanguageFamilyInvite, FamilyInviteSearch, LanguageFamilyInvite,
            LanguageFamilyInviteRepository,
        },
        language_family_permissions::LanguageFamilyPermissionRepository,
        language_invites::PermissionLevel,
        users::{User, UserRepository},
    },
    pagination::PaginatedRequest,
    util::{AppState, extract_session::Session},
};

pub fn create_router() -> (Router<AppState>, Router<AppState>) {
    let secure_routes = Router::<AppState>::new()
        .route(
            "/language-families/{code}/invites/new",
            post(create_invitation_submit),
        )
        .route(
            "/language-families/{code}/invites/{id}/revoke",
            post(revoke_invitation_submit),
        )
        .route(
            "/language-families/{code}/invites/accept",
            post(accept_invitation_submit),
        )
        .route(
            "/language-families/{code}/invites/dismiss",
            post(dismiss_invitation_submit),
        );

    let normal_routes = Router::<AppState>::new()
        .route("/language-families/{code}/invites", get(list_invites))
        .route(
            "/language-families/{code}/invites/new",
            get(new_invitation_form),
        )
        .route(
            "/language-families/{code}/invites/{id}/revoke",
            get(revoke_invitation_form),
        );

    (secure_routes, normal_routes)
}

#[derive(Template)]
#[template(path = "language_families/invites/new.html")]
struct NewInvitationTemplate {
    current_user: Option<User>,
    family: LanguageFamily,
    error: Option<AppError>,
    can_grant_owner: bool,
    user_has_permission: bool,
    will_create_audit_log: bool,
}

async fn new_invitation_form(
    s: Session,
    State(state): State<AppState>,
    families: LanguageFamilyRepository,
    permissions: LanguageFamilyPermissionRepository,
    Path(code): Path<String>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let family = attempt!(s, families.find_by_code(&code).await);

    let is_admin_or_mod = crate::util::is_admin_or_mod(&state, user.id)
        .await
        .unwrap_or(false);

    let can_grant_owner = is_admin_or_mod
        || attempt!(
            s,
            permissions
                .has_permission(user.id, family.id, PermissionLevel::Owner)
                .await
        );

    let user_has_permission = is_admin_or_mod
        || attempt!(
            s,
            permissions
                .has_permission(user.id, family.id, PermissionLevel::Admin)
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
        crate::util::will_create_audit_log_for_family(&state, &user, family.id).await;

    let template = NewInvitationTemplate {
        current_user: Some(user),
        family,
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
    families: LanguageFamilyRepository,
    invites: LanguageFamilyInviteRepository,
    permissions: LanguageFamilyPermissionRepository,
    Path(code): Path<String>,
    Form(form): Form<NewInvitationFormData>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let family = attempt!(s, families.find_by_code(&code).await);

    let is_admin_or_mod = crate::util::is_admin_or_mod(&state, user.id)
        .await
        .unwrap_or(false);

    let can_grant_owner = is_admin_or_mod
        || attempt!(
            s,
            permissions
                .has_permission(user.id, family.id, PermissionLevel::Owner)
                .await
        );

    let user_has_permission = is_admin_or_mod
        || attempt!(
            s,
            permissions
                .has_permission(user.id, family.id, PermissionLevel::Admin)
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
        crate::util::will_create_audit_log_for_family(&state, &user, family.id).await;

    match invites
        .create(
            user.clone(),
            CreateLanguageFamilyInvite {
                language_family: code.clone(),
                recipient: form.recipient.clone(),
                permissions: form.permissions,
            },
        )
        .await
    {
        Ok(_) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!("/language-families/{}/invites", code)).into_response(),
        ),
        Err(e) => {
            let template = NewInvitationTemplate {
                current_user: Some(user),
                family,
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
    invite: LanguageFamilyInvite,
    inviting_user: User,
    invited_user: User,
    can_revoke: bool,
    revoke_url: String,
}

#[derive(Template)]
#[template(path = "language_families/invites/list.html")]
struct ListInvitesTemplate {
    current_user: Option<User>,
    family: LanguageFamily,
    invites: Vec<InviteWithUsers>,
    user_has_permission: bool,
}

async fn list_invites(
    s: Session,
    families: LanguageFamilyRepository,
    invites: LanguageFamilyInviteRepository,
    users: UserRepository,
    permissions: LanguageFamilyPermissionRepository,
    Path(code): Path<String>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let family = attempt!(s, families.find_by_code(&code).await);

    let user_has_permission = attempt!(
        s,
        permissions
            .has_permission(user.id, family.id, PermissionLevel::Admin)
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
            .has_permission(user.id, family.id, PermissionLevel::Owner)
            .await
    );

    let invites_response = attempt!(
        s,
        invites
            .search(
                &user,
                family.id,
                PaginatedRequest {
                    limit: 100,
                    offset: 0,
                },
                FamilyInviteSearch {
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
        let revoke_url = format!("/language-families/{}/invites/{}/revoke", code, invite.id);

        invites_with_users.push(InviteWithUsers {
            invite,
            inviting_user,
            invited_user,
            can_revoke,
            revoke_url,
        });
    }

    let template = ListInvitesTemplate {
        current_user: Some(user),
        family,
        invites: invites_with_users,
        user_has_permission,
    };

    okay(render_template(template))
}

#[derive(Template)]
#[template(path = "language_families/invites/revoke.html")]
struct RevokeInvitationTemplate {
    current_user: Option<User>,
    family: LanguageFamily,
    invite: LanguageFamilyInvite,
    invited_user: User,
    user_has_permission: bool,
    will_create_audit_log: bool,
}

async fn revoke_invitation_form(
    s: Session,
    State(state): State<AppState>,
    families: LanguageFamilyRepository,
    invites: LanguageFamilyInviteRepository,
    users: UserRepository,
    permissions: LanguageFamilyPermissionRepository,
    Path((code, invite_id)): Path<(String, Uuid)>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let family = attempt!(s, families.find_by_code(&code).await);
    let invite = attempt!(s, invites.find_by_id(invite_id).await);

    if invite.family != family.id {
        return render_generic_error(s, crate::err::not_found("Invitation not found")).await;
    }

    let is_admin_or_mod = crate::util::is_admin_or_mod(&state, user.id)
        .await
        .unwrap_or(false);

    let user_has_permission = is_admin_or_mod
        || attempt!(
            s,
            permissions
                .has_permission(user.id, family.id, PermissionLevel::Admin)
                .await
        );

    let is_owner = is_admin_or_mod
        || attempt!(
            s,
            permissions
                .has_permission(user.id, family.id, PermissionLevel::Owner)
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
        crate::util::will_create_audit_log_for_family(&state, &user, family.id).await;

    let template = RevokeInvitationTemplate {
        current_user: Some(user),
        family,
        invite,
        invited_user,
        user_has_permission: user_has_permission || is_owner,
        will_create_audit_log,
    };

    okay(render_template(template))
}

async fn revoke_invitation_submit(
    s: Session,
    families: LanguageFamilyRepository,
    invites: LanguageFamilyInviteRepository,
    permissions: LanguageFamilyPermissionRepository,
    Path((code, invite_id)): Path<(String, Uuid)>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let family = attempt!(s, families.find_by_code(&code).await);
    let invite = attempt!(s, invites.find_by_id(invite_id).await);

    if invite.family != family.id {
        return render_generic_error(s, crate::err::not_found("Invitation not found")).await;
    }

    let user_has_permission = attempt!(
        s,
        permissions
            .has_permission(user.id, family.id, PermissionLevel::Admin)
            .await
    );

    let is_owner = attempt!(
        s,
        permissions
            .has_permission(user.id, family.id, PermissionLevel::Owner)
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

    attempt!(s, invites.delete(&user, family.id, invite.recipient).await);

    (
        StatusCode::SEE_OTHER,
        Redirect::to(&format!("/language-families/{}/invites", code)).into_response(),
    )
}

async fn accept_invitation_submit(
    s: Session,
    families: LanguageFamilyRepository,
    invites: LanguageFamilyInviteRepository,
    Path(code): Path<String>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let family = attempt!(s, families.find_by_code(&code).await);

    match invites.accept(&user, family.id).await {
        Ok(_) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!("/language-families/{}", code)).into_response(),
        ),
        Err(e) => render_generic_error(s, e).await,
    }
}

async fn dismiss_invitation_submit(
    s: Session,
    families: LanguageFamilyRepository,
    invites: LanguageFamilyInviteRepository,
    Path(code): Path<String>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let family = attempt!(s, families.find_by_code(&code).await);

    match invites.delete(&user, family.id, user.id).await {
        Ok(()) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!("/language-families/{}", code)).into_response(),
        ),
        Err(e) => render_generic_error(s, e).await,
    }
}
