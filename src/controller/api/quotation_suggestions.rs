use crate::{
    err::{AppResult, unauthorized_no_session},
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
    Json,
    extract::{Path, Query},
    http::StatusCode,
    routing::{delete, get, post, put},
};
use uuid::Uuid;
use validator::Validate;

pub fn create_router() -> axum::Router<crate::util::AppState> {
    axum::Router::new()
        .route(
            "/languages/{code}/quotation-suggestions",
            post(create_quotation_suggestion),
        )
        .route(
            "/languages/{code}/quotation-suggestions",
            get(list_quotation_suggestions_by_language),
        )
        .route(
            "/languages/{code}/quotation-suggestions/{id}",
            delete(delete_quotation_suggestion),
        )
}

type ApiResponse<T> = AppResult<T>;
type PaginatedApiResponse<T> = AppResult<PaginatedResponse<T>>;

pub async fn create_quotation_suggestion(
    s: Session,
    Path(code): Path<String>,
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

    quotation_suggestions
        .create(requestor, language.id, req)
        .await
        .map(Json)
}

#[derive(serde::Deserialize)]
pub struct ListQuotationSuggestionsQuery {
    content: String,
}

pub async fn list_quotation_suggestions_by_language(
    s: Session,
    languages: LanguageRepository,
    quotation_suggestions: QuotationSuggestionRepository,
    Path(code): Path<String>,
    Query(ListQuotationSuggestionsQuery { content }): axum::extract::Query<
        ListQuotationSuggestionsQuery,
    >,
    pagination: PaginatedRequest,
) -> PaginatedApiResponse<QuotationSuggestion> {
    let language = languages.find_by_code(&code).await?;

    quotation_suggestions
        .list_by_language(s.user(), language.id, pagination, content)
        .await
}

pub async fn delete_quotation_suggestion(
    s: Session,
    quotation_suggestions: QuotationSuggestionRepository,
    Path((code, id)): Path<(String, Uuid)>,
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

    use crate::controller::api::tests::{
        create_test_definition, create_test_language, create_test_translatable,
        create_test_translation, create_test_word, delete, delete_without_auth, get_with_auth,
        make_authed_user, post, print_response_body,
    };
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

        let translation =
            create_test_translation(&token, &mut app, translatable_slug, language_code).await;

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

    async fn create_test_quotation_suggestion(ctx: &mut TestContext) -> serde_json::Value {
        let language_code = ctx.language["code"].as_str().unwrap();
        let definition_id = ctx.definition["id"].as_str().unwrap();
        let body = json!({
            "span_content": "test",
            "definition": definition_id,
        });

        let request = post(
            &ctx.token,
            &format!("languages/{}/quotation-suggestions", language_code),
            body,
        )
        .await;
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let created = crate::tests::response_to_value(response.into_body()).await;
        created
    }

    #[tokio::test]
    async fn test_create_quotation_suggestion() {
        let mut ctx = create_test_context().await;

        let suggestion = create_test_quotation_suggestion(&mut ctx).await;
        let definition_id = ctx.definition["id"].as_str().unwrap();
        assert_eq!(suggestion["span_content"], "test");
    }

    #[tokio::test]
    async fn test_create_quotation_suggestion_unauthorized() {
        let mut ctx = create_test_context().await;
        let language_code = ctx.language["code"].as_str().unwrap();
        let definition_id = ctx.definition["id"].as_str().unwrap();
        let body = json!({
            "span_content": "test",
            "definition": definition_id,
        });
        let request = crate::controller::api::tests::post_without_auth(
            &format!("languages/{}/quotation-suggestions", language_code),
            body,
        )
        .await;
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_list_quotation_suggestions_by_language() {
        let mut ctx = create_test_context().await;

        let suggestion = create_test_quotation_suggestion(&mut ctx).await;
        let language_code = ctx.language["code"].as_str().unwrap();

        let request = get_with_auth(
            &ctx.token,
            &format!(
                "languages/{}/quotation-suggestions?content=test",
                language_code
            ),
        )
        .await;
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let listed = crate::tests::response_to_value(response.into_body()).await;
        let items = listed["items"].as_array().unwrap();
        assert!(items.iter().any(|item| item["id"] == suggestion["id"]));
    }

    #[tokio::test]
    async fn test_delete_quotation_suggestion() {
        let mut ctx = create_test_context().await;

        let suggestion = create_test_quotation_suggestion(&mut ctx).await;
        let language_code = ctx.language["code"].as_str().unwrap();
        let suggestion_id = suggestion["id"].as_str().unwrap();

        let request = delete(
            &ctx.token,
            &format!(
                "languages/{}/quotation-suggestions/{}",
                language_code, suggestion_id
            ),
        );
        let response = ctx.app.call(request).await.unwrap();
        if response.status() != StatusCode::NO_CONTENT {
            print_response_body(response).await;
            panic!("Expected NO_CONTENT status");
        }
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn test_delete_quotation_suggestion_unauthorized() {
        let mut ctx = create_test_context().await;
        let suggestion = create_test_quotation_suggestion(&mut ctx).await;
        let language_code = ctx.language["code"].as_str().unwrap();
        let suggestion_id = suggestion["id"].as_str().unwrap();

        let request = delete_without_auth(&format!(
            "languages/{}/quotation-suggestions/{}",
            language_code, suggestion_id
        ));
        let response = ctx.app.call(request).await.unwrap();
        if response.status() != StatusCode::UNAUTHORIZED {
            print_response_body(response).await;
            panic!("Expected UNAUTHORIZED status");
        }
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
