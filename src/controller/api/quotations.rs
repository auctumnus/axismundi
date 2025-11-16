use crate::{
    err::{unauthorized_no_session, AppResult},
    model::{
        definitions::DefinitionRepository,
        quotations::{CreateQuotation, Quotation, QuotationRepository, UpdateQuotation},
        translations::TranslationRepository,
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
            "/translations/{translation_id}/definitions/{definition_id}/quotations",
            post(create_quotation),
        )
        .route(
            "/translations/{translation_id}/quotations",
            get(list_quotations_by_translation),
        )
        .route(
            "/definitions/{definition_id}/quotations",
            get(list_quotations_by_definition),
        )
        .route("/quotations/{id}", get(get_quotation))
        .route("/quotations/{id}", put(edit_quotation))
        .route("/quotations/{id}", delete(delete_quotation))
}

type ApiResponse<T> = AppResult<T>;
type PaginatedApiResponse<T> = AppResult<PaginatedResponse<T>>;

pub async fn create_quotation(
    s: Session,
    Path((translation_id, definition_id)): Path<(Uuid, Uuid)>,
    translations: TranslationRepository,
    definitions: DefinitionRepository,
    quotations: QuotationRepository,
    Json(req): Json<CreateQuotation>,
) -> ApiResponse<Json<Quotation>> {
    req.validate()?;

    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    // Verify translation and definition exist
    translations.find_by_id(translation_id).await?;
    definitions.find_by_id(definition_id).await?;

    quotations
        .create(requestor, translation_id, definition_id, req)
        .await
        .map(Json)
}

pub async fn get_quotation(
    quotations: QuotationRepository,
    Path(id): Path<Uuid>,
) -> ApiResponse<Json<Quotation>> {
    let quotation = quotations.find_by_id(id).await?;
    Ok(Json(quotation))
}

pub async fn list_quotations_by_translation(
    translations: TranslationRepository,
    quotations: QuotationRepository,
    Path(translation_id): Path<Uuid>,
    pagination: PaginatedRequest,
) -> PaginatedApiResponse<Quotation> {
    // Verify translation exists
    translations.find_by_id(translation_id).await?;

    quotations.list_by_translation(translation_id, pagination).await
}

pub async fn list_quotations_by_definition(
    definitions: DefinitionRepository,
    quotations: QuotationRepository,
    Path(definition_id): Path<Uuid>,
    pagination: PaginatedRequest,
) -> PaginatedApiResponse<Quotation> {
    // Verify definition exists
    definitions.find_by_id(definition_id).await?;

    quotations.list_by_definition(definition_id, pagination).await
}

pub async fn edit_quotation(
    s: Session,
    quotations: QuotationRepository,
    Path(id): Path<Uuid>,
    Json(updates): Json<UpdateQuotation>,
) -> ApiResponse<Json<Quotation>> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    quotations.update(requestor, id, updates).await.map(Json)
}

