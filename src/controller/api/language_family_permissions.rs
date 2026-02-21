use axum::{Json, extract::Path, http::StatusCode};
use serde::Deserialize;

use crate::{
    err::{AppResult, unauthorized_no_session},
    model::{
        language_families::LanguageFamilyRepository,
        language_family_permissions::{
            LanguageFamilyPermission, LanguageFamilyPermissionRepository,
        },
        language_invites::PermissionLevel,
        users::UserRepository,
    },
    util::{AppState, extract_session::Session},
};

pub fn create_router() -> axum::Router<AppState> {
    axum::Router::new()
        .route(
            "/language-families/{code}/permissions",
            axum::routing::get(get_family_permissions),
        )
        .route(
            "/language-families/{code}/permissions/{username}",
            axum::routing::get(get_user_family_permissions)
                .put(edit_user_permissions)
                .delete(delete_user_permissions),
        )
}

type ApiResponse<T> = AppResult<T>;

pub async fn get_family_permissions(
    s: Session,
    families: LanguageFamilyRepository,
    permissions: LanguageFamilyPermissionRepository,
    Path(code): Path<String>,
) -> ApiResponse<Json<Vec<LanguageFamilyPermission>>> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    let family = families.find_by_code(&code).await?;

    permissions
        .list_by_family_checked(requestor, family.id)
        .await
        .map(Json)
}

pub async fn get_user_family_permissions(
    s: Session,
    families: LanguageFamilyRepository,
    permissions: LanguageFamilyPermissionRepository,
    users: UserRepository,
    Path((code, username)): Path<(String, String)>,
) -> ApiResponse<Json<LanguageFamilyPermission>> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    let family = families.find_by_code(&code).await?;
    let target_user = users.find_by_username(&username).await?;

    permissions
        .find_by_user_and_family_checked(requestor, family.id, target_user.id)
        .await
        .map(Json)
}

#[derive(Deserialize)]
pub struct EditPermissionRequest {
    pub permission_level: PermissionLevel,
}

pub async fn edit_user_permissions(
    s: Session,
    families: LanguageFamilyRepository,
    permissions: LanguageFamilyPermissionRepository,
    users: UserRepository,
    Path((code, username)): Path<(String, String)>,
    Json(req): Json<EditPermissionRequest>,
) -> ApiResponse<Json<LanguageFamilyPermission>> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    let family = families.find_by_code(&code).await?;
    let target_user = users.find_by_username(&username).await?;
    let target_perm = permissions
        .find_by_family_and_user(family.id, target_user.id)
        .await?;

    let Some(target) = target_perm else {
        return Err(crate::err::bad_request(
            "user doesn't have permissions for this language family",
        ));
    };

    permissions
        .update_permission_checked(requestor, target.id, req.permission_level)
        .await
        .map(Json)
}

