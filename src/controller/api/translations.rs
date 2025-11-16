use crate::{
    err::{unauthorized_no_session, AppResult},
    model::{
        languages::LanguageRepository,
        translatable::TranslatableRepository,
        translations::{CreateTranslation, Translation, TranslationRepository, UpdateTranslation},
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
        .route(
            "/translatable/{translatable_id}/languages/{code}/translations",
            post(create_translation),
        )
        .route(
            "/translatable/{translatable_id}/translations",
            get(list_translations_by_translatable),
        )
        .route(
            "/languages/{code}/translations",
            get(list_translations_by_language),
        )
        .route("/translations/{id}", get(get_translation))
        .route("/translations/{id}", put(edit_translation))
        .route("/translations/{id}", delete(delete_translation))
}

type ApiResponse<T> = AppResult<T>;
type PaginatedApiResponse<T> = AppResult<PaginatedResponse<T>>;

pub async fn create_translation(
    s: Session,
    Path((translatable_id, code)): Path<(Uuid, String)>,
    translatables: TranslatableRepository,
    languages: LanguageRepository,
    translations: TranslationRepository,
    Json(req): Json<CreateTranslation>,
) -> ApiResponse<Json<Translation>> {
    req.validate()?;

    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    // Verify translatable exists
    translatables.find_by_id(translatable_id).await?;

    let language = languages.find_by_code(&code).await?;

    translations
        .create(requestor, translatable_id, language.id, req)
        .await
        .map(Json)
}

pub async fn get_translation(
    translations: TranslationRepository,
    Path(id): Path<Uuid>,
) -> ApiResponse<Json<Translation>> {
    let translation = translations.find_by_id(id).await?;
    Ok(Json(translation))
}

pub async fn list_translations_by_translatable(
    translatables: TranslatableRepository,
    translations: TranslationRepository,
    Path(translatable_id): Path<Uuid>,
    pagination: PaginatedRequest,
) -> PaginatedApiResponse<Translation> {
    // Verify translatable exists
    translatables.find_by_id(translatable_id).await?;

    translations
        .list_by_translatable(translatable_id, pagination)
        .await
}

pub async fn list_translations_by_language(
    languages: LanguageRepository,
    translations: TranslationRepository,
    Path(code): Path<String>,
    pagination: PaginatedRequest,
) -> PaginatedApiResponse<Translation> {
    let language = languages.find_by_code(&code).await?;

    translations.list_by_language(language.id, pagination).await
}

pub async fn edit_translation(
    s: Session,
    translations: TranslationRepository,
    Path(id): Path<Uuid>,
    Json(updates): Json<UpdateTranslation>,
) -> ApiResponse<Json<Translation>> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    translations.update(requestor, id, updates).await.map(Json)
}

