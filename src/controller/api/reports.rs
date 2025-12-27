use crate::{
    err::{AppResult, unauthorized_no_session},
    model::reports::{
        CreateReport, Report, ReportRepository, ReportSearch, UpdateReportModerator,
    },
    pagination::{PaginatedRequest, PaginatedResponse},
    util::{extract_session::Session, ensure_verified, AppState},
};
use axum::{
    extract::{Path, Query},
    routing::{delete, get, patch, post},
    Json, Router,
};
use uuid::Uuid;

pub fn create_router() -> (Router<AppState>, Router<AppState>) {
    let secure_routes = Router::new()
        .route("/reports", post(create_report))
        .route("/reports/{id}", patch(update_report))
        .route("/reports/{id}", delete(delete_report));

    let normal_routes = Router::new()
        .route("/reports", get(search_reports))
        .route("/reports/own", get(search_own_reports))
        .route("/reports/{id}", get(get_report));

    (secure_routes, normal_routes)
}

/// Create a new report
async fn create_report(
    s: Session,
    reports: ReportRepository,
    Json(req): Json<CreateReport>,
) -> AppResult<Json<Report>> {
    let Some(reporter) = s.user() else {
        return Err(unauthorized_no_session());
    };
    ensure_verified(reporter)?;

    let report = reports.create(reporter, req).await?;
    Ok(Json(report))
}

/// Get a specific report by ID
/// - Mods/admins can see any report with all fields
/// - Users can only see their own reports with fields hidden
async fn get_report(
    s: Session,
    reports: ReportRepository,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Report>> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };
    ensure_verified(requestor)?;

    let report = reports.find_by_id(requestor, id).await?;
    Ok(Json(report))
}

/// Search all reports (mods/admins only)
async fn search_reports(
    s: Session,
    reports: ReportRepository,
    Query(pagination): Query<PaginatedRequest>,
    Query(search): Query<ReportSearch>,
) -> AppResult<Json<PaginatedResponse<Report>>> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };
    ensure_verified(requestor)?;

    let results = reports.search(requestor, pagination, search).await?;
    Ok(Json(results))
}

/// Search user's own reports (sanitized data)
async fn search_own_reports(
    s: Session,
    reports: ReportRepository,
    Query(pagination): Query<PaginatedRequest>,
    Query(search): Query<ReportSearch>,
) -> AppResult<Json<PaginatedResponse<Report>>> {
    let Some(user) = s.user() else {
        return Err(unauthorized_no_session());
    };
    ensure_verified(user)?;

    let results = reports.search_own(user, pagination, search).await?;
    Ok(Json(results))
}

/// Update a report (mods/admins only)
async fn update_report(
    s: Session,
    reports: ReportRepository,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateReportModerator>,
) -> AppResult<Json<Report>> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };
    ensure_verified(requestor)?;

    let report = reports.update(requestor, id, req).await?;
    Ok(Json(report))
}

