use crate::{
    err::{unauthorized_no_session, AppResult},
    model::{
        definitions::DefinitionRepository,
        languages::LanguageRepository,
        quotation_suggestions::{
            CreateQuotationSuggestion, QuotationSuggestion, QuotationSuggestionRepository,
            UpdateQuotationSuggestion,
        },
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
            "/languages/{code}/definitions/{definition_id}/quotation-suggestions",
            post(create_quotation_suggestion),
        )
        .route(
            "/languages/{code}/quotation-suggestions",
            get(list_quotation_suggestions_by_language),
        )
        .route(
            "/definitions/{definition_id}/quotation-suggestions",
            get(list_quotation_suggestions_by_definition),
        )
        .route("/quotation-suggestions/{id}", get(get_quotation_suggestion))
        .route("/quotation-suggestions/{id}", put(edit_quotation_suggestion))
        .route(
            "/quotation-suggestions/{id}",
            delete(delete_quotation_suggestion),
        )
}

type ApiResponse<T> = AppResult<T>;
type PaginatedApiResponse<T> = AppResult<PaginatedResponse<T>>;

pub async fn create_quotation_suggestion(
    s: Session,
    Path((code, definition_id)): Path<(String, Uuid)>,
    languages: LanguageRepository,
    definitions: DefinitionRepository,
    quotation_suggestions: QuotationSuggestionRepository,
    Json(req): Json<CreateQuotationSuggestion>,
) -> ApiResponse<Json<QuotationSuggestion>> {
    req.validate()?;

    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    let language = languages.find_by_code(&code).await?;

    // Verify definition exists
    definitions.find_by_id(definition_id).await?;

    quotation_suggestions
        .create(requestor, language.id, definition_id, req)
        .await
        .map(Json)
}

pub async fn get_quotation_suggestion(
    s: Session,
    quotation_suggestions: QuotationSuggestionRepository,
    Path(id): Path<Uuid>,
) -> ApiResponse<Json<QuotationSuggestion>> {
    let suggestion = quotation_suggestions
        .find_by_id_with_permission_check(s.user(), id)
        .await?;
    Ok(Json(suggestion))
}

pub async fn list_quotation_suggestions_by_language(
    s: Session,
    languages: LanguageRepository,
    quotation_suggestions: QuotationSuggestionRepository,
    Path(code): Path<String>,
    pagination: PaginatedRequest,
) -> PaginatedApiResponse<QuotationSuggestion> {
    let language = languages.find_by_code(&code).await?;

    quotation_suggestions
        .list_by_language(s.user(), language.id, pagination)
        .await
}

pub async fn list_quotation_suggestions_by_definition(
    s: Session,
    quotation_suggestions: QuotationSuggestionRepository,
    Path(definition_id): Path<Uuid>,
    pagination: PaginatedRequest,
) -> PaginatedApiResponse<QuotationSuggestion> {
    quotation_suggestions
        .list_by_definition(s.user(), definition_id, pagination)
        .await
}

pub async fn edit_quotation_suggestion(
    s: Session,
    quotation_suggestions: QuotationSuggestionRepository,
    Path(id): Path<Uuid>,
    Json(updates): Json<UpdateQuotationSuggestion>,
) -> ApiResponse<Json<QuotationSuggestion>> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    quotation_suggestions
        .update(requestor, id, updates)
        .await
        .map(Json)
}