pub async fn delete_translation(
    s: Session,
    translations: TranslationRepository,
    Path(id): Path<Uuid>,
) -> ApiResponse<StatusCode> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    translations.delete(requestor, id).await?;

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
    use tower::ServiceExt;

    async fn create_test_language(token: &str, app: &axum::routing::RouterIntoService<axum::body::Body>) -> serde_json::Value {
        let code = crate::tests::random_code();
        let body = json!({
            "code": code,
            "name": "Test Language",
        });
        let request = post(token, "languages", body).await;
        let response = ServiceExt::oneshot(app.clone(), request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        crate::tests::response_to_value(response.into_body()).await
    }

    async fn create_test_translatable(token: &str, app: &axum::routing::RouterIntoService<axum::body::Body>) -> serde_json::Value {
        let body = json!({
            "text": "Test translatable text for translation",
        });
        let request = post(token, "translatable", body).await;
        let response = ServiceExt::oneshot(app.clone(), request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        crate::tests::response_to_value(response.into_body()).await
    }

    #[tokio::test]
    async fn test_create_translation() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let username = crate::tests::random_name();
        let token = make_authed_user(&username, &app, email_service.clone()).await;

        let language = create_test_language(&token, &app).await;
        let language_code = language["code"].as_str().unwrap();

        let translatable = create_test_translatable(&token, &app).await;
        let translatable_id = translatable["id"].as_str().unwrap();

        let body = json!({
            "text": "This is a translated text",
        });

        let request = post(&token, &format!("translatable/{}/languages/{}/translations", translatable_id, language_code), body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(body["text"], "This is a translated text");
        assert!(body["id"].is_string());
    }

    #[tokio::test]
    async fn test_create_translation_unauthorized() {
        let mut app = crate::tests::test_app().await.unwrap();

        let body = json!({
            "text": "Should fail",
        });

        let request = crate::controller::api::tests::post_without_auth("translatable/00000000-0000-0000-0000-000000000000/languages/en/translations", body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_get_translation() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let username = crate::tests::random_name();
        let token = make_authed_user(&username, &app, email_service.clone()).await;

        let language = create_test_language(&token, &app).await;
        let language_code = language["code"].as_str().unwrap();

        let translatable = create_test_translatable(&token, &app).await;
        let translatable_id = translatable["id"].as_str().unwrap();

        let body = json!({
            "text": "Specific translation text",
        });

        let request = post(&token, &format!("translatable/{}/languages/{}/translations", translatable_id, language_code), body).await;
        let response = app.call(request).await.unwrap();
        let created = crate::tests::response_to_value(response.into_body()).await;
        let translation_id = created["id"].as_str().unwrap();

        let request = get(&format!("translations/{}", translation_id)).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(body["id"], translation_id);
        assert_eq!(body["text"], "Specific translation text");
    }

    #[tokio::test]
    async fn test_get_translation_not_found() {
        let mut app = crate::tests::test_app().await.unwrap();

        let request = get("translations/00000000-0000-0000-0000-000000000000").await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_list_translations_by_translatable() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let username = crate::tests::random_name();
        let token = make_authed_user(&username, &app, email_service.clone()).await;

        let language = create_test_language(&token, &app).await;
        let language_code = language["code"].as_str().unwrap();

        let translatable = create_test_translatable(&token, &app).await;
        let translatable_id = translatable["id"].as_str().unwrap();

        // Create a translation
        let body = json!({
            "text": "Translation 1",
        });
        let request = post(&token, &format!("translatable/{}/languages/{}/translations", translatable_id, language_code), body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let request = get(&format!("translatable/{}/translations", translatable_id)).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert!(body["items"].is_array());
        assert_eq!(body["items"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_list_translations_by_language() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let username = crate::tests::random_name();
        let token = make_authed_user(&username, &app, email_service.clone()).await;

        let language = create_test_language(&token, &app).await;
        let language_code = language["code"].as_str().unwrap();

        let translatable = create_test_translatable(&token, &app).await;
        let translatable_id = translatable["id"].as_str().unwrap();

        // Create a translation
        let body = json!({
            "text": "Translation in language",
        });
        let request = post(&token, &format!("translatable/{}/languages/{}/translations", translatable_id, language_code), body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let request = get(&format!("languages/{}/translations", language_code)).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert!(body["items"].is_array());
    }

    #[tokio::test]
    async fn test_edit_translation() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let username = crate::tests::random_name();
        let token = make_authed_user(&username, &app, email_service.clone()).await;

        let language = create_test_language(&token, &app).await;
        let language_code = language["code"].as_str().unwrap();

        let translatable = create_test_translatable(&token, &app).await;
        let translatable_id = translatable["id"].as_str().unwrap();

        let body = json!({
            "text": "Original translation",
        });

        let request = post(&token, &format!("translatable/{}/languages/{}/translations", translatable_id, language_code), body).await;
        let response = app.call(request).await.unwrap();
        let created = crate::tests::response_to_value(response.into_body()).await;
        let translation_id = created["id"].as_str().unwrap();

        let update_body = json!({
            "text": "Updated translation",
        });

        let request = crate::controller::api::tests::put(&token, &format!("translations/{}", translation_id), &update_body);
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(body["text"], "Updated translation");
    }

    #[tokio::test]
    async fn test_edit_translation_unauthorized() {
        let mut app = crate::tests::test_app().await.unwrap();

        let update_body = json!({
            "text": "Should not work",
        });

        let request = crate::controller::api::tests::put_without_auth("translations/00000000-0000-0000-0000-000000000000", &update_body);
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_delete_translation() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let username = crate::tests::random_name();
        let token = make_authed_user(&username, &app, email_service.clone()).await;

        let language = create_test_language(&token, &app).await;
        let language_code = language["code"].as_str().unwrap();

        let translatable = create_test_translatable(&token, &app).await;
        let translatable_id = translatable["id"].as_str().unwrap();

        let body = json!({
            "text": "To be deleted",
        });

        let request = post(&token, &format!("translatable/{}/languages/{}/translations", translatable_id, language_code), body).await;
        let response = app.call(request).await.unwrap();
        let created = crate::tests::response_to_value(response.into_body()).await;
        let translation_id = created["id"].as_str().unwrap();

        let request = delete(&token, &format!("translations/{}", translation_id));
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        // Verify it's deleted
        let request = get(&format!("translations/{}", translation_id)).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_delete_translation_unauthorized() {
        let mut app = crate::tests::test_app().await.unwrap();

        let request = crate::controller::api::tests::delete_without_auth("translations/00000000-0000-0000-0000-000000000000");
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
