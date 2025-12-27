use crate::{
    err::{AppResult, unauthorized_no_session},
    model::{
        user_bans::{CreateUserBan, UserBan, UserBanRepository, UserBanSearch},
        users::UserRepository,
    },
    pagination::{PaginatedRequest, PaginatedResponse},
    util::{AppState, ensure_verified, extract_session::Session},
};
use axum::{
    Json, Router,
    extract::{Path, Query},
    routing::{delete, get, post},
};

pub fn create_router() -> (Router<AppState>, Router<AppState>) {
    let secure_routes = Router::new()
        .route("/bans", post(create_user_ban))
        .route("/bans/{username}", delete(delete_user_ban));

    let normal_routes = Router::new()
        .route("/bans", get(list_user_bans))
        .route("/bans/{username}", get(get_user_ban));

    (secure_routes, normal_routes)
}

async fn create_user_ban(
    s: Session,
    bans: UserBanRepository,
    Json(req): Json<CreateUserBan>,
) -> AppResult<Json<UserBan>> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };
    ensure_verified(requestor)?;

    let ban = bans.create(requestor, req).await?;
    Ok(Json(ban))
}

async fn list_user_bans(
    s: Session,
    bans: UserBanRepository,
    Query(pagination): Query<PaginatedRequest>,
    Query(search): Query<UserBanSearch>,
) -> AppResult<Json<PaginatedResponse<UserBan>>> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };
    ensure_verified(requestor)?;

    let ban_list = bans.search(requestor, pagination, search).await?;
    Ok(Json(ban_list))
}

async fn get_user_ban(
    Path(username): Path<String>,
    users: UserRepository,
    bans: UserBanRepository,
) -> AppResult<Json<Option<UserBan>>> {
    let user = users.find_by_username(&username).await?;
    let ban = bans.find_by_user_id(user.id).await?;
    Ok(Json(ban))
}

