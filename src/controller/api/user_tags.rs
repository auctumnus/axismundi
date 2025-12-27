use crate::{
    err::{AppResult, unauthorized_no_session},
    model::{
        user_tags::{CreateUserTag, UserTag, UserTagRepository},
        users::UserRepository,
    },
    util::{AppState, ensure_verified, extract_session::Session},
};
use axum::{
    Json, Router,
    extract::Path,
    routing::{delete, get, post},
};

pub fn create_router() -> (Router<AppState>, Router<AppState>) {
    let secure_routes = Router::new()
        .route("/users/{username}/tags", post(create_user_tag))
        .route("/users/{username}/tags/{tag}", delete(delete_user_tag));

    let normal_routes = Router::new().route("/users/{username}/tags", get(list_user_tags));

    (secure_routes, normal_routes)
}

async fn create_user_tag(
    s: Session,
    user_tags: UserTagRepository,
    users: UserRepository,
    Path(username): Path<String>,
    Json(req): Json<CreateUserTag>,
) -> AppResult<Json<UserTag>> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };
    ensure_verified(requestor)?;

    // Find the user being tagged
    let user = users.find_by_username(&username).await?;

    let tag = user_tags.create(requestor, user.id, req).await?;
    Ok(Json(tag))
}

async fn list_user_tags(
    user_tags: UserTagRepository,
    users: UserRepository,
    Path(username): Path<String>,
) -> AppResult<Json<Vec<UserTag>>> {
    // Find the user whose tags we're listing
    let user = users.find_by_username(&username).await?;

    let tags = user_tags.find_all_by_user_id(user.id).await?;
    Ok(Json(tags))
}

