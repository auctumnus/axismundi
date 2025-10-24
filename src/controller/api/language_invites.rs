use crate::{
    err::{AppResult, unauthorized_no_session},
    model::{
        language_invites::{
            CreateLanguageInvite, InviteSearch, LanguageInvite, LanguageInviteRepository,
            PermissionLevel,
        },
        languages::LanguageRepository,
        users::UserRepository,
    },
    pagination::{PaginatedRequest, PaginatedResponse},
    util::extract_session::Session,
};
use axum::{
    Json,
    extract::{Path, Query},
    http::StatusCode,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub fn create_router() -> axum::Router<crate::util::AppState> {
    axum::Router::new()
        .route(
            "/languages/{code}/invites/{username}",
            axum::routing::post(invite_user_to_language),
        )
        .route(
            "/languages/{code}/invites",
            axum::routing::get(search_language_invites),
        )
        .route(
            "/languages/{code}/invites/{username}",
            axum::routing::get(view_language_invite),
        )
        .route(
            "/languages/{code}/invites/{username}",
            axum::routing::delete(delete_language_invite),
        )
        .route(
            "/languages/{code}/accept-invite",
            axum::routing::post(accept_language_invite),
        )
}

type ApiResponse<T> = AppResult<T>;
type PaginatedApiResponse<T> = AppResult<PaginatedResponse<T>>;

#[derive(Deserialize)]
pub struct InviteRequest {
    pub permission_level: PermissionLevel,
}

#[derive(Serialize)]
pub struct InviteResponse {
    pub recipient: String,
    pub sender: String,
    pub permissions: PermissionLevel,
    pub sent_at: DateTime<Utc>,
    pub accepted_at: Option<DateTime<Utc>>,
}

async fn map_invite_to_response(
    invite: LanguageInvite,
    users: UserRepository,
) -> ApiResponse<InviteResponse> {
    let recipient = users.find_by_id(invite.recipient).await?;
    let sender = users.find_by_id(invite.sender).await?;

    Ok(InviteResponse {
        recipient: recipient.username,
        sender: sender.username,
        permissions: invite.permissions,
        sent_at: invite.sent_at,
        accepted_at: invite.accepted_at,
    })
}

pub async fn invite_user_to_language(
    s: Session,
    invites: LanguageInviteRepository,
    users: UserRepository,
    Path((code, username)): Path<(String, String)>,
    Json(req): Json<InviteRequest>,
) -> ApiResponse<Json<InviteResponse>> {
    let Some(session) = s.session() else {
        return Err(unauthorized_no_session());
    };

    let invite = invites
        .create(
            CreateLanguageInvite {
                language: code,
                recipient: username,
                permissions: req.permission_level,
            },
            session.user_id,
        )
        .await?;

    let response = map_invite_to_response(invite, users).await?;
    Ok(Json(response))
}

pub async fn search_language_invites(
    s: Session,
    invites: LanguageInviteRepository,
    users: UserRepository,
    languages: LanguageRepository,
    Path(code): Path<String>,
    Query(search): Query<InviteSearch>,
    Query(pagination): Query<PaginatedRequest>,
) -> PaginatedApiResponse<InviteResponse> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    let language = languages.find_by_code(&code).await?;

    let response = invites
        .search(requestor, language.id, pagination, search)
        .await?;

    let mut mapped_items = vec![];

    for invite in response.items {
        let mapped_invite = map_invite_to_response(invite, users.clone()).await?;
        mapped_items.push(mapped_invite);
    }

    Ok(PaginatedResponse {
        items: mapped_items,
        total: response.total,
        offset: response.offset,
        limit: response.limit,
        has_more: response.has_more,
    })
}

pub async fn view_language_invite(
    s: Session,
    invites: LanguageInviteRepository,
    languages: LanguageRepository,
    users: UserRepository,
    Path((code, username)): Path<(String, String)>,
) -> ApiResponse<Json<InviteResponse>> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    let language = languages.find_by_code(&code).await?;
    let recipient = users.find_by_username(&username).await?;
    let invite = invites
        .find_by_language_and_recipient(requestor, language.id, recipient.id)
        .await?;

    let invite = map_invite_to_response(invite, users).await?;

    Ok(Json(invite))
}