pub async fn delete_quotation(
    s: Session,
    quotations: QuotationRepository,
    Path(id): Path<Uuid>,
) -> ApiResponse<StatusCode> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    quotations.delete(requestor, id).await?;

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

    async fn create_test_word(token: &str, language_code: &str, app: &axum::routing::RouterIntoService<axum::body::Body>) -> serde_json::Value {
        let body = json!({
            "lemma": crate::tests::random_name(),
            "word_class": "noun",
        });
        let request = post(token, &format!("languages/{}/words", language_code), body).await;
        let response = ServiceExt::oneshot(app.clone(), request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        crate::tests::response_to_value(response.into_body()).await
    }

    async fn create_test_definition(token: &str, word_id: &str, app: &axum::routing::RouterIntoService<axum::body::Body>) -> serde_json::Value {
        let body = json!({
            "definition": "A test definition for quotations",
        });
        let request = post(token, &format!("words/{}/definitions", word_id), body).await;
        let response = ServiceExt::oneshot(app.clone(), request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        crate::tests::response_to_value(response.into_body()).await
    }

    async fn create_test_translatable(token: &str, app: &axum::routing::RouterIntoService<axum::body::Body>) -> serde_json::Value {
        let body = json!({
            "text": "This is a sample text to be translated and quoted.",
        });
        let request = post(token, "translatable", body).await;
        let response = ServiceExt::oneshot(app.clone(), request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        crate::tests::response_to_value(response.into_body()).await
    }

    async fn create_test_translation(token: &str, translatable_id: &str, language_code: &str, app: &axum::routing::RouterIntoService<axum::body::Body>) -> serde_json::Value {
        let body = json!({
            "text": "Sample translated text for quotation testing.",
        });
        let request = post(token, &format!("translatable/{}/languages/{}/translations", translatable_id, language_code), body).await;
        let response = ServiceExt::oneshot(app.clone(), request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        crate::tests::response_to_value(response.into_body()).await
    }

    #[tokio::test]
    async fn test_create_quotation() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let username = crate::tests::random_name();
        let token = make_authed_user(&username, &app, email_service.clone()).await;

        let language = create_test_language(&token, &app).await;
        let language_code = language["code"].as_str().unwrap();

        let word = create_test_word(&token, language_code, &app).await;
        let word_id = word["id"].as_str().unwrap();

        let definition = create_test_definition(&token, word_id, &app).await;
        let definition_id = definition["id"].as_str().unwrap();

        let translatable = create_test_translatable(&token, &app).await;
        let translatable_id = translatable["id"].as_str().unwrap();

        let translation = create_test_translation(&token, translatable_id, language_code, &app).await;
        let translation_id = translation["id"].as_str().unwrap();

        let body = json!({
            "span_start": 0,
            "span_end": 10,
        });

        let request = post(&token, &format!("translations/{}/definitions/{}/quotations", translation_id, definition_id), body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(body["span_start"], 0);
        assert_eq!(body["span_end"], 10);
        assert!(body["id"].is_string());
    }

    #[tokio::test]
    async fn test_create_quotation_unauthorized() {
        let mut app = crate::tests::test_app().await.unwrap();

        let body = json!({
            "span_start": 0,
            "span_end": 10,
        });

        let request = crate::controller::api::tests::post_without_auth("translations/00000000-0000-0000-0000-000000000000/definitions/00000000-0000-0000-0000-000000000000/quotations", body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_get_quotation() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let username = crate::tests::random_name();
        let token = make_authed_user(&username, &app, email_service.clone()).await;

        let language = create_test_language(&token, &app).await;
        let language_code = language["code"].as_str().unwrap();

        let word = create_test_word(&token, language_code, &app).await;
        let word_id = word["id"].as_str().unwrap();

        let definition = create_test_definition(&token, word_id, &app).await;
        let definition_id = definition["id"].as_str().unwrap();

        let translatable = create_test_translatable(&token, &app).await;
        let translatable_id = translatable["id"].as_str().unwrap();

        let translation = create_test_translation(&token, translatable_id, language_code, &app).await;
        let translation_id = translation["id"].as_str().unwrap();

        let body = json!({
            "span_start": 5,
            "span_end": 15,
        });

        let request = post(&token, &format!("translations/{}/definitions/{}/quotations", translation_id, definition_id), body).await;
        let response = app.call(request).await.unwrap();
        let created = crate::tests::response_to_value(response.into_body()).await;
        let quotation_id = created["id"].as_str().unwrap();

        let request = get(&format!("quotations/{}", quotation_id)).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(body["id"], quotation_id);
        assert_eq!(body["span_start"], 5);
        assert_eq!(body["span_end"], 15);
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
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let username = crate::tests::random_name();
        let token = make_authed_user(&username, &app, email_service.clone()).await;

        let language = create_test_language(&token, &app).await;
        let language_code = language["code"].as_str().unwrap();

        let word = create_test_word(&token, language_code, &app).await;
        let word_id = word["id"].as_str().unwrap();

        let definition = create_test_definition(&token, word_id, &app).await;
        let definition_id = definition["id"].as_str().unwrap();

        let translatable = create_test_translatable(&token, &app).await;
        let translatable_id = translatable["id"].as_str().unwrap();

        let translation = create_test_translation(&token, translatable_id, language_code, &app).await;
        let translation_id = translation["id"].as_str().unwrap();

        // Create multiple quotations
        for i in 0..3 {
            let body = json!({
                "span_start": i * 10,
                "span_end": i * 10 + 5,
            });
            let request = post(&token, &format!("translations/{}/definitions/{}/quotations", translation_id, definition_id), body).await;
            let response = app.call(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
        }

        let request = get(&format!("translations/{}/quotations", translation_id)).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert!(body["items"].is_array());
        assert_eq!(body["items"].as_array().unwrap().len(), 3);
    }

    #[tokio::test]
    async fn test_list_quotations_by_definition() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let username = crate::tests::random_name();
        let token = make_authed_user(&username, &app, email_service.clone()).await;

        let language = create_test_language(&token, &app).await;
        let language_code = language["code"].as_str().unwrap();

        let word = create_test_word(&token, language_code, &app).await;
        let word_id = word["id"].as_str().unwrap();

        let definition = create_test_definition(&token, word_id, &app).await;
        let definition_id = definition["id"].as_str().unwrap();

        let translatable = create_test_translatable(&token, &app).await;
        let translatable_id = translatable["id"].as_str().unwrap();

        let translation = create_test_translation(&token, translatable_id, language_code, &app).await;
        let translation_id = translation["id"].as_str().unwrap();

        // Create a quotation
        let body = json!({
            "span_start": 0,
            "span_end": 10,
        });
        let request = post(&token, &format!("translations/{}/definitions/{}/quotations", translation_id, definition_id), body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let request = get(&format!("definitions/{}/quotations", definition_id)).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert!(body["items"].is_array());
        assert_eq!(body["items"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_edit_quotation() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let username = crate::tests::random_name();
        let token = make_authed_user(&username, &app, email_service.clone()).await;

        let language = create_test_language(&token, &app).await;
        let language_code = language["code"].as_str().unwrap();

        let word = create_test_word(&token, language_code, &app).await;
        let word_id = word["id"].as_str().unwrap();

        let definition = create_test_definition(&token, word_id, &app).await;
        let definition_id = definition["id"].as_str().unwrap();

        let translatable = create_test_translatable(&token, &app).await;
        let translatable_id = translatable["id"].as_str().unwrap();

        let translation = create_test_translation(&token, translatable_id, language_code, &app).await;
        let translation_id = translation["id"].as_str().unwrap();

        let body = json!({
            "span_start": 0,
            "span_end": 10,
        });

        let request = post(&token, &format!("translations/{}/definitions/{}/quotations", translation_id, definition_id), body).await;
        let response = app.call(request).await.unwrap();
        let created = crate::tests::response_to_value(response.into_body()).await;
        let quotation_id = created["id"].as_str().unwrap();

        let update_body = json!({
            "span_start": 30,
            "span_end": 40,
        });

        let request = crate::controller::api::tests::put(&token, &format!("quotations/{}", quotation_id), &update_body);
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(body["span_start"], 30);
        assert_eq!(body["span_end"], 40);
    }

    #[tokio::test]
    async fn test_edit_quotation_unauthorized() {
        let mut app = crate::tests::test_app().await.unwrap();

        let update_body = json!({
            "span_start": 5,
        });

        let request = crate::controller::api::tests::put_without_auth("quotations/00000000-0000-0000-0000-000000000000", &update_body);
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_delete_quotation() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let username = crate::tests::random_name();
        let token = make_authed_user(&username, &app, email_service.clone()).await;

        let language = create_test_language(&token, &app).await;
        let language_code = language["code"].as_str().unwrap();

        let word = create_test_word(&token, language_code, &app).await;
        let word_id = word["id"].as_str().unwrap();

        let definition = create_test_definition(&token, word_id, &app).await;
        let definition_id = definition["id"].as_str().unwrap();

        let translatable = create_test_translatable(&token, &app).await;
        let translatable_id = translatable["id"].as_str().unwrap();

        let translation = create_test_translation(&token, translatable_id, language_code, &app).await;
        let translation_id = translation["id"].as_str().unwrap();

        let body = json!({
            "span_start": 0,
            "span_end": 10,
        });

        let request = post(&token, &format!("translations/{}/definitions/{}/quotations", translation_id, definition_id), body).await;
        let response = app.call(request).await.unwrap();
        let created = crate::tests::response_to_value(response.into_body()).await;
        let quotation_id = created["id"].as_str().unwrap();

        let request = delete(&token, &format!("quotations/{}", quotation_id));
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        // Verify it's deleted
        let request = get(&format!("quotations/{}", quotation_id)).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_delete_quotation_unauthorized() {
        let mut app = crate::tests::test_app().await.unwrap();

        let request = crate::controller::api::tests::delete_without_auth("quotations/00000000-0000-0000-0000-000000000000");
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