async fn delete_user_tag(
    s: Session,
    user_tags: UserTagRepository,
    users: UserRepository,
    Path((username, tag)): Path<(String, String)>,
) -> AppResult<Json<()>> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };
    ensure_verified(requestor)?;

    // Find the user whose tag we're deleting
    let user = users.find_by_username(&username).await?;

    user_tags.delete(requestor, &user, tag).await?;
    Ok(Json(()))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use reqwest::StatusCode;
    use serde_json::json;

    use crate::CONFIG;
    use crate::{
        AppState,
        controller::api::tests::{delete, get, make_authed_user, post, post_without_auth},
        email::MockEmailService,
        tests::{random_name, response_to_value},
    };
    use sqlx::PgPool;
    use tower::{Service, ServiceExt};

    /// Helper to create a moderator user for testing
    async fn make_moderator_user(
        app: &axum::routing::RouterIntoService<axum::body::Body>,
        email_service: Arc<MockEmailService>,
        pool: &PgPool,
    ) -> String {
        let username = random_name();
        let token = make_authed_user(&username, app, email_service).await;

        let id = sqlx::query_scalar!("select id from users where username = $1", username)
            .fetch_one(pool)
            .await
            .unwrap();

        sqlx::query!(
            "insert into user_tags (user_id, tag, hidden) values ($1, 'moderator', false)",
            id
        )
        .execute(pool)
        .await
        .unwrap();

        token
    }

    #[tokio::test]
    async fn test_create_user_tag() {
        let email_service = Arc::new(MockEmailService::new());
        let (mut app, admin_token) =
            crate::tests::test_app_with_admin_and_email_service(&email_service).await;

        let username = random_name();
        let _ = make_authed_user(&username, &app, email_service).await;

        let create_tag_request = post(
            &admin_token,
            &format!("users/{username}/tags"),
            json!({
                "tag": "moderator",
                "hidden": false
            }),
        )
        .await;
        let create_tag_response = app.call(create_tag_request).await.unwrap();
        assert_eq!(create_tag_response.status(), StatusCode::OK);

        // Verify the tag was created
        let list_request = get(&format!("users/{username}/tags")).await;
        let list_response = app.call(list_request).await.unwrap();
        assert_eq!(list_response.status(), StatusCode::OK);

        let tags: serde_json::Value = response_to_value(list_response.into_body()).await;
        let tags_array = tags.as_array().unwrap();
        assert_eq!(tags_array.len(), 1);
        assert_eq!(tags_array[0]["tag"], "moderator");
        assert_eq!(tags_array[0]["hidden"], false);
    }

    #[tokio::test]
    async fn test_create_user_tag_unauthorized() {
        let email_service = Arc::new(MockEmailService::new());
        let (mut app, _admin_token) =
            crate::tests::test_app_with_admin_and_email_service(&email_service).await;

        let username = random_name();
        let _ = make_authed_user(&username, &app, email_service).await;

        // Try to create tag without auth
        let create_tag_request = post_without_auth(
            &format!("users/{username}/tags"),
            json!({
                "tag": "moderator",
                "hidden": false
            }),
        )
        .await;
        let create_tag_response = app.call(create_tag_request).await.unwrap();
        assert_eq!(create_tag_response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_create_user_tag_non_moderator() {
        let email_service = Arc::new(MockEmailService::new());
        let (mut app, _admin_token) =
            crate::tests::test_app_with_admin_and_email_service(&email_service).await;

        // Create a regular user
        let regular_username = random_name();
        let regular_token = make_authed_user(&regular_username, &app, email_service.clone()).await;

        let target_username = random_name();
        let _ = make_authed_user(&target_username, &app, email_service).await;

        // Try to create tag as regular user
        let create_tag_request = post(
            &regular_token,
            &format!("users/{target_username}/tags"),
            json!({
                "tag": "contributor",
                "hidden": false
            }),
        )
        .await;
        let create_tag_response = app.call(create_tag_request).await.unwrap();
        assert_eq!(create_tag_response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_create_admin_tag_forbidden() {
        let email_service = Arc::new(MockEmailService::new());
        let (mut app, admin_token) =
            crate::tests::test_app_with_admin_and_email_service(&email_service).await;

        let username = random_name();
        let _ = make_authed_user(&username, &app, email_service).await;

        // Try to create admin tag (should be forbidden even for admins)
        let create_tag_request = post(
            &admin_token,
            &format!("users/{username}/tags"),
            json!({
                "tag": "admin",
                "hidden": false
            }),
        )
        .await;
        let create_tag_response = app.call(create_tag_request).await.unwrap();
        assert_eq!(create_tag_response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_moderator_cannot_create_moderator() {
        let pool = PgPool::connect(&CONFIG.database_url).await.unwrap();
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let app_state = AppState {
            pool: pool.clone(),
            email_service: email_service_trait,
        };
        let mut app = crate::create_router(app_state).into_service();

        // Create a moderator user
        let moderator_token = make_moderator_user(&app, email_service.clone(), &pool).await;

        let target_username = random_name();
        let _ = make_authed_user(&target_username, &app, email_service).await;

        // Try to create moderator tag as moderator (should fail)
        let create_tag_request = post(
            &moderator_token,
            &format!("users/{target_username}/tags"),
            json!({
                "tag": "moderator",
                "hidden": false
            }),
        )
        .await;
        let create_tag_response = app.call(create_tag_request).await.unwrap();
        assert_eq!(create_tag_response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_admin_can_create_moderator() {
        let email_service = Arc::new(MockEmailService::new());
        let (mut app, admin_token) =
            crate::tests::test_app_with_admin_and_email_service(&email_service).await;

        let username = random_name();
        let _ = make_authed_user(&username, &app, email_service).await;

        // Admin should be able to create moderator tag
        let create_tag_request = post(
            &admin_token,
            &format!("users/{username}/tags"),
            json!({
                "tag": "moderator",
                "hidden": false
            }),
        )
        .await;
        let create_tag_response = app.call(create_tag_request).await.unwrap();
        assert_eq!(create_tag_response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_list_user_tags() {
        let email_service = Arc::new(MockEmailService::new());
        let (mut app, admin_token) =
            crate::tests::test_app_with_admin_and_email_service(&email_service).await;

        let username = random_name();
        let _ = make_authed_user(&username, &app, email_service.clone()).await;

        // Create multiple tags
        for tag in ["moderator", "translator"] {
            let create_tag_request = post(
                &admin_token,
                &format!("users/{username}/tags"),
                json!({
                    "tag": tag,
                    "hidden": false
                }),
            )
            .await;
            app.ready()
                .await
                .unwrap()
                .call(create_tag_request)
                .await
                .unwrap();
        }

        // List tags
        let list_request = get(&format!("users/{username}/tags")).await;
        let list_response = app.call(list_request).await.unwrap();
        assert_eq!(list_response.status(), StatusCode::OK);

        let tags: serde_json::Value = response_to_value(list_response.into_body()).await;
        let tags_array = tags.as_array().unwrap();
        assert_eq!(tags_array.len(), 2);
    }

    #[tokio::test]
    async fn test_list_user_tags_empty() {
        let email_service = Arc::new(MockEmailService::new());
        let (mut app, _admin_token) =
            crate::tests::test_app_with_admin_and_email_service(&email_service).await;

        let username = random_name();
        let _ = make_authed_user(&username, &app, email_service).await;

        // List tags for user with no tags
        let list_request = get(&format!("users/{username}/tags")).await;
        let list_response = app.call(list_request).await.unwrap();
        assert_eq!(list_response.status(), StatusCode::OK);

        let tags: serde_json::Value = response_to_value(list_response.into_body()).await;
        let tags_array = tags.as_array().unwrap();
        assert_eq!(tags_array.len(), 0);
    }

    #[tokio::test]
    async fn test_delete_user_tag() {
        let email_service = Arc::new(MockEmailService::new());
        let (mut app, admin_token) =
            crate::tests::test_app_with_admin_and_email_service(&email_service).await;

        let username = random_name();
        let _ = make_authed_user(&username, &app, email_service).await;

        // Create a tag
        let create_tag_request = post(
            &admin_token,
            &format!("users/{username}/tags"),
            json!({
                "tag": "moderator",
                "hidden": false
            }),
        )
        .await;
        app.ready()
            .await
            .unwrap()
            .call(create_tag_request)
            .await
            .unwrap();

        // Delete the tag
        let delete_request = delete(&admin_token, &format!("users/{username}/tags/moderator"));
        let delete_response = app
            .ready()
            .await
            .unwrap()
            .call(delete_request)
            .await
            .unwrap();
        assert_eq!(delete_response.status(), StatusCode::OK);

        // Verify tag was deleted
        let list_request = get(&format!("users/{username}/tags")).await;
        let list_response = app.call(list_request).await.unwrap();
        let tags: serde_json::Value = response_to_value(list_response.into_body()).await;
        let tags_array = tags.as_array().unwrap();
        assert_eq!(tags_array.len(), 0);
    }

    #[tokio::test]
    async fn test_delete_user_tag_unauthorized() {
        let email_service = Arc::new(MockEmailService::new());
        let (mut app, admin_token) =
            crate::tests::test_app_with_admin_and_email_service(&email_service).await;

        let username = random_name();
        let _ = make_authed_user(&username, &app, email_service).await;

        // Create a tag
        let create_tag_request = post(
            &admin_token,
            &format!("users/{username}/tags"),
            json!({
                "tag": "moderator",
                "hidden": false
            }),
        )
        .await;
        app.ready()
            .await
            .unwrap()
            .call(create_tag_request)
            .await
            .unwrap();

        // Try to delete without auth
        let delete_request = crate::controller::api::tests::delete_without_auth(&format!(
            "users/{username}/tags/moderator"
        ));
        let delete_response = app.call(delete_request).await.unwrap();
        assert_eq!(delete_response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_delete_user_tag_non_moderator() {
        let email_service = Arc::new(MockEmailService::new());
        let (mut app, admin_token) =
            crate::tests::test_app_with_admin_and_email_service(&email_service).await;

        let username = random_name();
        let _ = make_authed_user(&username, &app, email_service.clone()).await;

        // Create a tag
        let create_tag_request = post(
            &admin_token,
            &format!("users/{username}/tags"),
            json!({
                "tag": "moderator",
                "hidden": false
            }),
        )
        .await;
        app.ready()
            .await
            .unwrap()
            .call(create_tag_request)
            .await
            .unwrap();

        // Create a regular user
        let regular_username = random_name();
        let regular_token = make_authed_user(&regular_username, &app, email_service).await;

        // Try to delete as regular user
        let delete_request = delete(&regular_token, &format!("users/{username}/tags/moderator"));
        let delete_response = app.call(delete_request).await.unwrap();
        assert_eq!(delete_response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_delete_admin_tag_forbidden() {
        let pool = PgPool::connect(&CONFIG.database_url).await.unwrap();
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let app_state = AppState {
            pool: pool.clone(),
            email_service: email_service_trait,
        };
        let mut app = crate::create_router(app_state).into_service();

        // Create two admin users
        let admin1_username = random_name();
        let admin1_token = make_authed_user(&admin1_username, &app, email_service.clone()).await;
        let admin1_id =
            sqlx::query_scalar!("select id from users where username = $1", admin1_username)
                .fetch_one(&pool)
                .await
                .unwrap();
        sqlx::query!(
            "insert into user_tags (user_id, tag, hidden) values ($1, 'admin', false)",
            admin1_id
        )
        .execute(&pool)
        .await
        .unwrap();

        let admin2_username = random_name();
        let _ = make_authed_user(&admin2_username, &app, email_service).await;
        let admin2_id =
            sqlx::query_scalar!("select id from users where username = $1", admin2_username)
                .fetch_one(&pool)
                .await
                .unwrap();
        sqlx::query!(
            "insert into user_tags (user_id, tag, hidden) values ($1, 'admin', false)",
            admin2_id
        )
        .execute(&pool)
        .await
        .unwrap();

        // Try to delete admin tag (should be forbidden even for admins)
        let delete_request = delete(
            &admin1_token,
            &format!("users/{admin2_username}/tags/admin"),
        );
        let delete_response = app.call(delete_request).await.unwrap();
        assert_eq!(delete_response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_moderator_cannot_delete_moderator() {
        let pool = PgPool::connect(&CONFIG.database_url).await.unwrap();
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let app_state = AppState {
            pool: pool.clone(),
            email_service: email_service_trait,
        };
        let mut app = crate::create_router(app_state).into_service();

        // Create a moderator user
        let moderator_token = make_moderator_user(&app, email_service.clone(), &pool).await;

        // Create another user with moderator tag
        let target_username = random_name();
        let _ = make_authed_user(&target_username, &app, email_service.clone()).await;
        let target_id =
            sqlx::query_scalar!("select id from users where username = $1", target_username)
                .fetch_one(&pool)
                .await
                .unwrap();
        sqlx::query!(
            "insert into user_tags (user_id, tag, hidden) values ($1, 'moderator', false)",
            target_id
        )
        .execute(&pool)
        .await
        .unwrap();

        // Try to delete moderator tag as moderator (should fail)
        let delete_request = delete(
            &moderator_token,
            &format!("users/{target_username}/tags/moderator"),
        );
        let delete_response = app.call(delete_request).await.unwrap();
        assert_eq!(delete_response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_admin_can_delete_moderator() {
        let email_service = Arc::new(MockEmailService::new());
        let (mut app, admin_token) =
            crate::tests::test_app_with_admin_and_email_service(&email_service).await;

        let username = random_name();
        let _ = make_authed_user(&username, &app, email_service).await;

        // Create moderator tag
        let create_tag_request = post(
            &admin_token,
            &format!("users/{username}/tags"),
            json!({
                "tag": "moderator",
                "hidden": false
            }),
        )
        .await;
        app.ready()
            .await
            .unwrap()
            .call(create_tag_request)
            .await
            .unwrap();

        // Admin should be able to delete moderator tag
        let delete_request = delete(&admin_token, &format!("users/{username}/tags/moderator"));
        let delete_response = app.call(delete_request).await.unwrap();
        assert_eq!(delete_response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_delete_nonexistent_tag() {
        let email_service = Arc::new(MockEmailService::new());
        let (mut app, admin_token) =
            crate::tests::test_app_with_admin_and_email_service(&email_service).await;

        let username = random_name();
        let _ = make_authed_user(&username, &app, email_service).await;

        // Delete non-existent tag (should succeed - idempotent)
        let delete_request = delete(&admin_token, &format!("users/{username}/tags/nonexistent"));
        let delete_response = app.call(delete_request).await.unwrap();
        assert_eq!(delete_response.status(), StatusCode::OK);
    }
}