pub async fn delete_language_invite(
    s: Session,
    languages: LanguageRepository,
    invites: LanguageInviteRepository,
    users: UserRepository,
    Path((code, username)): Path<(String, String)>,
) -> ApiResponse<StatusCode> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    let language = languages.find_by_code(&code).await?;
    let recipient = users.find_by_username(&username).await?;

    invites.delete(requestor, language.id, recipient.id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn accept_language_invite(
    s: Session,
    languages: LanguageRepository,
    invites: LanguageInviteRepository,
    Path(code): Path<String>,
) -> ApiResponse<StatusCode> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    let language = languages.find_by_code(&code).await?;

    // mark invite as accepted
    invites.accept(requestor, language.id).await?;

    Ok(StatusCode::OK)
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use serde_json::json;
    use std::sync::Arc;
    use tower::Service;

    use crate::controller::api::tests::{
        delete_without_auth, make_authed_user, post, print_response_body,
    };
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
        let status = response.status();
        print_response_body(response).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn test_view_language_invite() {
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

        // view invite as owner
        let request = crate::controller::api::tests::get_with_auth(
            &owner_token,
            &format!("languages/{lang_code}/invites/{invitee_username}"),
        )
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(body["recipient"], invitee_username);
        assert_eq!(body["sender"], owner_username);
        assert_eq!(body["permissions"], "editor");

        // view invite as invitee
        let request = crate::controller::api::tests::get_with_auth(
            &invitee_token,
            &format!("languages/{lang_code}/invites/{invitee_username}"),
        )
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(body["recipient"], invitee_username);
    }

    #[tokio::test]
    async fn test_view_language_invite_unauthorized() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let owner_username = crate::tests::random_name();
        let owner_token = make_authed_user(&owner_username, &app, email_service.clone()).await;

        let invitee_username = crate::tests::random_name();
        let _invitee_token = make_authed_user(&invitee_username, &app, email_service.clone()).await;

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

        // try to view invite as someone else
        let request = crate::controller::api::tests::get_with_auth(
            &other_token,
            &format!("languages/{lang_code}/invites/{invitee_username}"),
        )
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_search_language_invites() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let owner_username = crate::tests::random_name();
        let owner_token = make_authed_user(&owner_username, &app, email_service.clone()).await;

        let invitee1_username = crate::tests::random_name();
        let _invitee1_token =
            make_authed_user(&invitee1_username, &app, email_service.clone()).await;

        let invitee2_username = crate::tests::random_name();
        let _invitee2_token =
            make_authed_user(&invitee2_username, &app, email_service.clone()).await;

        let lang_code = crate::tests::random_code();
        let body = json!({
            "code": lang_code,
            "name": "Test Language",
        });

        let request = post(&owner_token, "languages", body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // send invite to first user
        let invite_body = json!({
            "permission_level": "editor",
        });

        let request = post(
            &owner_token,
            &format!("languages/{lang_code}/invites/{invitee1_username}"),
            invite_body,
        )
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // send invite to second user
        let invite_body = json!({
            "permission_level": "viewer",
        });

        let request = post(
            &owner_token,
            &format!("languages/{lang_code}/invites/{invitee2_username}"),
            invite_body,
        )
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // search all invites
        let request = crate::controller::api::tests::get_with_auth(
            &owner_token,
            &format!("languages/{lang_code}/invites"),
        )
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        let items = body["items"].as_array().unwrap();
        assert_eq!(items.len(), 2);

        // search by recipient
        let request = crate::controller::api::tests::get_with_auth(
            &owner_token,
            &format!("languages/{lang_code}/invites?recipient={invitee1_username}"),
        )
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        let items = body["items"].as_array().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["recipient"], invitee1_username);

        // search by sender
        let request = crate::controller::api::tests::get_with_auth(
            &owner_token,
            &format!("languages/{lang_code}/invites?sender={owner_username}"),
        )
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        let items = body["items"].as_array().unwrap();
        assert_eq!(items.len(), 2);
    }

    #[tokio::test]
    async fn test_search_language_invites_unauthorized() {
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

        // try to search invites as someone without permissions
        let request = crate::controller::api::tests::get_with_auth(
            &other_token,
            &format!("languages/{lang_code}/invites/search"),
        )
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
