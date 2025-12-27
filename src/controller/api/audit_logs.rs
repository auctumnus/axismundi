use crate::{
    err::{AppResult, unauthorized_no_session},
    model::audit_log::{AuditLog, AuditLogFilter, AuditLogRepository},
    pagination::{PaginatedRequest, PaginatedResponse},
    util::extract_session::Session,
    util::{AppState, ensure_verified},
};
use axum::{
    Json, Router,
    extract::{Path, Query},
    routing::get,
};
use uuid::Uuid;

pub fn create_router() -> (Router<AppState>, Router<AppState>) {
    let secure_routes = Router::new();

    let normal_routes = Router::new()
        .route("/audit_logs", get(search_audit_logs))
        .route("/audit_logs/{id}", get(get_audit_log));

    (secure_routes, normal_routes)
}

/// Get a specific audit log by ID (mods/admins only)
async fn get_audit_log(
    s: Session,
    audit_logs: AuditLogRepository,
    Path(id): Path<Uuid>,
) -> AppResult<Json<AuditLog>> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };
    ensure_verified(requestor)?;

    let log = audit_logs.find_by_id(requestor, id).await?;
    Ok(Json(log))
}

/// Search audit logs with filters (mods/admins only)
async fn search_audit_logs(
    s: Session,
    audit_logs: AuditLogRepository,
    Query(pagination): Query<PaginatedRequest>,
    Query(filter): Query<AuditLogFilter>,
) -> AppResult<Json<PaginatedResponse<AuditLog>>> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };
    ensure_verified(requestor)?;

    let results = audit_logs.search(requestor, pagination, filter).await?;
    Ok(Json(results))
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use serde_json::json;
    use std::sync::Arc;
    use tower::Service;

    use crate::controller::api::tests::{
        create_test_definition, create_test_language, create_test_translatable,
        create_test_translation, create_test_word, delete, delete_without_auth, get, get_with_auth,
        make_authed_user, post, post_without_auth, print_response_body,
    };
    use crate::email::MockEmailService;
    use sqlx::PgPool;
    use tower::ServiceExt;

    use crate::{config::CONFIG, create_router, email, util::AppState};

    struct TestContext {
        app: axum::routing::RouterIntoService<axum::body::Body>,
        pool: PgPool,

        moderator_token: String,
        admin_token: String,
        normal_token: String,

        admin_id: uuid::Uuid,
        moderator_id: uuid::Uuid,
        normal_id: uuid::Uuid,

        audit_log_id: uuid::Uuid,
    }

    async fn make_test_context() -> TestContext {
        let pool = PgPool::connect(&CONFIG.database_url).await.unwrap();
        let email_service = std::sync::Arc::new(email::MockEmailService::new());
        let app_state = AppState {
            pool: pool.clone(),
            email_service: email_service.clone(),
        };
        let app = create_router(app_state).into_service();

        // make admin user

        let admin_username = crate::tests::random_name();
        let admin_token =
            crate::tests::make_authed_user(&admin_username, &app, email_service.clone()).await;

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

        // make moderator user

        let mod_username = crate::tests::random_name();
        let mod_token =
            crate::tests::make_authed_user(&mod_username, &app, email_service.clone()).await;

        let mod_id = sqlx::query_scalar!("select id from users where username = $1", mod_username)
            .fetch_one(&pool)
            .await
            .unwrap();

        sqlx::query!(
            "insert into user_tags (user_id, tag, hidden) values ($1, 'moderator', false)",
            mod_id
        )
        .execute(&pool)
        .await
        .unwrap();

        // make regular user

        let normal_username = crate::tests::random_name();
        let normal_token =
            crate::tests::make_authed_user(&normal_username, &app, email_service.clone()).await;

        let normal_id =
            sqlx::query_scalar!("select id from users where username = $1", normal_username)
                .fetch_one(&pool)
                .await
                .unwrap();

        let audit_log_id = sqlx::query_scalar!(
            r#"
            insert into audit_logs
            (user_id, action, resource_type, resource_id, details)
            values ($1, 'updated', 'user', $2, '{ "info": "test audit log" }')
            returning id
            "#,
            admin_id,
            mod_id
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        TestContext {
            app,
            pool,
            moderator_token: mod_token,
            admin_token,
            normal_token,
            audit_log_id,
            admin_id,
            moderator_id: mod_id,
            normal_id,
        }
    }

    async fn add_audit_logs_for_search(ctx: &mut TestContext) -> uuid::Uuid {
        let code = crate::tests::random_code();

        // Create a language for testing
        let language_id = sqlx::query_scalar!(
            r#"
            insert into languages (code, name, created_by, updated_by, description)
            values ($1, 'Lojban', $2, $2, 'loglang for oomfies')
            returning id
            "#,
            code,
            ctx.admin_id
        )
        .fetch_one(&ctx.pool)
        .await
        .unwrap();

        // Add various audit logs for different scenarios
        sqlx::query!(
            r#"
            insert into audit_logs (user_id, action, resource_type, resource_id, details)
            values
            ($1, 'created', 'language', $2, '{ "info": "created lojban language" }'),
            ($1, 'updated', 'language', $2, '{ "info": "updated lojban language" }')
            "#,
            ctx.admin_id,
            language_id
        )
        .execute(&ctx.pool)
        .await
        .unwrap();

        language_id
    }

    #[tokio::test]
    async fn test_get_audit_log_as_admin() {
        let mut ctx = make_test_context().await;
        let request = get_with_auth(
            &ctx.admin_token,
            &format!("audit_logs/{}", ctx.audit_log_id),
        )
        .await;
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_get_audit_log_as_moderator() {
        let mut ctx = make_test_context().await;
        let request = get_with_auth(
            &ctx.moderator_token,
            &format!("audit_logs/{}", ctx.audit_log_id),
        )
        .await;
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_get_audit_log_as_normal_user() {
        let mut ctx = make_test_context().await;
        let request = get_with_auth(
            &ctx.normal_token,
            &format!("audit_logs/{}", ctx.audit_log_id),
        )
        .await;
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_search_audit_logs_as_admin() {
        let mut ctx = make_test_context().await;
        let language_id = add_audit_logs_for_search(&mut ctx).await;

        let request = get_with_auth(&ctx.admin_token, "audit_logs").await;
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        // Should have at least the 3 logs we created (1 in setup, 2 in add_audit_logs_for_search)
        assert!(json["total"].as_i64().unwrap() >= 3);
    }

    #[tokio::test]
    async fn test_search_audit_logs_as_moderator() {
        let mut ctx = make_test_context().await;
        add_audit_logs_for_search(&mut ctx).await;

        let request = get_with_auth(&ctx.moderator_token, "audit_logs").await;
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_search_audit_logs_as_normal_user() {
        let mut ctx = make_test_context().await;

        let request = get_with_auth(&ctx.normal_token, "audit_logs").await;
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_search_audit_logs_without_auth() {
        let mut ctx = make_test_context().await;

        let request = get("audit_logs").await;
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_get_audit_log_without_auth() {
        let mut ctx = make_test_context().await;

        let request = get(&format!("audit_logs/{}", ctx.audit_log_id)).await;
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_search_audit_logs_by_action_type() {
        let mut ctx = make_test_context().await;
        add_audit_logs_for_search(&mut ctx).await;

        let request = get_with_auth(&ctx.admin_token, "audit_logs?action=created").await;
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        // Verify all results have action=created
        let items = json["items"].as_array().unwrap();
        for item in items {
            assert_eq!(item["action"].as_str().unwrap(), "created");
        }
    }

    #[tokio::test]
    async fn test_search_audit_logs_by_resource_type() {
        let mut ctx = make_test_context().await;
        add_audit_logs_for_search(&mut ctx).await;

        let request = get_with_auth(&ctx.admin_token, "audit_logs?resource_type=language").await;
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        // Verify all results have resource_type=language
        let items = json["items"].as_array().unwrap();
        for item in items {
            assert_eq!(item["resource_type"].as_str().unwrap(), "language");
        }
    }

    #[tokio::test]
    async fn test_search_audit_logs_by_resource_id() {
        let mut ctx = make_test_context().await;
        let language_id = add_audit_logs_for_search(&mut ctx).await;

        let request = get_with_auth(
            &ctx.admin_token,
            &format!("audit_logs?resource_id={}", language_id),
        )
        .await;
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        // Should have exactly 2 logs for this language
        assert_eq!(json["total"].as_i64().unwrap(), 2);
        let items = json["items"].as_array().unwrap();
        assert_eq!(items.len(), 2);
    }

    #[tokio::test]
    async fn test_search_audit_logs_pagination() {
        let mut ctx = make_test_context().await;
        add_audit_logs_for_search(&mut ctx).await;

        // Get first page with limit 1
        let request = get_with_auth(&ctx.admin_token, "audit_logs?limit=1").await;
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["items"].as_array().unwrap().len(), 1);
        assert_eq!(json["limit"].as_i64().unwrap(), 1);
        assert_eq!(json["offset"].as_i64().unwrap(), 0);
        assert!(json["has_more"].as_bool().unwrap());

        // Get second page
        let request = get_with_auth(&ctx.admin_token, "audit_logs?limit=1&offset=1").await;
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["items"].as_array().unwrap().len(), 1);
        assert_eq!(json["offset"].as_i64().unwrap(), 1);
    }

    #[tokio::test]
    async fn test_get_nonexistent_audit_log() {
        let mut ctx = make_test_context().await;
        let fake_id = uuid::Uuid::new_v4();

        let request = get_with_auth(&ctx.admin_token, &format!("audit_logs/{}", fake_id)).await;
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