pub async fn delete_user_permissions(
    s: Session,
    families: LanguageFamilyRepository,
    permissions: LanguageFamilyPermissionRepository,
    users: UserRepository,
    Path((code, username)): Path<(String, String)>,
) -> ApiResponse<StatusCode> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    let family = families.find_by_code(&code).await?;
    let target_user = users.find_by_username(&username).await?;
    let target_perm = permissions
        .find_by_family_and_user(family.id, target_user.id)
        .await?;

    let Some(target) = target_perm else {
        return Err(crate::err::not_found(
            "user doesn't have permissions for this language family",
        ));
    };

    permissions.delete_checked(requestor, target.id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use reqwest::StatusCode;
    use serde_json::json;
    use tower::Service;

    use crate::{
        controller::api::tests::{delete, get_with_auth, post, print_response_body, put},
        email::MockEmailService,
        model::{
            language_families::{CreateLanguageFamily, LanguageFamilyRepository},
            language_family_invites::{CreateLanguageFamilyInvite, LanguageFamilyInviteRepository},
            user_tags::UserTagRepository,
            users::{User, UserRepository},
        },
        tests::{make_authed_user, random_code, random_name},
        util::AppState,
    };

    struct TestContext {
        owner_user: User,
        owner_token: String,
        editor_user: User,
        editor_token: String,
        #[allow(dead_code)]
        other_user: User,
        other_token: String,
        app: axum::routing::RouterIntoService<axum::body::Body>,
        app_state: AppState,
        family_code: String,
    }

    async fn create_test_context() -> TestContext {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let (app, app_state) =
            crate::tests::test_app_with_email_service_state(&email_service_trait)
                .await
                .unwrap();

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

        let (owner_user, owner_token) =
            make_user_for_context(&app, app_state.clone(), email_service.clone(), "regular").await;
        let (editor_user, editor_token) =
            make_user_for_context(&app, app_state.clone(), email_service.clone(), "regular").await;
        let (other_user, other_token) =
            make_user_for_context(&app, app_state.clone(), email_service.clone(), "regular").await;

        // create a language family for testing
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

        // invite editor and have them accept
        let invites = LanguageFamilyInviteRepository::new(app_state.clone());
        invites
            .create(
                owner_user.clone(),
                CreateLanguageFamilyInvite {
                    language_family: family_code.clone(),
                    recipient: editor_user.username.clone(),
                    permissions: crate::model::language_invites::PermissionLevel::Editor,
                },
            )
            .await
            .unwrap();

        let family = families_repo.find_by_code(&family_code).await.unwrap();
        invites.accept(&editor_user, family.id).await.unwrap();

        TestContext {
            owner_user,
            owner_token,
            editor_user,
            editor_token,
            other_user,
            other_token,
            app,
            app_state,
            family_code,
        }
    }

    #[tokio::test]
    async fn test_get_family_permissions() {
        let mut context = create_test_context().await;

        let request = get_with_auth(
            &context.owner_token,
            &format!("language-families/{}/permissions", context.family_code),
        )
        .await;

        let response = context.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert!(body.is_array());
        let perms = body.as_array().unwrap();
        assert_eq!(perms.len(), 2); // owner + editor
    }

    #[tokio::test]
    async fn test_get_family_permissions_unauthorized() {
        let mut context = create_test_context().await;

        let request = crate::controller::api::tests::get(&format!(
            "language-families/{}/permissions",
            context.family_code
        ))
        .await;

        let response = context.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_get_family_permissions_forbidden_for_non_member() {
        let mut context = create_test_context().await;

        let request = get_with_auth(
            &context.other_token,
            &format!("language-families/{}/permissions", context.family_code),
        )
        .await;

        let response = context.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_get_user_family_permissions() {
        let mut context = create_test_context().await;

        let request = get_with_auth(
            &context.owner_token,
            &format!(
                "language-families/{}/permissions/{}",
                context.family_code, context.owner_user.username
            ),
        )
        .await;

        let response = context.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(body["permission"], "owner");
    }

    #[tokio::test]
    async fn test_get_user_family_permissions_unauthorized() {
        let mut context = create_test_context().await;

        let request = crate::controller::api::tests::get(&format!(
            "language-families/{}/permissions/{}",
            context.family_code, context.owner_user.username
        ))
        .await;

        let response = context.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_edit_user_permissions() {
        let mut context = create_test_context().await;

        // owner updates editor's permission to admin
        let update_body = json!({
            "permission_level": "admin",
        });

        let request = put(
            &context.owner_token,
            &format!(
                "language-families/{}/permissions/{}",
                context.family_code, context.editor_user.username
            ),
            &update_body,
        );

        let response = context.app.call(request).await.unwrap();

        if response.status() != StatusCode::OK {
            print_response_body(response).await;
            panic!("Failed to edit user permissions");
        }

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(body["permission"], "admin");
    }

    #[tokio::test]
    async fn test_edit_user_permissions_unauthorized() {
        let mut context = create_test_context().await;

        let update_body = json!({
            "permission_level": "admin",
        });

        let request = crate::controller::api::tests::put_without_auth(
            &format!(
                "language-families/{}/permissions/{}",
                context.family_code, context.editor_user.username
            ),
            &update_body,
        );

        let response = context.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_edit_user_permissions_editor_cannot_edit() {
        let mut context = create_test_context().await;

        // editor tries to update owner's permission
        let update_body = json!({
            "permission_level": "editor",
        });

        let request = put(
            &context.editor_token,
            &format!(
                "language-families/{}/permissions/{}",
                context.family_code, context.owner_user.username
            ),
            &update_body,
        );

        let response = context.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_delete_user_permissions() {
        let mut context = create_test_context().await;

        // owner deletes editor's permission
        let request = delete(
            &context.owner_token,
            &format!(
                "language-families/{}/permissions/{}",
                context.family_code, context.editor_user.username
            ),
        );

        let response = context.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        // verify editor no longer has permission
        let permissions =
            crate::model::language_family_permissions::LanguageFamilyPermissionRepository::new(
                context.app_state.clone(),
            );
        let families = LanguageFamilyRepository::new(context.app_state.clone());
        let family = families.find_by_code(&context.family_code).await.unwrap();
        let perm = permissions
            .find_by_family_and_user(family.id, context.editor_user.id)
            .await
            .unwrap();
        assert!(perm.is_none());
    }

    #[tokio::test]
    async fn test_delete_user_permissions_unauthorized() {
        let mut context = create_test_context().await;

        let request = crate::controller::api::tests::delete_without_auth(&format!(
            "language-families/{}/permissions/{}",
            context.family_code, context.editor_user.username
        ));

        let response = context.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_owner_cannot_delete_own_permission() {
        let mut context = create_test_context().await;

        let request = delete(
            &context.owner_token,
            &format!(
                "language-families/{}/permissions/{}",
                context.family_code, context.owner_user.username
            ),
        );

        let response = context.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_editor_can_remove_own_permission() {
        let mut context = create_test_context().await;

        // editor removes their own permission
        let request = delete(
            &context.editor_token,
            &format!(
                "language-families/{}/permissions/{}",
                context.family_code, context.editor_user.username
            ),
        );

        let response = context.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn test_editor_cannot_delete_owner_permission() {
        let mut context = create_test_context().await;

        let request = delete(
            &context.editor_token,
            &format!(
                "language-families/{}/permissions/{}",
                context.family_code, context.owner_user.username
            ),
        );

        let response = context.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_admin_can_promote_editor() {
        let mut context = create_test_context().await;

        // first, promote editor to admin
        let promote_body = json!({
            "permission_level": "admin",
        });

        let request = put(
            &context.owner_token,
            &format!(
                "language-families/{}/permissions/{}",
                context.family_code, context.editor_user.username
            ),
            &promote_body,
        );

        let response = context.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // invite other_user as editor via owner
        let invite_body = json!({
            "permission_level": "editor",
        });

        let request = post(
            &context.owner_token,
            &format!(
                "language-families/{}/invites/{}",
                context.family_code, context.other_user.username
            ),
            invite_body,
        )
        .await;

        let response = context.app.call(request).await.unwrap();

        if response.status() != StatusCode::OK {
            print_response_body(response).await;
            panic!("Failed to invite new editor");
        }

        // accept invite
        let invites = LanguageFamilyInviteRepository::new(context.app_state.clone());
        let families = LanguageFamilyRepository::new(context.app_state.clone());
        let family = families.find_by_code(&context.family_code).await.unwrap();
        invites
            .accept(&context.other_user, family.id)
            .await
            .unwrap();

        // now the admin (formerly editor) can promote the new editor
        let promote_body = json!({
            "permission_level": "admin",
        });

        let request = put(
            &context.editor_token, // now admin
            &format!(
                "language-families/{}/permissions/{}",
                context.family_code, context.other_user.username
            ),
            &promote_body,
        );

        let response = context.app.call(request).await.unwrap();
        // admin can promote editor to admin
        assert_eq!(response.status(), StatusCode::OK);
    }
}
