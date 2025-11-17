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
        .route("/translatable/{slug}", get(get_translatable))
        .route("/translatable/{slug}", put(edit_translatable))
        .route("/translatable/{slug}", delete(delete_translatable))
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
    Path(slug): Path<String>,
) -> ApiResponse<Json<Translatable>> {
    let translatable = translatables.find_by_slug(&slug).await?;
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
    Path(slug): Path<String>,
    Json(updates): Json<UpdateTranslatable>,
) -> ApiResponse<Json<Translatable>> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    let id = translatables.find_by_slug(&slug).await?.id;

    translatables.update(requestor, id, updates).await.map(Json)
}

pub async fn delete_translatable(
    s: Session,
    translatables: TranslatableRepository,
    Path(slug): Path<String>,
) -> ApiResponse<StatusCode> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    let translatable = translatables.find_by_slug(&slug).await?;

    translatables.delete(requestor, translatable).await?;

    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use serde_json::json;
    use std::sync::Arc;
    use tower::Service;

    use crate::controller::api::tests::{delete, delete_without_auth, get, make_authed_user, post, put, put_without_auth};
    use crate::email::MockEmailService;

    struct TestContext {
        token: String,
        app: axum::routing::RouterIntoService<axum::body::Body>,
    }

    async fn create_test_context() -> TestContext {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let username = crate::tests::random_name();
        let token = make_authed_user(&username, &app, email_service.clone()).await;

        TestContext { token, app }
    }

    async fn create_test_translatable(
        token: &str,
        app: &mut axum::routing::RouterIntoService<axum::body::Body>,
    ) -> serde_json::Value {
        let body = json!({
            "title": "test translatable",
            "english": "This is a test translatable text.",
        });
        let request = post(token, "translatable", body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        crate::tests::response_to_value(response.into_body()).await
    }

    #[tokio::test]
    async fn test_create_translatable() {
        let mut ctx = create_test_context().await;
        let translatable = create_test_translatable(&ctx.token, &mut ctx.app).await;
        assert_eq!(translatable["title"], "test translatable");
        assert_eq!(translatable["english"], "This is a test translatable text.");
    }

    #[tokio::test]
    async fn test_create_translatable_unauthorized() {
        let mut app = crate::tests::test_app().await.unwrap();

        let body = json!({
            "title": "test translatable",
            "english": "This is a test translatable text.",
        });

        let request = crate::controller::api::tests::post_without_auth("translatable", body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_get_translatable() {
        let mut ctx = create_test_context().await;
        let translatable = create_test_translatable(&ctx.token, &mut ctx.app).await;
        let slug = translatable["slug"].as_str().unwrap();

        let request = get(&format!("translatable/{}", slug)).await;
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_get_translatable_not_found() {
        let mut app = crate::tests::test_app().await.unwrap();

        let request = get("translatable/awawa").await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_search_translatable() {
        let mut ctx = create_test_context().await;
        let _translatable = create_test_translatable(&ctx.token, &mut ctx.app).await;

        let request = get("translatable?limit=10&offset=0").await;
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_edit_translatable() {
        let mut ctx = create_test_context().await;
        let translatable = create_test_translatable(&ctx.token, &mut ctx.app).await;
        let slug = translatable["slug"].as_str().unwrap();

        let update_body = json!({
            "english": "Updated translatable text.",
        });
        let request = put(&ctx.token, &format!("translatable/{}", slug), &update_body);
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_edit_translatable_unauthorized() {
        let mut ctx = create_test_context().await;
        let translatable = create_test_translatable(&ctx.token, &mut ctx.app).await;
        let slug = translatable["slug"].as_str().unwrap();

        let update_body = json!({
            "english": "Updated translatable text.",
        });
        let request = put_without_auth(&format!("translatable/{}", slug), &update_body);
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_delete_translatable() {
        let mut ctx = create_test_context().await;
        let translatable = create_test_translatable(&ctx.token, &mut ctx.app).await;
        let slug = translatable["slug"].as_str().unwrap();

        let request = delete(&ctx.token, &format!("translatable/{}", slug));
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn test_delete_translatable_unauthorized() {
        let mut ctx = create_test_context().await;
        let translatable = create_test_translatable(&ctx.token, &mut ctx.app).await;
        let slug = translatable["slug"].as_str().unwrap();

        let request = delete_without_auth(&format!("translatable/{}", slug));
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}