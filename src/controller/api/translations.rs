use crate::{
    err::{AppResult, unauthorized_no_session},
    model::{
        languages::LanguageRepository,
        translatable::TranslatableRepository,
        translations::{CreateTranslation, Translation, TranslationRepository, UpdateTranslation},
    },
    pagination::{PaginatedRequest, PaginatedResponse},
    util::extract_session::Session,
};
use axum::{
    Json,
    extract::Path,
    http::StatusCode,
    routing::{delete, get, post, put},
};
use validator::Validate;

pub fn create_router() -> axum::Router<crate::util::AppState> {
    axum::Router::new()
        .route(
            "/translatable/{translatable_slug}/translations/{code}",
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
        .route(
            "/translatable/{translatable_slug}/translations/{code}",
            get(get_translation),
        )
        .route(
            "/translatable/{translatable_slug}/translations/{code}",
            put(edit_translation),
        )
        .route(
            "/translatable/{translatable_slug}/translations/{code}",
            delete(delete_translation),
        )
        .route(
            "/translatable/{translatable_slug}/translations/{code}/like",
            post(like_translation),
        )
        .route(
            "/translatable/{translatable_slug}/translations/{code}/unlike",
            post(unlike_translation),
        )
}

type ApiResponse<T> = AppResult<T>;
type PaginatedApiResponse<T> = AppResult<PaginatedResponse<T>>;

#[derive(serde::Serialize)]
pub struct LikeTranslationResponse {
    pub liked: bool,
    pub like_count: i64,
}

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

    Ok(Json(translation))
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
    let translation = translations
        .find_by_translatable_and_language(translatable.id, language.id)
        .await?;

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
    let translation = translations
        .find_by_translatable_and_language(translatable.id, language.id)
        .await?;

    translations.delete(requestor, translation.id).await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn like_translation(
    s: Session,
    Path((translatable_slug, code)): Path<(String, String)>,
    translatables: TranslatableRepository,
    languages: LanguageRepository,
    translations: TranslationRepository,
) -> ApiResponse<Json<LikeTranslationResponse>> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    // Look up translatable by slug
    let translatable = translatables.find_by_slug(&translatable_slug).await?;

    // Look up language by code
    let language = languages.find_by_code(&code).await?;

    // Find existing translation
    let translation = translations
        .find_by_translatable_and_language(translatable.id, language.id)
        .await?;

    let like_count = translations
        .like_translation(translation.id, requestor.id)
        .await?;
    let response = LikeTranslationResponse {
        liked: true,
        like_count: like_count.unwrap_or(translation.like_count),
    };
    Ok(Json(response))
}

