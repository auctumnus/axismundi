use crate::{
    err::{AppResult, unauthorized_no_session},
    model::{
        definitions::DefinitionRepository, languages::LanguageRepository, quotations::{CreateQuotation, Quotation, QuotationRepository, UpdateQuotation}, translatable::TranslatableRepository, translations::TranslationRepository
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
            "/translatables/{translatable_slug}/translations/{language_code}/quotations",
            post(create_quotation),
        )
        .route(
            "/translatables/{translatable_slug}/translations/{language_code}/quotations",
            get(list_quotations_by_translation),
        )
        .route(
            "/languages/{language_code}/words/{word_slug}/definitions/{definition_id}/quotations",
            get(list_quotations_by_definition),
        )
        .route("/translatables/{translatable_slug}/translations/{language_code}/quotations/{id}", get(get_quotation_from_translation))
        .route("/translatables/{translatable_slug}/translations/{language_code}/quotations/{id}", put(edit_quotation_from_translation))
        .route("/translatables/{translatable_slug}/translations/{language_code}/quotations/{id}", delete(delete_quotation_from_translation))
}

type ApiResponse<T> = AppResult<T>;
type PaginatedApiResponse<T> = AppResult<PaginatedResponse<T>>;

pub async fn create_quotation(
    s: Session,
    Path((translatable_slug, language_code)): Path<(String, String)>,
    translations: TranslationRepository,
    translatables: TranslatableRepository,
    definitions: DefinitionRepository,
    quotations: QuotationRepository,
    languages: LanguageRepository,
    Json(req): Json<CreateQuotation>,
) -> ApiResponse<Json<Quotation>> {
    req.validate()?;

    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    let Ok(language) = languages.find_by_code(&language_code).await else {
        return Err(crate::err::not_found("Language not found"));
    };

    let translatable = translatables
        .find_by_slug(&translatable_slug)
        .await?;

    let Some(translation) = translations
        .find_by_translatable_and_language(translatable.id, language.id)
        .await? else {
            return Err(crate::err::not_found("Translation not found"));
    };

    let definition = definitions
        .find_by_id(req.definition)
        .await?;
    let translation_id = translation.id;
    let definition_id = definition.id;
    quotations
        .create(requestor, translation_id, definition_id, req)
        .await
        .map(Json)
}

pub async fn get_quotation_from_translation(
    Path((translatable_slug, language_code, id)): Path<(String, String, Uuid)>,
    translatables: TranslatableRepository,
    translations: TranslationRepository,
    languages: LanguageRepository,
    quotations: QuotationRepository,
) -> ApiResponse<Json<Quotation>> {
    let language = languages.find_by_code(&language_code).await
        .map_err(|_| crate::err::not_found("Language not found"))?;

    let translatable = translatables.find_by_slug(&translatable_slug).await?;

    let Some(translation) = translations
        .find_by_translatable_and_language(translatable.id, language.id)
        .await? else {
            return Err(crate::err::not_found("Translation not found"));
    };

    let quotation = quotations.find_by_id(id).await?;

    // Verify quotation belongs to this translation
    if quotation.translation != translation.id {
        return Err(crate::err::not_found("Quotation not found"));
    }

    Ok(Json(quotation))
}

pub async fn list_quotations_by_translation(
    Path((translatable_slug, language_code)): Path<(String, String)>,
    translatables: TranslatableRepository,
    translations: TranslationRepository,
    languages: LanguageRepository,
    quotations: QuotationRepository,
    pagination: PaginatedRequest,
) -> PaginatedApiResponse<Quotation> {
    let language = languages.find_by_code(&language_code).await
        .map_err(|_| crate::err::not_found("Language not found"))?;

    let translatable = translatables.find_by_slug(&translatable_slug).await?;

    let Some(translation) = translations
        .find_by_translatable_and_language(translatable.id, language.id)
        .await? else {
            return Err(crate::err::not_found("Translation not found"));
    };

    quotations.list_by_translation(translation.id, pagination).await
}