pub async fn delete_quotation_suggestion(
    s: Session,
    quotation_suggestions: QuotationSuggestionRepository,
    Path(id): Path<Uuid>,
) -> ApiResponse<StatusCode> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    quotation_suggestions.delete(requestor, id).await?;

    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use serde_json::json;
    use std::sync::Arc;
    use tower::Service;

    use crate::controller::api::tests::{delete, get_with_auth, make_authed_user, post};
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
            "definition": "A test definition for quotation suggestions",
        });
        let request = post(token, &format!("words/{}/definitions", word_id), body).await;
        let response = ServiceExt::oneshot(app.clone(), request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        crate::tests::response_to_value(response.into_body()).await
    }

    #[tokio::test]
    async fn test_create_quotation_suggestion() {
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

        let body = json!({
            "text": "This is a suggested quotation text",
            "source": "Example Book",
        });

        let request = post(&token, &format!("languages/{}/definitions/{}/quotation-suggestions", language_code, definition_id), body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(body["text"], "This is a suggested quotation text");
        assert_eq!(body["source"], "Example Book");
        assert!(body["id"].is_string());
    }

    #[tokio::test]
    async fn test_create_quotation_suggestion_unauthorized() {
        let mut app = crate::tests::test_app().await.unwrap();

        let body = json!({
            "text": "Should fail",
            "source": "Test",
        });

        let request = crate::controller::api::tests::post_without_auth("languages/en/definitions/00000000-0000-0000-0000-000000000000/quotation-suggestions", body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_get_quotation_suggestion() {
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

        let body = json!({
            "text": "Specific suggestion text",
            "source": "Test Source",
        });

        let request = post(&token, &format!("languages/{}/definitions/{}/quotation-suggestions", language_code, definition_id), body).await;
        let response = app.call(request).await.unwrap();
        let created = crate::tests::response_to_value(response.into_body()).await;
        let suggestion_id = created["id"].as_str().unwrap();

        let request = get_with_auth(&token, &format!("quotation-suggestions/{}", suggestion_id)).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(body["id"], suggestion_id);
        assert_eq!(body["text"], "Specific suggestion text");
        assert_eq!(body["source"], "Test Source");
    }

    #[tokio::test]
    async fn test_get_quotation_suggestion_not_found() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let username = crate::tests::random_name();
        let token = make_authed_user(&username, &app, email_service.clone()).await;

        let request = get_with_auth(&token, "quotation-suggestions/00000000-0000-0000-0000-000000000000").await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_list_quotation_suggestions_by_language() {
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

        // Create a suggestion
        let body = json!({
            "text": "Suggestion 1",
            "source": "Source 1",
        });
        let request = post(&token, &format!("languages/{}/definitions/{}/quotation-suggestions", language_code, definition_id), body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let request = get_with_auth(&token, &format!("languages/{}/quotation-suggestions", language_code)).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert!(body["items"].is_array());
        assert!(body["items"].as_array().unwrap().len() >= 1);
    }

    #[tokio::test]
    async fn test_list_quotation_suggestions_by_definition() {
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

        // Create suggestions
        for i in 0..2 {
            let body = json!({
                "text": format!("Suggestion {}", i),
                "source": format!("Source {}", i),
            });
            let request = post(&token, &format!("languages/{}/definitions/{}/quotation-suggestions", language_code, definition_id), body).await;
            let response = app.call(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
        }

        let request = get_with_auth(&token, &format!("definitions/{}/quotation-suggestions", definition_id)).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert!(body["items"].is_array());
        assert_eq!(body["items"].as_array().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn test_edit_quotation_suggestion() {
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

        let body = json!({
            "text": "Original suggestion",
            "source": "Original source",
        });

        let request = post(&token, &format!("languages/{}/definitions/{}/quotation-suggestions", language_code, definition_id), body).await;
        let response = app.call(request).await.unwrap();
        let created = crate::tests::response_to_value(response.into_body()).await;
        let suggestion_id = created["id"].as_str().unwrap();

        let update_body = json!({
            "text": "Updated suggestion",
            "source": "Updated source",
        });

        let request = crate::controller::api::tests::put(&token, &format!("quotation-suggestions/{}", suggestion_id), &update_body);
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(body["text"], "Updated suggestion");
        assert_eq!(body["source"], "Updated source");
    }

    #[tokio::test]
    async fn test_edit_quotation_suggestion_unauthorized() {
        let mut app = crate::tests::test_app().await.unwrap();

        let update_body = json!({
            "text": "Should not work",
        });

        let request = crate::controller::api::tests::put_without_auth("quotation-suggestions/00000000-0000-0000-0000-000000000000", &update_body);
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_delete_quotation_suggestion() {
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

        let body = json!({
            "text": "To be deleted",
            "source": "Delete source",
        });

        let request = post(&token, &format!("languages/{}/definitions/{}/quotation-suggestions", language_code, definition_id), body).await;
        let response = app.call(request).await.unwrap();
        let created = crate::tests::response_to_value(response.into_body()).await;
        let suggestion_id = created["id"].as_str().unwrap();

        let request = delete(&token, &format!("quotation-suggestions/{}", suggestion_id));
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        // Verify it's deleted
        let request = get_with_auth(&token, &format!("quotation-suggestions/{}", suggestion_id)).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_delete_quotation_suggestion_unauthorized() {
        let mut app = crate::tests::test_app().await.unwrap();

        let request = crate::controller::api::tests::delete_without_auth("quotation-suggestions/00000000-0000-0000-0000-000000000000");
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
