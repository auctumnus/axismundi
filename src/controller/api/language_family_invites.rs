use axum::{
    Json,
    extract::{Path, Query},
    http::StatusCode,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    err::{AppResult, unauthorized_no_session},
    model::{
        language_families::LanguageFamilyRepository,
        language_family_invites::{
            CreateLanguageFamilyInvite, FamilyInviteSearch, LanguageFamilyInvite,
            LanguageFamilyInviteRepository,
        },
        language_invites::PermissionLevel,
        users::UserRepository,
    },
    pagination::{PaginatedRequest, PaginatedResponse},
    util::{AppState, extract_session::Session},
};

pub fn create_router() -> axum::Router<AppState> {
    axum::Router::new()
        .route(
            "/language-families/{code}/invites/{username}",
            axum::routing::post(invite_user_to_family),
        )
        .route(
            "/language-families/{code}/invites",
            axum::routing::get(search_family_invites),
        )
        .route(
            "/language-families/{code}/invites/{username}",
            axum::routing::get(view_family_invite),
        )
        .route(
            "/language-families/{code}/invites/{username}",
            axum::routing::delete(delete_family_invite),
        )
        .route(
            "/language-families/{code}/accept-invite",
            axum::routing::post(accept_family_invite),
        )
}

type ApiResponse<T> = AppResult<T>;
type PaginatedApiResponse<T> = AppResult<Json<PaginatedResponse<T>>>;

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
    invite: LanguageFamilyInvite,
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

pub async fn invite_user_to_family(
    s: Session,
    invites: LanguageFamilyInviteRepository,
    users: UserRepository,
    Path((code, username)): Path<(String, String)>,
    Json(req): Json<InviteRequest>,
) -> ApiResponse<Json<InviteResponse>> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    let invite = invites
        .create(
            requestor.clone(),
            CreateLanguageFamilyInvite {
                language_family: code,
                recipient: username,
                permissions: req.permission_level,
            },
        )
        .await?;

    let response = map_invite_to_response(invite, users).await?;
    Ok(Json(response))
}

pub async fn search_family_invites(
    s: Session,
    invites: LanguageFamilyInviteRepository,
    users: UserRepository,
    families: LanguageFamilyRepository,
    Path(code): Path<String>,
    Query(search): Query<FamilyInviteSearch>,
    pagination: PaginatedRequest,
) -> PaginatedApiResponse<InviteResponse> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    let family = families.find_by_code(&code).await?;

    let response = invites
        .search(requestor, family.id, pagination, search)
        .await?;

    let mut mapped_items = vec![];
    for invite in response.items {
        let mapped = map_invite_to_response(invite, users.clone()).await?;
        mapped_items.push(mapped);
    }

    Ok(Json(PaginatedResponse {
        items: mapped_items,
        total: response.total,
        offset: response.offset,
        limit: response.limit,
        has_more: response.has_more,
    }))
}

pub async fn view_family_invite(
    s: Session,
    invites: LanguageFamilyInviteRepository,
    families: LanguageFamilyRepository,
    users: UserRepository,
    Path((code, username)): Path<(String, String)>,
) -> ApiResponse<Json<InviteResponse>> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    let family = families.find_by_code(&code).await?;
    let recipient = users.find_by_username(&username).await?;
    let invite = invites
        .find_by_family_and_recipient(requestor, family.id, recipient.id)
        .await?;

    let response = map_invite_to_response(invite, users).await?;
    Ok(Json(response))
}