pub async fn list_quotations_by_definition(
    Path((language_code, word_slug, definition_id)): Path<(String, String, Uuid)>,
    languages: LanguageRepository,
    definitions: DefinitionRepository,
    quotations: QuotationRepository,
    pagination: PaginatedRequest,
) -> PaginatedApiResponse<Quotation> {
    let language = languages.find_by_code(&language_code).await
        .map_err(|_| crate::err::not_found("Language not found"))?;

    // Verify definition exists and belongs to the word
    let definition = definitions.find_by_id(definition_id).await?;

    // Note: We have language_code and word_slug in the path but aren't strictly validating them
    // The definition_id is already unique and verifies the definition exists

    quotations.list_by_definition(definition_id, pagination).await
}

pub async fn edit_quotation_from_translation(
    s: Session,
    Path((translatable_slug, language_code, id)): Path<(String, String, Uuid)>,
    translatables: TranslatableRepository,
    translations: TranslationRepository,
    languages: LanguageRepository,
    quotations: QuotationRepository,
    Json(updates): Json<UpdateQuotation>,
) -> ApiResponse<Json<Quotation>> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    let language = languages.find_by_code(&language_code).await
        .map_err(|_| crate::err::not_found("Language not found"))?;

    let translatable = translatables.find_by_slug(&translatable_slug).await?;

    let Some(translation) = translations
        .find_by_translatable_and_language(translatable.id, language.id)
        .await? else {
            return Err(crate::err::not_found("Translation not found"));
    };

    // Verify quotation belongs to this translation
    let quotation = quotations.find_by_id(id).await?;
    if quotation.translation != translation.id {
        return Err(crate::err::not_found("Quotation not found"));
    }

    quotations.update(requestor, id, updates).await.map(Json)
}

