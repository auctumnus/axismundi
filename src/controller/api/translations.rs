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
use validator::Validate;

pub fn create_router() -> axum::Router<crate::util::AppState> {
    axum::Router::new()
        .route(
            "/translatable/{translatable_slug}/languages/{code}/translations",
            post(create_translation),
        )
        .route(
            "/translatable/{translatable_slug}/translations",
            get(list_translations_by_translatable),
        )
        .route(
            "/languages/{code}/translations",
            get(list_translations_by_language),
        )
        .route("/translatable/{translatable_slug}/translations/{code}", get(get_translation))
        .route("/translatable/{translatable_slug}/translations/{code}", put(edit_translation))
        .route("/translatable/{translatable_slug}/translations/{code}", delete(delete_translation))
}

type ApiResponse<T> = AppResult<T>;
type PaginatedApiResponse<T> = AppResult<PaginatedResponse<T>>;

pub async fn create_translation(
    s: Session,
    Path((translatable_slug, code)): Path<(String, String)>,
    translatables: TranslatableRepository,
    languages: LanguageRepository,
    translations: TranslationRepository,
    Json(req): Json<CreateTranslation>,
) -> ApiResponse<Json<Translation>> {
    req.validate()?;

    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    // Look up translatable by slug
    let translatable = translatables.find_by_slug(&translatable_slug).await?;

    let language = languages.find_by_code(&code).await?;

    translations
        .create(requestor, translatable.id, language.id, req)
        .await
        .map(Json)
}

pub async fn get_translation(
    Path((translatable_slug, code)): Path<(String, String)>,
    translatables: TranslatableRepository,
    languages: LanguageRepository,
    translations: TranslationRepository,
) -> ApiResponse<Json<Translation>> {
    // Look up translatable by slug
    let translatable = translatables.find_by_slug(&translatable_slug).await?;

    // Look up language by code
    let language = languages.find_by_code(&code).await?;

    // Find translation by translatable and language
    let translation = translations
        .find_by_translatable_and_language(translatable.id, language.id)
        .await?;

    match translation {
        Some(t) => Ok(Json(t)),
        None => Err(crate::err::not_found(format!(
            "translation for translatable '{}' in language '{}'",
            translatable_slug, code
        ))),
    }
}

pub async fn list_translations_by_translatable(
    translatables: TranslatableRepository,
    translations: TranslationRepository,
    Path(translatable_slug): Path<String>,
    pagination: PaginatedRequest,
) -> PaginatedApiResponse<Translation> {
    // Look up translatable by slug
    let translatable = translatables.find_by_slug(&translatable_slug).await?;

    translations
        .list_by_translatable(translatable.id, pagination)
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
    Path((translatable_slug, code)): Path<(String, String)>,
    translatables: TranslatableRepository,
    languages: LanguageRepository,
    translations: TranslationRepository,
    Json(updates): Json<UpdateTranslation>,
) -> ApiResponse<Json<Translation>> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    // Look up translatable by slug
    let translatable = translatables.find_by_slug(&translatable_slug).await?;

    // Look up language by code
    let language = languages.find_by_code(&code).await?;

    // Find existing translation
    let existing = translations
        .find_by_translatable_and_language(translatable.id, language.id)
        .await?;

    let Some(translation) = existing else {
        return Err(crate::err::not_found(format!(
            "translation for translatable '{}' in language '{}'",
            translatable_slug, code
        )));
    };

    translations
        .update(requestor, translation.id, updates)
        .await
        .map(Json)
}