async fn delete_user_ban(
    s: Session,
    Path(username): Path<String>,
    users: UserRepository,
    bans: UserBanRepository,
) -> AppResult<Json<()>> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };
    ensure_verified(requestor)?;

    let user = users.find_by_username(&username).await?;
    bans.delete(requestor, user.id).await?;
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
        controller::api::tests::{delete, get, get_with_auth, make_authed_user, post},
        email::MockEmailService,
        tests::random_name,
    };
    use sqlx::PgPool;
    use tower::Service;
    use tower::ServiceExt;
    use uuid::Uuid;

    /// Helper to create a moderator user for testing
    async fn make_moderator_user(
        app: &axum::routing::RouterIntoService<axum::body::Body>,
        email_service: Arc<MockEmailService>,
        pool: &PgPool,
    ) -> (String, Uuid) {
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

        (token, id)
    }

    #[tokio::test]
    async fn test_ban_user() {
        let pool = PgPool::connect(&CONFIG.database_url).await.unwrap();
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let app_state = AppState {
            pool: pool.clone(),
            email_service: email_service_trait,
        };
        let mut app = crate::create_router(app_state).into_service();

        // Create a moderator user
        let (moderator_token, _) = make_moderator_user(&app, email_service.clone(), &pool).await;

        // Create a regular user to ban
        let target_username = random_name();
        let _ = make_authed_user(&target_username, &app, email_service).await;
        let target_id =
            sqlx::query_scalar!("select id from users where username = $1", target_username)
                .fetch_one(&pool)
                .await
                .unwrap();

        // Ban the user
        let create_ban_request = post(
            &moderator_token,
            "bans",
            json!({
                "user_id": target_id,
                "reason": "Testing ban functionality"
            }),
        )
        .await;
        let create_ban_response = app
            .ready()
            .await
            .unwrap()
            .call(create_ban_request)
            .await
            .unwrap();
        assert_eq!(create_ban_response.status(), StatusCode::OK);

        // Verify the ban exists
        let get_ban_request = get(&format!("bans/{}", target_username)).await;
        let get_ban_response = app.call(get_ban_request).await.unwrap();
        assert_eq!(get_ban_response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_ban_user_unauthorized() {
        let pool = PgPool::connect(&CONFIG.database_url).await.unwrap();
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let app_state = AppState {
            pool: pool.clone(),
            email_service: email_service_trait,
        };
        let mut app = crate::create_router(app_state).into_service();

        // Create a regular user
        let regular_username = random_name();
        let regular_token = make_authed_user(&regular_username, &app, email_service.clone()).await;

        // Create a target user
        let target_username = random_name();
        let _ = make_authed_user(&target_username, &app, email_service).await;
        let target_id =
            sqlx::query_scalar!("select id from users where username = $1", target_username)
                .fetch_one(&pool)
                .await
                .unwrap();

        // Try to ban as regular user (should fail)
        let create_ban_request = post(
            &regular_token,
            "bans",
            json!({
                "user_id": target_id,
                "reason": "Testing ban functionality"
            }),
        )
        .await;
        let create_ban_response = app.call(create_ban_request).await.unwrap();
        assert_eq!(create_ban_response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_cannot_ban_moderator() {
        let pool = PgPool::connect(&CONFIG.database_url).await.unwrap();
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let app_state = AppState {
            pool: pool.clone(),
            email_service: email_service_trait,
        };
        let mut app = crate::create_router(app_state).into_service();

        // Create a moderator user
        let (moderator_token, _) = make_moderator_user(&app, email_service.clone(), &pool).await;

        // Create another moderator user to try to ban
        let (_, target_mod_id) = make_moderator_user(&app, email_service, &pool).await;

        // Try to ban the moderator (should fail)
        let create_ban_request = post(
            &moderator_token,
            "bans",
            json!({
                "user_id": target_mod_id,
                "reason": "Testing ban functionality"
            }),
        )
        .await;
        let create_ban_response = app.call(create_ban_request).await.unwrap();
        assert_eq!(create_ban_response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_cannot_ban_admin() {
        let pool = PgPool::connect(&CONFIG.database_url).await.unwrap();
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let app_state = AppState {
            pool: pool.clone(),
            email_service: email_service_trait,
        };
        let mut app = crate::create_router(app_state).into_service();

        // Create a moderator user
        let (moderator_token, _) = make_moderator_user(&app, email_service.clone(), &pool).await;

        // Create an admin user
        let admin_username = random_name();
        let _ = make_authed_user(&admin_username, &app, email_service).await;
        let admin_id =
            sqlx::query_scalar!("select id from users where username = $1", admin_username)
                .fetch_one(&pool)
                .await
                .unwrap();
        sqlx::query!(
            "insert into user_tags (user_id, tag, hidden) values ($1, 'admin', false)",
            admin_id
        )
        .execute(&pool)
        .await
        .unwrap();

        // Try to ban the admin (should fail)
        let create_ban_request = post(
            &moderator_token,
            "bans",
            json!({
                "user_id": admin_id,
                "reason": "Testing ban functionality"
            }),
        )
        .await;
        let create_ban_response = app.call(create_ban_request).await.unwrap();
        assert_eq!(create_ban_response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_unban_user() {
        let pool = PgPool::connect(&CONFIG.database_url).await.unwrap();
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let app_state = AppState {
            pool: pool.clone(),
            email_service: email_service_trait,
        };
        let mut app = crate::create_router(app_state).into_service();

        // Create a moderator user
        let (moderator_token, _) = make_moderator_user(&app, email_service.clone(), &pool).await;

        // Create a regular user to ban
        let target_username = random_name();
        let _ = make_authed_user(&target_username, &app, email_service).await;
        let target_id =
            sqlx::query_scalar!("select id from users where username = $1", target_username)
                .fetch_one(&pool)
                .await
                .unwrap();

        // Ban the user
        let create_ban_request = post(
            &moderator_token,
            "bans",
            json!({
                "user_id": target_id,
                "reason": "Testing ban functionality"
            }),
        )
        .await;
        app.ready()
            .await
            .unwrap()
            .call(create_ban_request)
            .await
            .unwrap();

        // Unban the user
        let delete_ban_request = delete(&moderator_token, &format!("bans/{}", target_username));
        let delete_ban_response = app
            .ready()
            .await
            .unwrap()
            .call(delete_ban_request)
            .await
            .unwrap();
        assert_eq!(delete_ban_response.status(), StatusCode::OK);

        // Verify the ban is gone
        let get_ban_request = get(&format!("bans/{}", target_username)).await;
        let get_ban_response = app.call(get_ban_request).await.unwrap();
        assert_eq!(get_ban_response.status(), StatusCode::OK);

        // Response should be null/None
        let body = axum::body::to_bytes(get_ban_response.into_body(), 10_000)
            .await
            .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(value.is_null());
    }

    #[tokio::test]
    async fn test_list_bans() {
        let pool = PgPool::connect(&CONFIG.database_url).await.unwrap();
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let app_state = AppState {
            pool: pool.clone(),
            email_service: email_service_trait,
        };
        let mut app = crate::create_router(app_state).into_service();

        // Create a moderator user
        let (moderator_token, _) = make_moderator_user(&app, email_service.clone(), &pool).await;

        // Create and ban multiple users
        for _ in 0..3 {
            let target_username = random_name();
            let _ = make_authed_user(&target_username, &app, email_service.clone()).await;
            let target_id =
                sqlx::query_scalar!("select id from users where username = $1", target_username)
                    .fetch_one(&pool)
                    .await
                    .unwrap();

            let create_ban_request = post(
                &moderator_token,
                "bans",
                json!({
                    "user_id": target_id,
                    "reason": "Testing ban functionality"
                }),
            )
            .await;
            app.ready()
                .await
                .unwrap()
                .call(create_ban_request)
                .await
                .unwrap();
        }

        // List all bans
        let list_request = get_with_auth(&moderator_token, "bans").await;
        let list_response = app.call(list_request).await.unwrap();
        assert_eq!(list_response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(list_response.into_body(), 10_000)
            .await
            .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(value["items"].is_array());
        assert!(value["items"].as_array().unwrap().len() >= 3);
    }
}