/// Delete a report (admins only)
async fn delete_report(
    s: Session,
    reports: ReportRepository,
    Path(id): Path<Uuid>,
) -> AppResult<Json<()>> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };
    ensure_verified(requestor)?;

    reports.delete(requestor, id).await?;
    Ok(Json(()))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use reqwest::StatusCode;
    use serde_json::json;

    use crate::{
        controller::api::tests::{delete, get, get_with_auth, make_authed_user, post},
        email::MockEmailService,
        model::reports::{ReportPriority, ResolutionStatus},
        tests::{random_name, response_to_value},
        AppState,
    };
    use sqlx::PgPool;
    use tower::{Service, ServiceExt};
    use crate::CONFIG;
    use uuid::Uuid;

    /// Helper to create a patch request
    fn patch(token: &str, uri: &str, body: serde_json::Value) -> axum::http::Request<axum::body::Body> {
        use axum::http::Request;
        use axum::body::Body;

        Request::builder()
            .uri(format!("/api/{uri}"))
            .method("PATCH")
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {token}"))
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap()
    }

    /// Helper to create a moderator user for testing
    async fn make_moderator_user(app: &axum::routing::RouterIntoService<axum::body::Body>, email_service: Arc<MockEmailService>, pool: &PgPool) -> (String, Uuid) {
        let username = random_name();
        let token = make_authed_user(&username, app, email_service).await;

        let id = sqlx::query_scalar!(
            "select id from users where username = $1",
            username
        )
        .fetch_one(pool)
        .await.unwrap();

        sqlx::query!(
            "insert into user_tags (user_id, tag, hidden) values ($1, 'moderator', false)",
            id
        )
        .execute(pool)
        .await.unwrap();

        (token, id)
    }

    /// Helper to create an admin user for testing
    async fn make_admin_user(app: &axum::routing::RouterIntoService<axum::body::Body>, email_service: Arc<MockEmailService>, pool: &PgPool) -> (String, Uuid) {
        let username = random_name();
        let token = make_authed_user(&username, app, email_service).await;

        let id = sqlx::query_scalar!(
            "select id from users where username = $1",
            username
        )
        .fetch_one(pool)
        .await.unwrap();

        sqlx::query!(
            "insert into user_tags (user_id, tag, hidden) values ($1, 'admin', false)",
            id
        )
        .execute(pool)
        .await.unwrap();

        (token, id)
    }

    #[tokio::test]
    async fn test_create_report() {
        let pool = PgPool::connect(&CONFIG.database_url).await.unwrap();
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let app_state = AppState {
            pool: pool.clone(),
            email_service: email_service_trait,
        };
        let mut app = crate::create_router(app_state).into_service();

        // Create a user
        let reporter_username = random_name();
        let reporter_token = make_authed_user(&reporter_username, &app, email_service.clone()).await;
        let reporter_id = sqlx::query_scalar!(
            "select id from users where username = $1",
            reporter_username
        )
        .fetch_one(&pool)
        .await.unwrap();

        // Create a report
        let create_report_request = post(
            &reporter_token,
            "reports",
            json!({
                "resource_type": "user",
                "resource_id": reporter_id,
                "reason": "This is a test report"
            }),
        ).await;
        let create_report_response = app.ready().await.unwrap().call(create_report_request).await.unwrap();
        assert_eq!(create_report_response.status(), StatusCode::OK);

        let report: serde_json::Value = response_to_value(create_report_response.into_body()).await;
        assert_eq!(report["reason"].as_str().unwrap(), "This is a test report");

        // Verify sanitization - regular users shouldn't see mod-only fields
        assert!(report["priority"].is_null());
        assert!(report["resolved_by"].is_null());
        assert!(report["mods_updated_at"].is_null());
        assert!(report["mods_updated_by"].is_null());
    }

    #[tokio::test]
    async fn test_create_report_validation() {
        let email_service = Arc::new(MockEmailService::new());
        let (mut app, _) = crate::tests::test_app_with_admin_and_email_service(&email_service).await;

        let user_token = make_authed_user(&random_name(), &app, email_service.clone()).await;

        // Test empty reason
        let create_report_request = post(
            &user_token,
            "reports",
            json!({
                "resource_type": "user",
                "resource_id": Uuid::new_v4(),
                "reason": ""
            }),
        ).await;
        let create_report_response = app.ready().await.unwrap().call(create_report_request).await.unwrap();
        assert_eq!(create_report_response.status(), StatusCode::BAD_REQUEST);

        // Test reason too long (>5000 chars)
        let long_reason = "x".repeat(5001);
        let create_report_request = post(
            &user_token,
            "reports",
            json!({
                "resource_type": "user",
                "resource_id": Uuid::new_v4(),
                "reason": long_reason
            }),
        ).await;
        let create_report_response = app.call(create_report_request).await.unwrap();
        assert_eq!(create_report_response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_get_own_report() {
        let pool = PgPool::connect(&CONFIG.database_url).await.unwrap();
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let app_state = AppState {
            pool: pool.clone(),
            email_service: email_service_trait,
        };
        let mut app = crate::create_router(app_state).into_service();

        // Create a user and a report
        let reporter_username = random_name();
        let reporter_token = make_authed_user(&reporter_username, &app, email_service.clone()).await;
        let reporter_id = sqlx::query_scalar!(
            "select id from users where username = $1",
            reporter_username
        )
        .fetch_one(&pool)
        .await.unwrap();

        let create_report_request = post(
            &reporter_token,
            "reports",
            json!({
                "resource_type": "user",
                "resource_id": reporter_id,
                "reason": "Test report"
            }),
        ).await;
        let create_report_response = app.ready().await.unwrap().call(create_report_request).await.unwrap();
        let report: serde_json::Value = response_to_value(create_report_response.into_body()).await;
        let report_id = report["id"].as_str().unwrap();

        // Get the report
        let get_report_request = get_with_auth(&reporter_token, &format!("reports/{}", report_id)).await;
        let get_report_response = app.call(get_report_request).await.unwrap();
        assert_eq!(get_report_response.status(), StatusCode::OK);

        let fetched_report: serde_json::Value = response_to_value(get_report_response.into_body()).await;
        assert_eq!(fetched_report["id"], report["id"]);

        // Verify sanitization
        assert!(fetched_report["priority"].is_null());
        assert!(fetched_report["resolved_by"].is_null());
    }

    #[tokio::test]
    async fn test_cannot_get_others_report() {
        let pool = PgPool::connect(&CONFIG.database_url).await.unwrap();
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let app_state = AppState {
            pool: pool.clone(),
            email_service: email_service_trait,
        };
        let mut app = crate::create_router(app_state).into_service();

        // Create user 1 and a report
        let reporter1_username = random_name();
        let reporter1_token = make_authed_user(&reporter1_username, &app, email_service.clone()).await;
        let reporter1_id = sqlx::query_scalar!(
            "select id from users where username = $1",
            reporter1_username
        )
        .fetch_one(&pool)
        .await.unwrap();

        let create_report_request = post(
            &reporter1_token,
            "reports",
            json!({
                "resource_type": "user",
                "resource_id": reporter1_id,
                "reason": "Test report"
            }),
        ).await;
        let create_report_response = app.ready().await.unwrap().call(create_report_request).await.unwrap();
        let report: serde_json::Value = response_to_value(create_report_response.into_body()).await;
        let report_id = report["id"].as_str().unwrap();

        // Create user 2
        let reporter2_username = random_name();
        let reporter2_token = make_authed_user(&reporter2_username, &app, email_service).await;

        // Try to get user 1's report as user 2
        let get_report_request = get_with_auth(&reporter2_token, &format!("reports/{}", report_id)).await;
        let get_report_response = app.call(get_report_request).await.unwrap();
        assert_eq!(get_report_response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_moderator_can_get_any_report() {
        let pool = PgPool::connect(&CONFIG.database_url).await.unwrap();
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let app_state = AppState {
            pool: pool.clone(),
            email_service: email_service_trait,
        };
        let mut app = crate::create_router(app_state).into_service();

        // Create a regular user and a report
        let reporter_username = random_name();
        let reporter_token = make_authed_user(&reporter_username, &app, email_service.clone()).await;
        let reporter_id = sqlx::query_scalar!(
            "select id from users where username = $1",
            reporter_username
        )
        .fetch_one(&pool)
        .await.unwrap();

        let create_report_request = post(
            &reporter_token,
            "reports",
            json!({
                "resource_type": "user",
                "resource_id": reporter_id,
                "reason": "Test report"
            }),
        ).await;
        let create_report_response = app.ready().await.unwrap().call(create_report_request).await.unwrap();
        let report: serde_json::Value = response_to_value(create_report_response.into_body()).await;
        let report_id = report["id"].as_str().unwrap();

        // Create a moderator
        let (moderator_token, _) = make_moderator_user(&app, email_service, &pool).await;

        // Moderator can get the report
        let get_report_request = get_with_auth(&moderator_token, &format!("reports/{}", report_id)).await;
        let get_report_response = app.call(get_report_request).await.unwrap();
        assert_eq!(get_report_response.status(), StatusCode::OK);

        let fetched_report: serde_json::Value = response_to_value(get_report_response.into_body()).await;

        // Moderators see all fields including priority
        assert!(!fetched_report["priority"].is_null());
        assert_eq!(fetched_report["priority"].as_str().unwrap(), "medium");
    }

    #[tokio::test]
    async fn test_search_all_reports_moderator_only() {
        let pool = PgPool::connect(&CONFIG.database_url).await.unwrap();
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let app_state = AppState {
            pool: pool.clone(),
            email_service: email_service_trait,
        };
        let mut app = crate::create_router(app_state).into_service();

        // Create a moderator
        let (moderator_token, _) = make_moderator_user(&app, email_service.clone(), &pool).await;

        // Create a regular user
        let regular_username = random_name();
        let regular_token = make_authed_user(&regular_username, &app, email_service.clone()).await;

        // Moderator can search all reports
        let search_request = get_with_auth(&moderator_token, "reports").await;
        let search_response = app.ready().await.unwrap().call(search_request).await.unwrap();
        assert_eq!(search_response.status(), StatusCode::OK);

        // Regular user cannot search all reports
        let search_request = get_with_auth(&regular_token, "reports").await;
        let search_response = app.call(search_request).await.unwrap();
        assert_eq!(search_response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_search_own_reports() {
        let pool = PgPool::connect(&CONFIG.database_url).await.unwrap();
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let app_state = AppState {
            pool: pool.clone(),
            email_service: email_service_trait,
        };
        let mut app = crate::create_router(app_state).into_service();

        // Create a user and multiple reports
        let reporter_username = random_name();
        let reporter_token = make_authed_user(&reporter_username, &app, email_service.clone()).await;
        let reporter_id = sqlx::query_scalar!(
            "select id from users where username = $1",
            reporter_username
        )
        .fetch_one(&pool)
        .await.unwrap();

        // Create 3 reports
        for i in 0..3 {
            let create_report_request = post(
                &reporter_token,
                "reports",
                json!({
                    "resource_type": "user",
                    "resource_id": reporter_id,
                    "reason": format!("Test report {}", i)
                }),
            ).await;
            app.ready().await.unwrap().call(create_report_request).await.unwrap();
        }

        // Search own reports
        let search_request = get_with_auth(&reporter_token, "reports/own").await;
        let search_response = app.call(search_request).await.unwrap();
        assert_eq!(search_response.status(), StatusCode::OK);

        let search_results: serde_json::Value = response_to_value(search_response.into_body()).await;
        let items = search_results["items"].as_array().unwrap();
        assert_eq!(items.len(), 3);

        // Verify all reports are sanitized
        for item in items {
            assert!(item["priority"].is_null());
            assert!(item["resolved_by"].is_null());
        }
    }

    #[tokio::test]
    async fn test_update_report_moderator_only() {
        let pool = PgPool::connect(&CONFIG.database_url).await.unwrap();
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let app_state = AppState {
            pool: pool.clone(),
            email_service: email_service_trait,
        };
        let mut app = crate::create_router(app_state).into_service();

        // Create a user and a report
        let reporter_username = random_name();
        let reporter_token = make_authed_user(&reporter_username, &app, email_service.clone()).await;
        let reporter_id = sqlx::query_scalar!(
            "select id from users where username = $1",
            reporter_username
        )
        .fetch_one(&pool)
        .await.unwrap();

        let create_report_request = post(
            &reporter_token,
            "reports",
            json!({
                "resource_type": "user",
                "resource_id": reporter_id,
                "reason": "Test report"
            }),
        ).await;
        let create_report_response = app.ready().await.unwrap().call(create_report_request).await.unwrap();
        let report: serde_json::Value = response_to_value(create_report_response.into_body()).await;
        let report_id = report["id"].as_str().unwrap();

        // Create a moderator
        let (moderator_token, _) = make_moderator_user(&app, email_service.clone(), &pool).await;

        // Moderator can update the report
        let update_request = patch(
            &moderator_token,
            &format!("reports/{}", report_id),
            json!({
                "priority": "high",
                "resolution_status": "in_progress",
                "resolution_note": "Looking into this"
            }),
        );
        let update_response = app.ready().await.unwrap().call(update_request).await.unwrap();
        assert_eq!(update_response.status(), StatusCode::OK);

        let updated_report: serde_json::Value = response_to_value(update_response.into_body()).await;
        assert_eq!(updated_report["priority"].as_str().unwrap(), "high");
        assert_eq!(updated_report["resolution_status"].as_str().unwrap(), "in_progress");

        // Regular user cannot update
        let update_request = patch(
            &reporter_token,
            &format!("reports/{}", report_id),
            json!({
                "priority": "low"
            }),
        );
        let update_response = app.call(update_request).await.unwrap();
        assert_eq!(update_response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_update_report_sets_resolved_fields() {
        let pool = PgPool::connect(&CONFIG.database_url).await.unwrap();
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let app_state = AppState {
            pool: pool.clone(),
            email_service: email_service_trait,
        };
        let mut app = crate::create_router(app_state).into_service();

        // Create a user and a report
        let reporter_username = random_name();
        let reporter_token = make_authed_user(&reporter_username, &app, email_service.clone()).await;
        let reporter_id = sqlx::query_scalar!(
            "select id from users where username = $1",
            reporter_username
        )
        .fetch_one(&pool)
        .await.unwrap();

        let create_report_request = post(
            &reporter_token,
            "reports",
            json!({
                "resource_type": "user",
                "resource_id": reporter_id,
                "reason": "Test report"
            }),
        ).await;
        let create_report_response = app.ready().await.unwrap().call(create_report_request).await.unwrap();
        let report: serde_json::Value = response_to_value(create_report_response.into_body()).await;
        let report_id = report["id"].as_str().unwrap();

        // Create a moderator
        let (moderator_token, moderator_id) = make_moderator_user(&app, email_service.clone(), &pool).await;

        // Mark report as dismissed
        let update_request = patch(
            &moderator_token,
            &format!("reports/{}", report_id),
            json!({
                "resolution_status": "dismissed"
            }),
        );
        let update_response = app.call(update_request).await.unwrap();
        assert_eq!(update_response.status(), StatusCode::OK);

        let updated_report: serde_json::Value = response_to_value(update_response.into_body()).await;

        // resolved_at should be set
        assert!(!updated_report["resolved_at"].is_null());

        // resolved_by should be set to the moderator
        assert_eq!(updated_report["resolved_by"].as_str().unwrap(), moderator_id.to_string());
    }

    #[tokio::test]
    async fn test_update_report_validation_constraint() {
        let pool = PgPool::connect(&CONFIG.database_url).await.unwrap();
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let app_state = AppState {
            pool: pool.clone(),
            email_service: email_service_trait,
        };
        let mut app = crate::create_router(app_state).into_service();

        // Create a user and a report
        let reporter_username = random_name();
        let reporter_token = make_authed_user(&reporter_username, &app, email_service.clone()).await;
        let reporter_id = sqlx::query_scalar!(
            "select id from users where username = $1",
            reporter_username
        )
        .fetch_one(&pool)
        .await.unwrap();

        let create_report_request = post(
            &reporter_token,
            "reports",
            json!({
                "resource_type": "user",
                "resource_id": reporter_id,
                "reason": "Test report"
            }),
        ).await;
        let create_report_response = app.ready().await.unwrap().call(create_report_request).await.unwrap();
        let report: serde_json::Value = response_to_value(create_report_response.into_body()).await;
        let report_id = report["id"].as_str().unwrap();

        // Create a moderator
        let (moderator_token, _) = make_moderator_user(&app, email_service.clone(), &pool).await;

        // Try to set resolution_note_hidden=false when resolution_status_hidden=true
        // This should fail the constraint
        let update_request = patch(
            &moderator_token,
            &format!("reports/{}", report_id),
            json!({
                "resolution_status_hidden": true,
                "resolution_note_hidden": false
            }),
        );
        let update_response = app.call(update_request).await.unwrap();
        assert_eq!(update_response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_delete_report_admin_only() {
        let pool = PgPool::connect(&CONFIG.database_url).await.unwrap();
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let app_state = AppState {
            pool: pool.clone(),
            email_service: email_service_trait,
        };
        let mut app = crate::create_router(app_state).into_service();

        // Create a user and a report
        let reporter_username = random_name();
        let reporter_token = make_authed_user(&reporter_username, &app, email_service.clone()).await;
        let reporter_id = sqlx::query_scalar!(
            "select id from users where username = $1",
            reporter_username
        )
        .fetch_one(&pool)
        .await.unwrap();

        let create_report_request = post(
            &reporter_token,
            "reports",
            json!({
                "resource_type": "user",
                "resource_id": reporter_id,
                "reason": "Test report"
            }),
        ).await;
        let create_report_response = app.ready().await.unwrap().call(create_report_request).await.unwrap();
        let report: serde_json::Value = response_to_value(create_report_response.into_body()).await;
        let report_id = report["id"].as_str().unwrap();

        // Regular user cannot delete
        let delete_request = delete(&reporter_token, &format!("reports/{}", report_id));
        let delete_response = app.ready().await.unwrap().call(delete_request).await.unwrap();
        assert_eq!(delete_response.status(), StatusCode::FORBIDDEN);

        // Moderator cannot delete (only admins)
        let (moderator_token, _) = make_moderator_user(&app, email_service.clone(), &pool).await;
        let delete_request = delete(&moderator_token, &format!("reports/{}", report_id));
        let delete_response = app.ready().await.unwrap().call(delete_request).await.unwrap();
        assert_eq!(delete_response.status(), StatusCode::FORBIDDEN);

        // Admin can delete
        let (admin_token, _) = make_admin_user(&app, email_service, &pool).await;
        let delete_request = delete(&admin_token, &format!("reports/{}", report_id));
        let delete_response = app.ready().await.unwrap().call(delete_request).await.unwrap();
        assert_eq!(delete_response.status(), StatusCode::OK);

        // Verify report was deleted
        let get_request = get_with_auth(&admin_token, &format!("reports/{}", report_id)).await;
        let get_response = app.call(get_request).await.unwrap();
        assert_eq!(get_response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_report_field_sanitization() {
        let pool = PgPool::connect(&CONFIG.database_url).await.unwrap();
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let app_state = AppState {
            pool: pool.clone(),
            email_service: email_service_trait,
        };
        let mut app = crate::create_router(app_state).into_service();

        // Create a user and a report
        let reporter_username = random_name();
        let reporter_token = make_authed_user(&reporter_username, &app, email_service.clone()).await;
        let reporter_id = sqlx::query_scalar!(
            "select id from users where username = $1",
            reporter_username
        )
        .fetch_one(&pool)
        .await.unwrap();

        let create_report_request = post(
            &reporter_token,
            "reports",
            json!({
                "resource_type": "user",
                "resource_id": reporter_id,
                "reason": "Test report"
            }),
        ).await;
        let create_report_response = app.ready().await.unwrap().call(create_report_request).await.unwrap();
        let report: serde_json::Value = response_to_value(create_report_response.into_body()).await;
        let report_id = report["id"].as_str().unwrap();

        // Create a moderator and update the report with hidden/visible flags
        let (moderator_token, _) = make_moderator_user(&app, email_service.clone(), &pool).await;

        // Update with resolution_status visible but resolution_note hidden
        let update_request = patch(
            &moderator_token,
            &format!("reports/{}", report_id),
            json!({
                "resolution_status": "in_progress",
                "resolution_note": "Working on it",
                "resolution_status_hidden": false,
                "resolution_note_hidden": true
            }),
        );
        app.ready().await.unwrap().call(update_request).await.unwrap();

        // Regular user should see resolution_status but not resolution_note
        let get_request = get_with_auth(&reporter_token, &format!("reports/{}", report_id)).await;
        let get_response = app.call(get_request).await.unwrap();
        let fetched_report: serde_json::Value = response_to_value(get_response.into_body()).await;

        assert_eq!(fetched_report["resolution_status"].as_str().unwrap(), "in_progress");
        assert!(fetched_report["resolution_note"].is_null()); // Hidden
        assert!(fetched_report["priority"].is_null()); // Always hidden for regular users
    }

    #[tokio::test]
    async fn test_search_reports_with_filters() {
        let pool = PgPool::connect(&CONFIG.database_url).await.unwrap();
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let app_state = AppState {
            pool: pool.clone(),
            email_service: email_service_trait,
        };
        let mut app = crate::create_router(app_state).into_service();

        // Create two users and reports
        let reporter1_username = random_name();
        let reporter1_token = make_authed_user(&reporter1_username, &app, email_service.clone()).await;
        let reporter1_id = sqlx::query_scalar!(
            "select id from users where username = $1",
            reporter1_username
        )
        .fetch_one(&pool)
        .await.unwrap();

        // Create reports with different resource types
        let create_report1 = post(
            &reporter1_token,
            "reports",
            json!({
                "resource_type": "user",
                "resource_id": reporter1_id,
                "reason": "spam user"
            }),
        ).await;
        app.ready().await.unwrap().call(create_report1).await.unwrap();

        let create_report2 = post(
            &reporter1_token,
            "reports",
            json!({
                "resource_type": "language",
                "resource_id": Uuid::new_v4(),
                "reason": "inappropriate language"
            }),
        ).await;
        app.ready().await.unwrap().call(create_report2).await.unwrap();

        // Create a moderator
        let (moderator_token, _) = make_moderator_user(&app, email_service.clone(), &pool).await;

        // Search for User reports only
        let search_request = get_with_auth(&moderator_token, "reports?resource_type=user").await;
        let search_response = app.call(search_request).await.unwrap();
        assert_eq!(search_response.status(), StatusCode::OK);

        let search_results: serde_json::Value = response_to_value(search_response.into_body()).await;
        let items = search_results["items"].as_array().unwrap();

        // Should only get User reports
        for item in items {
            assert_eq!(item["resource_type"].as_str().unwrap(), "user");
        }
    }
}