pub async fn delete_translation(
    s: Session,
    Path((translatable_slug, code)): Path<(String, String)>,
    translatables: TranslatableRepository,
    languages: LanguageRepository,
    translations: TranslationRepository,
) -> ApiResponse<StatusCode> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    // Look up translatable by slug
    let translatable = translatables.find_by_slug(&translatable_slug).await?;

    // Look up language by code
    let language = languages.find_by_code(&code).await?;

    // Find existing translation
    let existing = translations
        .find_by_translatable_and_language(translatable.id, language.id)
        .await?;

    let Some(translation) = existing else {
        return Err(crate::err::not_found(format!(
            "translation for translatable '{}' in language '{}'",
            translatable_slug, code
        )));
    };

    translations.delete(requestor, translation.id).await?;

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

    async fn create_test_language(token: &str, app: &mut axum::routing::RouterIntoService<axum::body::Body>) -> serde_json::Value {
        let code = crate::tests::random_code();
        let body = json!({
            "code": code,
            "name": "Test Language",
        });
        let request = post(token, "languages", body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        crate::tests::response_to_value(response.into_body()).await
    }

    async fn create_test_translatable(token: &str, app: &mut axum::routing::RouterIntoService<axum::body::Body>) -> serde_json::Value {
        let body = json!({
            "title": "Test translatable title",
            "english": "Test translatable text for translation",
        });
        let request = post(token, "translatable", body).await;
        let response = app.call(request).await.unwrap();
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

        let language = create_test_language(&token, &mut app).await;
        let language_code = language["code"].as_str().unwrap();

        let translatable = create_test_translatable(&token, &mut app).await;
        let translatable_slug = translatable["slug"].as_str().unwrap();

        let body = json!({
            "translated_text": "This is a translated text",
        });

        let request = post(&token, &format!("translatable/{}/languages/{}/translations", translatable_slug, language_code), body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(body["translated_text"], "This is a translated text");
        assert!(body["id"].is_string());
    }

    #[tokio::test]
    async fn test_create_translation_unauthorized() {
        let mut app = crate::tests::test_app().await.unwrap();

        let body = json!({
            "translated_text": "Should fail",
        });

        let request = crate::controller::api::tests::post_without_auth("translatable/nonexistent-slug/languages/en/translations", body).await;
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

        let language = create_test_language(&token, &mut app).await;
        let language_code = language["code"].as_str().unwrap();

        let translatable = create_test_translatable(&token, &mut app).await;
        let translatable_slug = translatable["slug"].as_str().unwrap();

        let body = json!({
            "translated_text": "Specific translation text",
        });

        let request = post(&token, &format!("translatable/{}/languages/{}/translations", translatable_slug, language_code), body).await;
        let response = app.call(request).await.unwrap();
        let _created = crate::tests::response_to_value(response.into_body()).await;

        let request = get(&format!("translatable/{}/translations/{}", translatable_slug, language_code)).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(body["translated_text"], "Specific translation text");
    }

    #[tokio::test]
    async fn test_get_translation_not_found() {
        let mut app = crate::tests::test_app().await.unwrap();

        let request = get("translatable/nonexistent-slug/translations/en").await;
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

        let language = create_test_language(&token, &mut app).await;
        let language_code = language["code"].as_str().unwrap();

        let translatable = create_test_translatable(&token, &mut app).await;
        let translatable_slug = translatable["slug"].as_str().unwrap();

        // Create a translation
        let body = json!({
            "translated_text": "Translation 1",
        });
        let request = post(&token, &format!("translatable/{}/languages/{}/translations", translatable_slug, language_code), body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let request = get(&format!("translatable/{}/translations", translatable_slug)).await;
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

        let language = create_test_language(&token, &mut app).await;
        let language_code = language["code"].as_str().unwrap();

        let translatable = create_test_translatable(&token, &mut app).await;
        let translatable_slug = translatable["slug"].as_str().unwrap();

        // Create a translation
        let body = json!({
            "translated_text": "Translation in language",
        });
        let request = post(&token, &format!("translatable/{}/languages/{}/translations", translatable_slug, language_code), body).await;
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

        let language = create_test_language(&token, &mut app).await;
        let language_code = language["code"].as_str().unwrap();

        let translatable = create_test_translatable(&token, &mut app).await;
        let translatable_slug = translatable["slug"].as_str().unwrap();

        let body = json!({
            "translated_text": "Original translation",
        });

        let request = post(&token, &format!("translatable/{}/languages/{}/translations", translatable_slug, language_code), body).await;
        let response = app.call(request).await.unwrap();
        let _created = crate::tests::response_to_value(response.into_body()).await;

        let update_body = json!({
            "translated_text": "Updated translation",
        });

        let request = crate::controller::api::tests::put(&token, &format!("translatable/{}/translations/{}", translatable_slug, language_code), &update_body);
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(body["translated_text"], "Updated translation");
    }

    #[tokio::test]
    async fn test_edit_translation_unauthorized() {
        let mut app = crate::tests::test_app().await.unwrap();

        let update_body = json!({
            "translated_text": "Should not work",
        });

        let request = crate::controller::api::tests::put_without_auth("translatable/nonexistent-slug/translations/en", &update_body);
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

        let language = create_test_language(&token, &mut app).await;
        let language_code = language["code"].as_str().unwrap();

        let translatable = create_test_translatable(&token, &mut app).await;
        let translatable_slug = translatable["slug"].as_str().unwrap();

        let body = json!({
            "translated_text": "To be deleted",
        });

        let request = post(&token, &format!("translatable/{}/languages/{}/translations", translatable_slug, language_code), body).await;
        let response = app.call(request).await.unwrap();
        let _created = crate::tests::response_to_value(response.into_body()).await;

        let request = delete(&token, &format!("translatable/{}/translations/{}", translatable_slug, language_code));
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        // Verify it's deleted
        let request = get(&format!("translatable/{}/translations/{}", translatable_slug, language_code)).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_delete_translation_unauthorized() {
        let mut app = crate::tests::test_app().await.unwrap();

        let request = crate::controller::api::tests::delete_without_auth("translatable/nonexistent-slug/translations/en");
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