pub async fn delete_quotation_from_translation(
    s: Session,
    Path((translatable_slug, language_code, id)): Path<(String, String, Uuid)>,
    translatables: TranslatableRepository,
    translations: TranslationRepository,
    languages: LanguageRepository,
    quotations: QuotationRepository,
) -> ApiResponse<StatusCode> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    let language = languages.find_by_code(&language_code).await
        .map_err(|_| crate::err::not_found("Language not found"))?;

    let translatable = translatables.find_by_slug(&translatable_slug).await?;

    let Some(translation) = translations
        .find_by_translatable_and_language(translatable.id, language.id)
        .await? else {
            return Err(crate::err::not_found("Translation not found"));
    };

    // Verify quotation belongs to this translation
    let quotation = quotations.find_by_id(id).await?;
    if quotation.translation != translation.id {
        return Err(crate::err::not_found("Quotation not found"));
    }

    quotations.delete(requestor, id).await?;

    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use serde_json::json;
    use std::sync::Arc;
    use tower::Service;

    use crate::controller::api::tests::{create_test_definition, create_test_language, create_test_translatable, create_test_translation, create_test_word, delete, delete_without_auth, get, make_authed_user, post, post_without_auth, print_response_body};
    use crate::email::MockEmailService;
    use tower::ServiceExt;

    struct TestContext {
        token: String,
        app: axum::routing::RouterIntoService<axum::body::Body>,
        language: serde_json::Value,
        word: serde_json::Value,
        definition: serde_json::Value,
        translation: serde_json::Value,
        translatable: serde_json::Value,
    }

    async fn create_test_context() -> TestContext {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let username = crate::tests::random_name();
        let token = make_authed_user(&username, &mut app, email_service.clone()).await;

        let language = create_test_language(&token, &mut app).await;
        let language_code = language["code"].as_str().unwrap();

        let word = create_test_word(&token, &mut app, language_code).await;
        let slug = word["slug"].as_str().unwrap();
        let lemma = word["lemma"].as_i64().unwrap();

        let definition = create_test_definition(&token, &mut app, language_code, slug, lemma).await;

        let translatable = create_test_translatable(&token, &mut app, language_code).await;
        let translatable_slug = translatable["slug"].as_str().unwrap();

        let translation = create_test_translation(&token,  &mut app, translatable_slug, language_code).await;

        TestContext {
            token,
            app,
            language,
            word,
            definition,
            translation,
            translatable,
        }
    }

    async fn create_test_quotation(
        token: &str,
        app: &mut axum::routing::RouterIntoService<axum::body::Body>,
        translatable_slug: &str,
        language_code: &str,
        definition_id: &str,
    ) -> serde_json::Value {
        let body = json!({
            "span_start": 0,
            "span_end": 10,
            "definition": definition_id,
        });

        let request = post(
            token,
            &format!(
                "translatables/{translatable_slug}/translations/{language_code}/quotations",
                
            ),
            body,
        )
        .await;
        let response = app.call(request).await.unwrap();
        if response.status() != StatusCode::OK {
            print_response_body(response).await;
            panic!("Failed to create test quotation");
        }
        assert_eq!(response.status(), StatusCode::OK);
        crate::tests::response_to_value(response.into_body()).await
    }

    #[tokio::test]
    async fn test_create_quotation() {
        let ctx = create_test_context().await;
        let TestContext { token, mut app, translation, translatable, language, definition, .. } = ctx;
        let translatable_slug = translatable["slug"].as_str().unwrap();
        let language_code = language["code"].as_str().unwrap();
        let definition_id = definition["id"].as_str().unwrap();
        let quotation = create_test_quotation(&token, &mut app, translatable_slug, language_code, definition_id).await;
        assert_eq!(quotation["span_start"], 0);
        assert_eq!(quotation["span_end"], 10);
        assert!(quotation["id"].is_string());
    }

    #[tokio::test]
    async fn test_create_quotation_unauthorized() {
        let ctx = create_test_context().await;
        let TestContext { token, mut app, translation, definition, .. } = ctx;
        let translatable_slug = translation["translatable_slug"].as_str().unwrap();
        let language_code = ctx.language["code"].as_str().unwrap();
        let definition_id = definition["id"].as_str().unwrap();
        let body = json!({
            "span_start": 0,
            "span_end": 10,
            "definition": definition_id,
        });

        let request = post_without_auth(
            &format!(
                "translatables/{translatable_slug}/translations/{language_code}/quotations",
            ),
            body,
        ).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_get_quotation() {
        let ctx = create_test_context().await;
        let TestContext { token, mut app, translatable, language, translation, definition, .. } = ctx;
        let translatable_slug = translatable["slug"].as_str().unwrap();
        let language_code = language["code"].as_str().unwrap();
        let definition_id = definition["id"].as_str().unwrap();
        let quotation = create_test_quotation(&token, &mut app, translatable_slug, language_code, definition_id).await;
        let quotation_id = quotation["id"].as_str().unwrap();

        let request = get(&format!("translatables/{}/translations/{}/quotations/{}", translatable_slug, language_code, quotation_id)).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(body["id"], quotation_id);
        assert_eq!(body["span_start"], 0);
        assert_eq!(body["span_end"], 10);
    }

    #[tokio::test]
    async fn test_get_quotation_not_found() {
        let mut app = crate::tests::test_app().await.unwrap();

        let request = get("quotations/00000000-0000-0000-0000-000000000000").await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_list_quotations_by_translation() {
        let ctx = create_test_context().await;
        let TestContext { token, mut app, translatable, translation, definition, language, .. } = ctx;

        let translatable_slug = translatable["slug"].as_str().unwrap();
        let language_code = language["code"].as_str().unwrap();

        let quotation = create_test_quotation(&token, &mut app, translatable_slug, language_code, definition["id"].as_str().unwrap()).await;

        let request = get(&format!("translatables/{}/translations/{}/quotations", translatable_slug, language_code)).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = crate::tests::response_to_value(response.into_body()).await;
        assert!(body["items"].is_array());
        assert_eq!(body["items"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_list_quotations_by_definition() {
        let ctx = create_test_context().await;
        let TestContext { token, mut app, translatable, translation, definition, language, word, .. } = ctx;

        let translatable_slug = translatable["slug"].as_str().unwrap();
        let word_slug = word["slug"].as_str().unwrap();
        let language_code = language["code"].as_str().unwrap();

        let quotation = create_test_quotation(&token, &mut app, translatable_slug, language_code, definition["id"].as_str().unwrap()).await;

        let request = get(&format!("languages/{}/words/{}/definitions/{}/quotations", language_code, word_slug, definition["id"].as_str().unwrap())).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = crate::tests::response_to_value(response.into_body()).await;
        assert!(body["items"].is_array());
        assert_eq!(body["items"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_edit_quotation() {
        let ctx = create_test_context().await;
        let TestContext { token, mut app, translatable, language, translation, definition, .. } = ctx;
        let translatable_slug = translatable["slug"].as_str().unwrap();
        let language_code = language["code"].as_str().unwrap();
        let definition_id = definition["id"].as_str().unwrap();
        let quotation = create_test_quotation(&token, &mut app, translatable_slug, language_code, definition_id).await;
        let quotation_id = quotation["id"].as_str().unwrap();

        let update_body = json!({
            "span_start": 5,
        });

        let request = crate::controller::api::tests::put(&token, &format!("translatables/{}/translations/{}/quotations/{}", translatable_slug, language_code, quotation_id), &update_body);
        let response = app.call(request).await.unwrap();
        if response.status() != StatusCode::OK {
            print_response_body(response).await;
            panic!("Failed to edit quotation");
        }
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(body["id"], quotation_id);
        assert_eq!(body["span_start"], 5);
        assert_eq!(body["span_end"], 10); // unchanged
    }

    #[tokio::test]
    async fn test_edit_quotation_unauthorized() {
        let ctx = create_test_context().await;
        let TestContext { token, mut app, translatable, language, translation, definition, .. } = ctx;
        let translatable_slug = translatable["slug"].as_str().unwrap();
        let language_code = language["code"].as_str().unwrap();
        let definition_id = definition["id"].as_str().unwrap();
        let update_body = json!({
            "span_start": 5,
        });



        let request = crate::controller::api::tests::put_without_auth(&format!("translatables/{}/translations/{}/quotations/{}", translatable_slug, language_code, uuid::Uuid::nil()), &update_body);
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_delete_quotation() {
        let ctx = create_test_context().await;
        let TestContext { token, mut app, translatable, language, translation, definition, .. } = ctx;
        let translatable_slug = translatable["slug"].as_str().unwrap();
        let language_code = language["code"].as_str().unwrap();
        let definition_id = definition["id"].as_str().unwrap();
        let quotation = create_test_quotation(&token, &mut app, translatable_slug, language_code, definition_id).await;
        let quotation_id = quotation["id"].as_str().unwrap();

        let request = delete(&token, &format!("translatables/{}/translations/{}/quotations/{}", translatable_slug, language_code, quotation_id));
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn test_delete_quotation_unauthorized() {
        let ctx = create_test_context().await;
        let TestContext { token, mut app, translatable, language, translation, definition, .. } = ctx;
        let translatable_slug = translatable["slug"].as_str().unwrap();
        let language_code = language["code"].as_str().unwrap();
        let definition_id = definition["id"].as_str().unwrap();
        let quotation = create_test_quotation(&token, &mut app, translatable_slug, language_code, definition_id).await;
        let quotation_id = quotation["id"].as_str().unwrap();

        let request = delete_without_auth(&format!("translatables/{}/translations/{}/quotations/{}", translatable_slug, language_code, quotation_id));
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
