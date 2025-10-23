use crate::{
    err::{AppResult, bad_request, forbidden, not_found, unauthorized_no_session},
    model::{
        language::LanguageRepository,
        language_invite::{
            CreateLanguageInvite, LanguageInvite, LanguageInviteRepository, PermissionLevel,
        },
        language_permission::{CreateLanguagePermission, LanguagePermissionRepository},
        user::UserRepository,
    },
    util::extract_session::Session,
};
use axum::{Json, extract::Path, http::StatusCode};
use serde::Deserialize;

type ApiResponse<T> = AppResult<T>;

#[derive(Deserialize)]
pub struct InviteRequest {
    pub permission_level: PermissionLevel,
}
// the match is more readable here imo
#[allow(clippy::match_like_matches_macro)]
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
    let sender_perm = permissions
        .find_by_user_and_language(session.user_id, language.id)
        .await?;

    let Some(sender) = sender_perm else {
        return Err(forbidden("you don't have permission to invite users"));
    };

    let recipient = users.find_by_username(&username).await?;

    // check if user already has permissions
    let existing = permissions
        .find_by_user_and_language(recipient.id, language.id)
        .await?;
    if existing.is_some() {
        return Err(bad_request(
            "user already has permissions for this language",
        ));
    }

    // check if invite already exists
    let existing_invites = invites.list_by_language(language.id).await?;
    if existing_invites
        .iter()
        .any(|i| i.recipient == recipient.id && i.accepted_at.is_none())
    {
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

    invites
        .create(
            CreateLanguageInvite {
                language: language.id,
                recipient: recipient.id,
                permissions: req.permission_level,
            },
            session.user_id,
        )
        .await
        .map(Json)
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
    let invite = existing_invites
        .iter()
        .find(|i| i.recipient == recipient.id && i.accepted_at.is_none());

    let Some(invite) = invite else {
        return Err(not_found("invite not found"));
    };

    // if the recipient is deleting (rejecting) their own invite
    if session.user_id == recipient.id {
        invites.delete(invite.id).await?;
        return Ok(StatusCode::NO_CONTENT);
    }

    // otherwise check sender permissions
    let sender_perm = permissions
        .find_by_user_and_language(session.user_id, language.id)
        .await?;

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
    let invite = existing_invites
        .iter()
        .find(|i| i.recipient == session.user_id && i.accepted_at.is_none());

    let Some(invite) = invite else {
        return Err(not_found("no pending invite found"));
    };

    // create permission
    permissions
        .create(
            CreateLanguagePermission {
                language: language.id,
                user: session.user_id,
                permission: invite.permissions,
                via: Some(invite.id),
            },
            invite.sender,
        )
        .await?;

    // mark invite as accepted
    invites.accept(invite.id).await?;

    Ok(StatusCode::OK)
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use serde_json::json;
    use std::sync::Arc;
    use tower::Service;

    use crate::controller::api::tests::{delete_without_auth, make_authed_user, post};
    use crate::email::tests::MockEmailService;

    #[tokio::test]
    async fn test_invite_user_to_language() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let owner_username = crate::tests::random_name();
        let owner_token = make_authed_user(&owner_username, &app, email_service.clone()).await;

        let invitee_username = crate::tests::random_name();
        let _invitee_token = make_authed_user(&invitee_username, &app, email_service.clone()).await;

        let lang_code = crate::tests::random_code();
        let body = json!({
            "code": lang_code,
            "name": "Test Language",
        });

        let request = post(&owner_token, "languages", body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let invite_body = json!({
            "permission_level": "editor",
        });

        let request = post(
            &owner_token,
            &format!("languages/{lang_code}/invites/{invitee_username}"),
            invite_body,
        )
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(body["permissions"], "editor");
    }

    #[tokio::test]
    async fn test_invite_user_unauthorized() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let owner_username = crate::tests::random_name();
        let owner_token = make_authed_user(&owner_username, &app, email_service.clone()).await;

        let invitee_username = crate::tests::random_name();
        let _invitee_token = make_authed_user(&invitee_username, &app, email_service.clone()).await;

        let lang_code = crate::tests::random_code();
        let body = json!({
            "code": lang_code,
            "name": "Test Language",
        });

        let request = post(&owner_token, "languages", body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let invite_body = json!({
            "permission_level": "editor",
        });

        let request = crate::controller::api::tests::post_without_auth(
            &format!("languages/{lang_code}/invites/{invitee_username}"),
            invite_body,
        )
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_invite_user_already_has_permissions() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let owner_username = crate::tests::random_name();
        let owner_token = make_authed_user(&owner_username, &app, email_service.clone()).await;

        let lang_code = crate::tests::random_code();
        let body = json!({
            "code": lang_code,
            "name": "Test Language",
        });

        let request = post(&owner_token, "languages", body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // try to invite the owner (who already has permissions)
        let invite_body = json!({
            "permission_level": "editor",
        });

        let request = post(
            &owner_token,
            &format!("languages/{lang_code}/invites/{owner_username}"),
            invite_body,
        )
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_accept_language_invite() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let owner_username = crate::tests::random_name();
        let owner_token = make_authed_user(&owner_username, &app, email_service.clone()).await;

        let invitee_username = crate::tests::random_name();
        let invitee_token = make_authed_user(&invitee_username, &app, email_service.clone()).await;

        let lang_code = crate::tests::random_code();
        let body = json!({
            "code": lang_code,
            "name": "Test Language",
        });

        let request = post(&owner_token, "languages", body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // send invite
        let invite_body = json!({
            "permission_level": "editor",
        });

        let request = post(
            &owner_token,
            &format!("languages/{lang_code}/invites/{invitee_username}"),
            invite_body,
        )
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // accept invite
        let request = post(
            &invitee_token,
            &format!("languages/{lang_code}/accept-invite"),
            json!({}),
        )
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_accept_language_invite_unauthorized() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let owner_username = crate::tests::random_name();
        let owner_token = make_authed_user(&owner_username, &app, email_service.clone()).await;

        let lang_code = crate::tests::random_code();
        let body = json!({
            "code": lang_code,
            "name": "Test Language",
        });

        let request = post(&owner_token, "languages", body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let request = crate::controller::api::tests::post_without_auth(
            &format!("languages/{lang_code}/accept-invite"),
            json!({}),
        )
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_accept_language_invite_not_found() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let owner_username = crate::tests::random_name();
        let owner_token = make_authed_user(&owner_username, &app, email_service.clone()).await;

        let other_username = crate::tests::random_name();
        let other_token = make_authed_user(&other_username, &app, email_service.clone()).await;

        let lang_code = crate::tests::random_code();
        let body = json!({
            "code": lang_code,
            "name": "Test Language",
        });

        let request = post(&owner_token, "languages", body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // try to accept invite that doesn't exist
        let request = post(
            &other_token,
            &format!("languages/{lang_code}/accept-invite"),
            json!({}),
        )
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_delete_language_invite() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let owner_username = crate::tests::random_name();
        let owner_token = make_authed_user(&owner_username, &app, email_service.clone()).await;

        let invitee_username = crate::tests::random_name();
        let _invitee_token = make_authed_user(&invitee_username, &app, email_service.clone()).await;

        let lang_code = crate::tests::random_code();
        let body = json!({
            "code": lang_code,
            "name": "Test Language",
        });

        let request = post(&owner_token, "languages", body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // send invite
        let invite_body = json!({
            "permission_level": "editor",
        });

        let request = post(
            &owner_token,
            &format!("languages/{lang_code}/invites/{invitee_username}"),
            invite_body,
        )
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // delete invite
        let request = crate::controller::api::tests::delete(
            &owner_token,
            &format!("languages/{lang_code}/invites/{invitee_username}"),
        );
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn test_delete_language_invite_unauthorized() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let owner_username = crate::tests::random_name();
        let owner_token = make_authed_user(&owner_username, &app, email_service.clone()).await;

        let invitee_username = crate::tests::random_name();
        let _invitee_token = make_authed_user(&invitee_username, &app, email_service.clone()).await;

        let lang_code = crate::tests::random_code();
        let body = json!({
            "code": lang_code,
            "name": "Test Language",
        });

        let request = post(&owner_token, "languages", body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // send invite
        let invite_body = json!({
            "permission_level": "editor",
        });

        let request = post(
            &owner_token,
            &format!("languages/{lang_code}/invites/{invitee_username}"),
            invite_body,
        )
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // try to delete without auth
        let request =
            delete_without_auth(&format!("languages/{lang_code}/invites/{invitee_username}"));
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_delete_language_invite_recipient_can_reject() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let owner_username = crate::tests::random_name();
        let owner_token = make_authed_user(&owner_username, &app, email_service.clone()).await;

        let invitee_username = crate::tests::random_name();
        let invitee_token = make_authed_user(&invitee_username, &app, email_service.clone()).await;

        let lang_code = crate::tests::random_code();
        let body = json!({
            "code": lang_code,
            "name": "Test Language",
        });

        let request = post(&owner_token, "languages", body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // send invite
        let invite_body = json!({
            "permission_level": "editor",
        });

        let request = post(
            &owner_token,
            &format!("languages/{lang_code}/invites/{invitee_username}"),
            invite_body,
        )
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // recipient deletes (rejects) invite
        let request = crate::controller::api::tests::delete(
            &invitee_token,
            &format!("languages/{lang_code}/invites/{invitee_username}"),
        );
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }
}
