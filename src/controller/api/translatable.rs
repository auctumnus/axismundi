use crate::{
    err::{unauthorized_no_session, AppResult},
    model::translatable::{
        CreateTranslatable, Translatable, TranslatableRepository, TranslatableSearch, UpdateTranslatable,
    },
    pagination::{PaginatedRequest, PaginatedResponse},
    util::extract_session::Session,
};
use axum::{
    extract::Path,
    http::StatusCode,
    routing::{delete, get, post, put},
    Json,
};
use uuid::Uuid;
use validator::Validate;

pub fn create_router() -> axum::Router<crate::util::AppState> {
    axum::Router::new()
        .route("/translatable", post(create_translatable))
        .route("/translatable", get(search_translatable))
        .route("/translatable/{id}", get(get_translatable))
        .route("/translatable/{id}", put(edit_translatable))
        .route("/translatable/{id}", delete(delete_translatable))
}

type ApiResponse<T> = AppResult<T>;
type PaginatedApiResponse<T> = AppResult<PaginatedResponse<T>>;

pub async fn create_translatable(
    s: Session,
    translatables: TranslatableRepository,
    Json(req): Json<CreateTranslatable>,
) -> ApiResponse<Json<Translatable>> {
    req.validate()?;

    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    translatables.create(requestor, req).await.map(Json)
}

pub async fn get_translatable(
    translatables: TranslatableRepository,
    Path(id): Path<Uuid>,
) -> ApiResponse<Json<Translatable>> {
    let translatable = translatables.find_by_id(id).await?;
    Ok(Json(translatable))
}

pub async fn search_translatable(
    translatables: TranslatableRepository,
    pagination: PaginatedRequest,
    axum::extract::Query(query): axum::extract::Query<TranslatableSearch>,
) -> PaginatedApiResponse<Translatable> {
    translatables.search(pagination, query).await
}

pub async fn edit_translatable(
    s: Session,
    translatables: TranslatableRepository,
    Path(id): Path<Uuid>,
    Json(updates): Json<UpdateTranslatable>,
) -> ApiResponse<Json<Translatable>> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    translatables.update(requestor, id, updates).await.map(Json)
}

pub async fn delete_translatable(
    s: Session,
    translatables: TranslatableRepository,
    Path(id): Path<Uuid>,
) -> ApiResponse<StatusCode> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    translatables.delete(requestor, id).await?;

    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use serde_json::json;
    use std::sync::Arc;
    use tower::Service;

    use crate::controller::api::tests::{delete, get, make_authed_user, post};
    use crate::email::MockEmailService;

    #[tokio::test]
    async fn test_create_translatable() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let username = crate::tests::random_name();
        let token = make_authed_user(&username, &app, email_service.clone()).await;

        let body = json!({
            "text": "This is a test translatable text",
        });

        let request = post(&token, "translatable", body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(body["text"], "This is a test translatable text");
        assert!(body["id"].is_string());
    }

    #[tokio::test]
    async fn test_create_translatable_unauthorized() {
        let mut app = crate::tests::test_app().await.unwrap();

        let body = json!({
            "text": "Should fail",
        });

        let request = crate::controller::api::tests::post_without_auth("translatable", body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_get_translatable() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let username = crate::tests::random_name();
        let token = make_authed_user(&username, &app, email_service.clone()).await;

        let body = json!({
            "text": "Specific translatable text",
        });

        let request = post(&token, "translatable", body).await;
        let response = app.call(request).await.unwrap();
        let created = crate::tests::response_to_value(response.into_body()).await;
        let translatable_id = created["id"].as_str().unwrap();

        let request = get(&format!("translatable/{}", translatable_id)).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(body["id"], translatable_id);
        assert_eq!(body["text"], "Specific translatable text");
    }

    #[tokio::test]
    async fn test_get_translatable_not_found() {
        let mut app = crate::tests::test_app().await.unwrap();

        let request = get("translatable/00000000-0000-0000-0000-000000000000").await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_search_translatable() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let username = crate::tests::random_name();
        let token = make_authed_user(&username, &app, email_service.clone()).await;

        // Create multiple translatables
        for i in 0..3 {
            let body = json!({
                "text": format!("Translatable text number {}", i),
            });
            let request = post(&token, "translatable", body).await;
            let response = app.call(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
        }

        let request = get("translatable").await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert!(body["items"].is_array());
    }

    #[tokio::test]
    async fn test_edit_translatable() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let username = crate::tests::random_name();
        let token = make_authed_user(&username, &app, email_service.clone()).await;

        let body = json!({
            "text": "Original text",
        });

        let request = post(&token, "translatable", body).await;
        let response = app.call(request).await.unwrap();
        let created = crate::tests::response_to_value(response.into_body()).await;
        let translatable_id = created["id"].as_str().unwrap();

        let update_body = json!({
            "text": "Updated text",
        });

        let request = crate::controller::api::tests::put(&token, &format!("translatable/{}", translatable_id), &update_body);
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(body["text"], "Updated text");
    }

    #[tokio::test]
    async fn test_edit_translatable_unauthorized() {
        let mut app = crate::tests::test_app().await.unwrap();

        let update_body = json!({
            "text": "Should not work",
        });

        let request = crate::controller::api::tests::put_without_auth("translatable/00000000-0000-0000-0000-000000000000", &update_body);
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_delete_translatable() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let username = crate::tests::random_name();
        let token = make_authed_user(&username, &app, email_service.clone()).await;

        let body = json!({
            "text": "To be deleted",
        });

        let request = post(&token, "translatable", body).await;
        let response = app.call(request).await.unwrap();
        let created = crate::tests::response_to_value(response.into_body()).await;
        let translatable_id = created["id"].as_str().unwrap();

        let request = delete(&token, &format!("translatable/{}", translatable_id));
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        // Verify it's deleted
        let request = get(&format!("translatable/{}", translatable_id)).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_delete_translatable_unauthorized() {
        let mut app = crate::tests::test_app().await.unwrap();

        let request = crate::controller::api::tests::delete_without_auth("translatable/00000000-0000-0000-0000-000000000000");
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