pub async fn delete_family_invite(
    s: Session,
    families: LanguageFamilyRepository,
    invites: LanguageFamilyInviteRepository,
    users: UserRepository,
    Path((code, username)): Path<(String, String)>,
) -> ApiResponse<StatusCode> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    let family = families.find_by_code(&code).await?;
    let recipient = users.find_by_username(&username).await?;

    invites.delete(requestor, family.id, recipient.id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn accept_family_invite(
    s: Session,
    families: LanguageFamilyRepository,
    invites: LanguageFamilyInviteRepository,
    Path(code): Path<String>,
) -> ApiResponse<StatusCode> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    let family = families.find_by_code(&code).await?;

    invites.accept(requestor, family.id).await?;

    Ok(StatusCode::OK)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use reqwest::StatusCode;
    use serde_json::{Value, json};
    use tower::Service;

    use crate::{
        controller::api::tests::{delete, get_with_auth, post, print_response_body},
        email::MockEmailService,
        model::{
            language_families::{CreateLanguageFamily, LanguageFamilyRepository},
            user_tags::UserTagRepository,
            users::{User, UserRepository},
        },
        tests::{make_authed_user, random_code, random_name},
        util::AppState,
    };

    struct TestContext {
        owner_user: User,
        owner_token: String,
        invitee_user: User,
        invitee_token: String,
        #[allow(dead_code)]
        other_user: User,
        other_token: String,
        app: axum::routing::RouterIntoService<axum::body::Body>,
        family_code: String,
    }

    async fn create_test_context() -> TestContext {
        async fn make_user_for_context(
            app: &axum::routing::RouterIntoService<axum::body::Body>,
            state: AppState,
            email_service: Arc<MockEmailService>,
            tag: &str,
        ) -> (User, String) {
            let username = random_name();
            let token = make_authed_user(&username, app, email_service.clone()).await;
            let users = UserRepository::new(state.clone());
            let user = users.find_by_username(&username).await.unwrap();
            let tags = UserTagRepository::new(state.clone());
            tags.create_unchecked(user.id, tag.to_string(), true)
                .await
                .unwrap();
            (user, token)
        }

        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let (app, app_state) =
            crate::tests::test_app_with_email_service_state(&email_service_trait)
                .await
                .unwrap();

        let (owner_user, owner_token) =
            make_user_for_context(&app, app_state.clone(), email_service.clone(), "regular").await;
        let (invitee_user, invitee_token) =
            make_user_for_context(&app, app_state.clone(), email_service.clone(), "regular").await;
        let (other_user, other_token) =
            make_user_for_context(&app, app_state.clone(), email_service.clone(), "regular").await;

        // create a language family for testing (owner owns the family)
        let family_code = random_code();
        let families_repo = LanguageFamilyRepository::new(app_state.clone());
        families_repo
            .create(
                owner_user.clone(),
                CreateLanguageFamily {
                    code: family_code.clone(),
                    name: "Test Family".to_string(),
                    description: "A test language family".to_string(),
                },
            )
            .await
            .unwrap();

        TestContext {
            owner_user,
            owner_token,
            invitee_user,
            invitee_token,
            other_user,
            other_token,
            app,
            family_code,
        }
    }

    async fn create_invite(
        app: &mut axum::routing::RouterIntoService<axum::body::Body>,
        token: &str,
        family_code: &str,
        invitee_username: &str,
        permission_level: &str,
    ) -> Value {
        let body = json!({
            "permission_level": permission_level,
        });

        let request = post(
            token,
            &format!("language-families/{family_code}/invites/{invitee_username}"),
            body,
        )
        .await;

        let response = app.call(request).await.unwrap();

        if response.status() != StatusCode::OK {
            print_response_body(response).await;
            panic!("Failed to create invite");
        }

        crate::tests::response_to_value(response.into_body()).await
    }

    #[tokio::test]
    async fn test_invite_user_to_family() {
        let mut context = create_test_context().await;

        let invite = create_invite(
            &mut context.app,
            &context.owner_token,
            &context.family_code,
            &context.invitee_user.username,
            "editor",
        )
        .await;

        assert_eq!(invite["permissions"], "editor");
        assert_eq!(invite["recipient"], context.invitee_user.username);
        assert_eq!(invite["sender"], context.owner_user.username);
    }

    #[tokio::test]
    async fn test_invite_user_unauthorized() {
        let mut context = create_test_context().await;

        let body = json!({
            "permission_level": "editor",
        });

        let request = axum::http::Request::builder()
            .uri(format!(
                "/api/language-families/{}/invites/{}",
                context.family_code, context.invitee_user.username
            ))
            .method("POST")
            .header("content-type", "application/json")
            .body(axum::body::Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();

        let response = context.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_invite_user_already_has_permissions() {
        let mut context = create_test_context().await;

        // try to invite the owner (who already has permissions)
        let body = json!({
            "permission_level": "editor",
        });

        let request = post(
            &context.owner_token,
            &format!(
                "language-families/{}/invites/{}",
                context.family_code, context.owner_user.username
            ),
            body,
        )
        .await;

        let response = context.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_accept_family_invite() {
        let mut context = create_test_context().await;

        // send invite
        create_invite(
            &mut context.app,
            &context.owner_token,
            &context.family_code,
            &context.invitee_user.username,
            "editor",
        )
        .await;

        // accept invite
        let request = post(
            &context.invitee_token,
            &format!("language-families/{}/accept-invite", context.family_code),
            json!({}),
        )
        .await;

        let response = context.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_accept_family_invite_unauthorized() {
        let mut context = create_test_context().await;

        let request = axum::http::Request::builder()
            .uri(format!(
                "/api/language-families/{}/accept-invite",
                context.family_code
            ))
            .method("POST")
            .header("content-type", "application/json")
            .body(axum::body::Body::from(
                serde_json::to_vec(&json!({})).unwrap(),
            ))
            .unwrap();

        let response = context.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_accept_family_invite_not_found() {
        let mut context = create_test_context().await;

        // try to accept invite that doesn't exist
        let request = post(
            &context.other_token,
            &format!("language-families/{}/accept-invite", context.family_code),
            json!({}),
        )
        .await;

        let response = context.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_delete_family_invite() {
        let mut context = create_test_context().await;

        // send invite
        create_invite(
            &mut context.app,
            &context.owner_token,
            &context.family_code,
            &context.invitee_user.username,
            "editor",
        )
        .await;

        // delete invite
        let request = delete(
            &context.owner_token,
            &format!(
                "language-families/{}/invites/{}",
                context.family_code, context.invitee_user.username
            ),
        );

        let response = context.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn test_delete_family_invite_unauthorized() {
        let mut context = create_test_context().await;

        // send invite
        create_invite(
            &mut context.app,
            &context.owner_token,
            &context.family_code,
            &context.invitee_user.username,
            "editor",
        )
        .await;

        // try to delete without auth
        let request = axum::http::Request::builder()
            .uri(format!(
                "/api/language-families/{}/invites/{}",
                context.family_code, context.invitee_user.username
            ))
            .method("DELETE")
            .header("content-type", "application/json")
            .body(axum::body::Body::empty())
            .unwrap();

        let response = context.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_delete_family_invite_recipient_can_reject() {
        let mut context = create_test_context().await;

        // send invite
        create_invite(
            &mut context.app,
            &context.owner_token,
            &context.family_code,
            &context.invitee_user.username,
            "editor",
        )
        .await;

        // recipient deletes (rejects) invite
        let request = delete(
            &context.invitee_token,
            &format!(
                "language-families/{}/invites/{}",
                context.family_code, context.invitee_user.username
            ),
        );

        let response = context.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn test_view_family_invite() {
        let mut context = create_test_context().await;

        // send invite
        create_invite(
            &mut context.app,
            &context.owner_token,
            &context.family_code,
            &context.invitee_user.username,
            "editor",
        )
        .await;

        // view invite as owner
        let request = get_with_auth(
            &context.owner_token,
            &format!(
                "language-families/{}/invites/{}",
                context.family_code, context.invitee_user.username
            ),
        )
        .await;

        let response = context.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(body["recipient"], context.invitee_user.username);
        assert_eq!(body["sender"], context.owner_user.username);
        assert_eq!(body["permissions"], "editor");

        // view invite as invitee
        let request = get_with_auth(
            &context.invitee_token,
            &format!(
                "language-families/{}/invites/{}",
                context.family_code, context.invitee_user.username
            ),
        )
        .await;

        let response = context.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(body["recipient"], context.invitee_user.username);
    }

    #[tokio::test]
    async fn test_view_family_invite_unauthorized() {
        let mut context = create_test_context().await;

        // send invite
        create_invite(
            &mut context.app,
            &context.owner_token,
            &context.family_code,
            &context.invitee_user.username,
            "editor",
        )
        .await;

        // try to view invite as someone else
        let request = get_with_auth(
            &context.other_token,
            &format!(
                "language-families/{}/invites/{}",
                context.family_code, context.invitee_user.username
            ),
        )
        .await;

        let response = context.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_search_family_invites() {
        let mut context = create_test_context().await;

        // send invite to first invitee
        create_invite(
            &mut context.app,
            &context.owner_token,
            &context.family_code,
            &context.invitee_user.username,
            "editor",
        )
        .await;

        // send invite to second user (other_user from context)
        create_invite(
            &mut context.app,
            &context.owner_token,
            &context.family_code,
            &context.other_user.username,
            "viewer",
        )
        .await;

        // search all invites
        let request = get_with_auth(
            &context.owner_token,
            &format!("language-families/{}/invites", context.family_code),
        )
        .await;

        let response = context.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        let items = body["items"].as_array().unwrap();
        assert_eq!(items.len(), 2);

        // search by recipient
        let request = get_with_auth(
            &context.owner_token,
            &format!(
                "language-families/{}/invites?recipient={}",
                context.family_code, context.invitee_user.username
            ),
        )
        .await;

        let response = context.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        let items = body["items"].as_array().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["recipient"], context.invitee_user.username);
    }

    #[tokio::test]
    async fn test_search_family_invites_unauthorized() {
        let mut context = create_test_context().await;

        // try to search invites as someone without permissions
        let request = get_with_auth(
            &context.other_token,
            &format!("language-families/{}/invites", context.family_code),
        )
        .await;

        let response = context.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_invite_with_admin_permission() {
        let mut context = create_test_context().await;

        // invite user as admin
        let invite = create_invite(
            &mut context.app,
            &context.owner_token,
            &context.family_code,
            &context.invitee_user.username,
            "admin",
        )
        .await;

        assert_eq!(invite["permissions"], "admin");

        // accept the invite
        let request = post(
            &context.invitee_token,
            &format!("language-families/{}/accept-invite", context.family_code),
            json!({}),
        )
        .await;

        let response = context.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // now the invitee (now admin) should be able to invite others
        let invite2 = create_invite(
            &mut context.app,
            &context.invitee_token,
            &context.family_code,
            &context.other_user.username,
            "editor",
        )
        .await;

        assert_eq!(invite2["permissions"], "editor");
    }

    #[tokio::test]
    async fn test_editor_cannot_invite() {
        let mut context = create_test_context().await;

        // invite user as editor
        create_invite(
            &mut context.app,
            &context.owner_token,
            &context.family_code,
            &context.invitee_user.username,
            "editor",
        )
        .await;

        // accept the invite
        let request = post(
            &context.invitee_token,
            &format!("language-families/{}/accept-invite", context.family_code),
            json!({}),
        )
        .await;

        let response = context.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // editor should not be able to invite others
        let body = json!({
            "permission_level": "viewer",
        });

        let request = post(
            &context.invitee_token,
            &format!(
                "language-families/{}/invites/{}",
                context.family_code, context.other_user.username
            ),
            body,
        )
        .await;

        let response = context.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}