pub async fn unlike_translation(
    s: Session,
    Path((translatable_slug, code)): Path<(String, String)>,
    translatables: TranslatableRepository,
    languages: LanguageRepository,
    translations: TranslationRepository,
) -> ApiResponse<Json<LikeTranslationResponse>> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    // Look up translatable by slug
    let translatable = translatables.find_by_slug(&translatable_slug).await?;

    // Look up language by code
    let language = languages.find_by_code(&code).await?;

    // Find existing translation
    let translation = translations
        .find_by_translatable_and_language(translatable.id, language.id)
        .await?;

    let like_count = translations
        .unlike_translation(translation.id, requestor.id)
        .await?;
    let response = LikeTranslationResponse {
        liked: false,
        like_count: like_count.unwrap_or(translation.like_count),
    };
    Ok(Json(response))
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use serde_json::json;
    use std::sync::Arc;
    use tower::Service;

    use crate::controller::api::tests::{
        delete, get, make_authed_user, post_without_auth, put, put_without_auth,
    };
    use crate::email::MockEmailService;

    struct TestContext {
        token: String,
        app: axum::routing::RouterIntoService<axum::body::Body>,
        language: serde_json::Value,
        translatable: serde_json::Value,
    }

    async fn create_test_context() -> TestContext {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let username = crate::tests::random_name();
        let token = make_authed_user(&username, &app, email_service.clone()).await;

        let language = crate::controller::api::tests::create_test_language(&token, &mut app).await;
        let language_code = language["code"].as_str().unwrap();
        let translatable = crate::controller::api::tests::create_test_translatable(
            &token,
            &mut app,
            language_code,
        )
        .await;

        TestContext {
            token,
            app,
            language,
            translatable,
        }
    }

    #[tokio::test]
    async fn test_create_translation() {
        let mut ctx = create_test_context().await;
        let translatable_slug = ctx.translatable["slug"].as_str().unwrap();
        let language_code = ctx.language["code"].as_str().unwrap();

        let app = &mut ctx.app;

        let translation = crate::controller::api::tests::create_test_translation(
            &ctx.token,
            app,
            translatable_slug,
            language_code,
        )
        .await;
        assert_eq!(
            translation["translated_text"].as_str().unwrap(),
            "A test translation"
        );
    }

    #[tokio::test]
    async fn test_create_translation_unauthorized() {
        let mut ctx = create_test_context().await;
        let translatable_slug = ctx.translatable["slug"].as_str().unwrap();
        let language_code = ctx.language["code"].as_str().unwrap();

        let app = &mut ctx.app;

        let body = json!({
            "translated_text": "A test translation",
        });
        let request = post_without_auth(
            &format!("translatable/{translatable_slug}/translations/{language_code}"),
            body,
        )
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_get_translation() {
        let mut ctx = create_test_context().await;
        let translatable_slug = ctx.translatable["slug"].as_str().unwrap();
        let language_code = ctx.language["code"].as_str().unwrap();

        let app = &mut ctx.app;

        let _created = crate::controller::api::tests::create_test_translation(
            &ctx.token,
            app,
            translatable_slug,
            language_code,
        )
        .await;

        let request = get(&format!(
            "translatable/{translatable_slug}/translations/{language_code}"
        ))
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
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
        let mut ctx = create_test_context().await;
        let translatable_slug = ctx.translatable["slug"].as_str().unwrap();
        let language_code = ctx.language["code"].as_str().unwrap();

        let app = &mut ctx.app;

        let _translation = crate::controller::api::tests::create_test_translation(
            &ctx.token,
            app,
            translatable_slug,
            language_code,
        )
        .await;

        let request = get(&format!(
            "translatable/{translatable_slug}/translations?limit=10&offset=0"
        ))
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_list_translations_by_language() {
        let mut ctx = create_test_context().await;
        let translatable_slug = ctx.translatable["slug"].as_str().unwrap();
        let language_code = ctx.language["code"].as_str().unwrap();

        let app = &mut ctx.app;

        let _translation = crate::controller::api::tests::create_test_translation(
            &ctx.token,
            app,
            translatable_slug,
            language_code,
        )
        .await;

        let request = get(&format!(
            "languages/{language_code}/translations?limit=10&offset=0"
        ))
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_edit_translation() {
        let mut ctx = create_test_context().await;
        let translatable_slug = ctx.translatable["slug"].as_str().unwrap();
        let language_code = ctx.language["code"].as_str().unwrap();

        let app = &mut ctx.app;

        let _created = crate::controller::api::tests::create_test_translation(
            &ctx.token,
            app,
            translatable_slug,
            language_code,
        )
        .await;

        let update_body = json!({
            "translated_text": "Updated translation text.",
        });

        let request = put(
            &ctx.token,
            &format!("translatable/{translatable_slug}/translations/{language_code}"),
            &update_body,
        );
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let updated = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(updated["translated_text"], "Updated translation text.");
    }

    #[tokio::test]
    async fn test_edit_translation_unauthorized() {
        let mut ctx = create_test_context().await;
        let translatable_slug = ctx.translatable["slug"].as_str().unwrap();
        let language_code = ctx.language["code"].as_str().unwrap();

        let app = &mut ctx.app;

        let _created = crate::controller::api::tests::create_test_translation(
            &ctx.token,
            app,
            translatable_slug,
            language_code,
        )
        .await;

        let update_body = json!({
            "translated_text": "Updated translation text.",
        });

        let request = put_without_auth(
            &format!("translatable/{translatable_slug}/translations/{language_code}"),
            &update_body,
        );
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_delete_translation() {
        let mut ctx = create_test_context().await;
        let translatable_slug = ctx.translatable["slug"].as_str().unwrap();
        let language_code = ctx.language["code"].as_str().unwrap();

        let app = &mut ctx.app;

        let _created = crate::controller::api::tests::create_test_translation(
            &ctx.token,
            app,
            translatable_slug,
            language_code,
        )
        .await;

        let request = delete(
            &ctx.token,
            &format!("translatable/{translatable_slug}/translations/{language_code}"),
        );
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn test_delete_translation_unauthorized() {
        let mut ctx = create_test_context().await;
        let translatable_slug = ctx.translatable["slug"].as_str().unwrap();
        let language_code = ctx.language["code"].as_str().unwrap();

        let app = &mut ctx.app;

        let _created = crate::controller::api::tests::create_test_translation(
            &ctx.token,
            app,
            translatable_slug,
            language_code,
        )
        .await;

        let request = crate::controller::api::tests::delete_without_auth(&format!(
            "translatable/{translatable_slug}/translations/{language_code}"
        ));
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_like_unlike_translation() {
        let mut ctx = create_test_context().await;
        let translatable_slug = ctx.translatable["slug"].as_str().unwrap();
        let language_code = ctx.language["code"].as_str().unwrap();

        let app = &mut ctx.app;

        let _created = crate::controller::api::tests::create_test_translation(
            &ctx.token,
            app,
            translatable_slug,
            language_code,
        )
        .await;

        let request = crate::controller::api::tests::post(
            &ctx.token,
            &format!(
                "translatable/{}/translations/{}/like",
                translatable_slug, language_code
            ),
            json!({}),
        )
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(body["liked"], true);
        assert_eq!(body["like_count"], 1);

        let request = crate::controller::api::tests::post(
            &ctx.token,
            &format!(
                "translatable/{}/translations/{}/unlike",
                translatable_slug, language_code
            ),
            json!({}),
        )
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(body["liked"], false);
        assert_eq!(body["like_count"], 0);
    }
}
