use crate::{
    err::{AppResult, unauthorized_no_session},
    model::{
        definitions::{CreateDefinition, Definition, DefinitionRepository, UpdateDefinition},
        languages::LanguageRepository,
        words::WordRepository,
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
use uuid::Uuid;
use validator::Validate;

pub fn create_router() -> axum::Router<crate::util::AppState> {
    axum::Router::new()
        .route(
            "/languages/{code}/words/{slug}/{lemma}/definitions",
            post(create_definition),
        )
        .route(
            "/languages/{code}/words/{slug}/{lemma}/definitions",
            get(list_definitions),
        )
        .route(
            "/languages/{code}/words/{slug}/{lemma}/definitions/{id}",
            get(get_definition),
        )
        .route(
            "/languages/{code}/words/{slug}/{lemma}/definitions/{id}",
            put(edit_definition),
        )
        .route(
            "/languages/{code}/words/{slug}/{lemma}/definitions/{id}",
            delete(delete_definition),
        )
}

type ApiResponse<T> = AppResult<T>;
type PaginatedApiResponse<T> = AppResult<PaginatedResponse<T>>;

pub async fn create_definition(
    s: Session,
    Path((code, slug, lemma)): Path<(String, String, i32)>,
    languages: LanguageRepository,
    words: WordRepository,
    definitions: DefinitionRepository,
    Json(req): Json<CreateDefinition>,
) -> ApiResponse<Json<Definition>> {
    req.validate()?;

    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    // Look up language and word
    let language = languages.find_by_code(&code).await?;
    let word = words
        .find_by_slug_and_lemma(Some(requestor), language.id, &slug, lemma)
        .await?;

    definitions.create(requestor, word.id, req).await.map(Json)
}

pub async fn get_definition(
    Path((_code, _slug, _lemma, id)): Path<(String, String, i32, Uuid)>,
    definitions: DefinitionRepository,
) -> ApiResponse<Json<Definition>> {
    let definition = definitions.find_by_id(id).await?;
    Ok(Json(definition))
}

pub async fn list_definitions(
    Path((code, slug, lemma)): Path<(String, String, i32)>,
    languages: LanguageRepository,
    words: WordRepository,
    definitions: DefinitionRepository,
    pagination: PaginatedRequest,
) -> PaginatedApiResponse<Definition> {
    // Look up language and word
    let language = languages.find_by_code(&code).await?;
    let word = words
        .find_by_slug_and_lemma(None, language.id, &slug, lemma)
        .await?;

    definitions.list_by_word(word.id, pagination).await
}

pub async fn edit_definition(
    s: Session,
    Path((_code, _slug, _lemma, id)): Path<(String, String, i32, Uuid)>,
    definitions: DefinitionRepository,
    Json(updates): Json<UpdateDefinition>,
) -> ApiResponse<Json<Definition>> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    definitions.update(requestor, id, updates).await.map(Json)
}

pub async fn delete_definition(
    s: Session,
    Path((code, slug, lemma, _id)): Path<(String, String, i32, Uuid)>,
    languages: LanguageRepository,
    words: WordRepository,
) -> ApiResponse<StatusCode> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    let language = languages.find_by_code(&code).await?;
    words
        .delete_by_lemma(requestor, language.id, &slug, lemma)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use serde_json::json;
    use std::sync::Arc;
    use tower::Service;

    use crate::controller::api::tests::{
        create_test_language, create_test_word, delete, delete_without_auth, get, make_authed_user,
        post, post_without_auth,
    };
    use crate::email::MockEmailService;

    struct TestContext {
        token: String,
        app: axum::routing::RouterIntoService<axum::body::Body>,
        language: serde_json::Value,
        word: serde_json::Value,
    }

    async fn create_test_context() -> TestContext {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let username = crate::tests::random_name();
        let token = make_authed_user(&username, &app, email_service.clone()).await;

        // Create language
        let language = create_test_language(&token, &mut app).await;
        let language_code = language["code"].as_str().unwrap();

        // Create word
        let word = create_test_word(&token, &mut app, language_code).await;

        TestContext {
            token,
            app,
            language,
            word,
        }
    }

    #[tokio::test]
    async fn test_create_definition() {
        let ctx = create_test_context().await;

        let TestContext {
            token,
            mut app,
            language,
            word,
        } = ctx;

        let body = json!({
            "definition": "A test definition",
            "context": "Used in testing scenarios",
        });

        let code = language["code"].as_str().unwrap();
        let slug = word.get("slug").unwrap().as_str().unwrap();
        let lemma = word.get("lemma").unwrap().as_i64().unwrap();

        let request = post(
            &token,
            &format!("languages/{}/words/{}/{}/definitions", code, slug, lemma),
            body,
        )
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(body["definition"], "A test definition");
        assert_eq!(body["context"], "Used in testing scenarios");
        assert!(body["id"].is_string());
    }

    #[tokio::test]
    async fn test_create_definition_unauthorized() {
        let ctx = create_test_context().await;

        let TestContext {
            mut app,
            language,
            word,
            ..
        } = ctx;

        let body = json!({
            "definition": "A test definition",
            "context": "Used in testing scenarios",
        });

        let code = language["code"].as_str().unwrap();
        let slug = word.get("slug").unwrap().as_str().unwrap();
        let lemma = word.get("lemma").unwrap().as_i64().unwrap();

        let request = post_without_auth(
            &format!("languages/{}/words/{}/{}/definitions", code, slug, lemma),
            body,
        )
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_get_definition() {
        let ctx = create_test_context().await;

        let TestContext {
            token,
            mut app,
            language,
            word,
        } = ctx;

        let body = json!({
            "definition": "A test definition",
            "context": "Used in testing scenarios",
        });

        let code = language["code"].as_str().unwrap();
        let slug = word.get("slug").unwrap().as_str().unwrap();
        let lemma = word.get("lemma").unwrap().as_i64().unwrap();

        let request = post(
            &token,
            &format!("languages/{}/words/{}/{}/definitions", code, slug, lemma),
            body,
        )
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(body["definition"], "A test definition");
        assert_eq!(body["context"], "Used in testing scenarios");
        assert!(body["id"].is_string());

        let definition_id = body["id"].as_str().unwrap();
        let request = get(&format!(
            "languages/{}/words/{}/{}/definitions/{}",
            code, slug, lemma, definition_id
        ))
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(body["definition"], "A test definition");
        assert_eq!(body["context"], "Used in testing scenarios");
    }

    #[tokio::test]
    async fn test_get_definition_not_found() {
        let ctx = create_test_context().await;

        let TestContext {
            mut app,
            language,
            word,
            ..
        } = ctx;
        let code = language["code"].as_str().unwrap();
        let slug = word.get("slug").unwrap().as_str().unwrap();
        let lemma = word.get("lemma").unwrap().as_i64().unwrap();

        let request = get(&format!(
            "languages/{}/words/{}/{}/definitions/00000000-0000-0000-0000-000000000000",
            code, slug, lemma
        ))
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_list_definitions() {
        let ctx = create_test_context().await;

        let TestContext {
            token,
            mut app,
            language,
            word,
        } = ctx;

        let code = language["code"].as_str().unwrap();
        let slug = word.get("slug").unwrap().as_str().unwrap();
        let lemma = word.get("lemma").unwrap().as_i64().unwrap();

        // Create multiple definitions
        for i in 0..3 {
            let body = json!({
                "definition": format!("A test definition {}", i),
                "context": "Used in testing scenarios",
            });

            let request = post(
                &token,
                &format!("languages/{}/words/{}/{}/definitions", code, slug, lemma),
                body,
            )
            .await;
            let response = app.call(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);

            let body = crate::tests::response_to_value(response.into_body()).await;
            assert_eq!(body["definition"], format!("A test definition {}", i));
            assert_eq!(body["context"], "Used in testing scenarios");
            assert!(body["id"].is_string());
        }

        let request = get(&format!(
            "languages/{}/words/{}/{}/definitions",
            code, slug, lemma
        ))
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = crate::tests::response_to_value(response.into_body()).await;
        assert!(body.get("items").is_some());
        let items = body.get("items").unwrap().as_array().unwrap();
        assert!(items.len() == 3);
    }

    #[tokio::test]
    async fn test_edit_definition() {
        let ctx = create_test_context().await;

        let TestContext {
            token,
            mut app,
            language,
            word,
        } = ctx;

        let body = json!({
            "definition": "A test definition",
            "context": "Used in testing scenarios",
        });

        let code = language["code"].as_str().unwrap();
        let slug = word.get("slug").unwrap().as_str().unwrap();
        let lemma = word.get("lemma").unwrap().as_i64().unwrap();

        let request = post(
            &token,
            &format!("languages/{}/words/{}/{}/definitions", code, slug, lemma),
            body,
        )
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;

        let definition_id = body["id"].as_str().unwrap();

        let update_body = json!({
            "definition": "Updated definition",
            "context": "New context",
        });

        let request = crate::controller::api::tests::put(
            &token,
            &format!(
                "languages/{}/words/{}/{}/definitions/{}",
                code, slug, lemma, definition_id
            ),
            &update_body,
        );
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(body["definition"], "Updated definition");
        assert_eq!(body["context"], "New context");
    }

    #[tokio::test]
    async fn test_edit_definition_unauthorized() {
        let ctx = create_test_context().await;

        let TestContext {
            token,
            mut app,
            language,
            word,
        } = ctx;

        let body = json!({
            "definition": "A test definition",
            "context": "Used in testing scenarios",
        });

        let code = language["code"].as_str().unwrap();
        let slug = word.get("slug").unwrap().as_str().unwrap();
        let lemma = word.get("lemma").unwrap().as_i64().unwrap();

        let request = post(
            &token,
            &format!("languages/{}/words/{}/{}/definitions", code, slug, lemma),
            body,
        )
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;

        let definition_id = body["id"].as_str().unwrap();

        let update_body = json!({
            "definition": "Updated definition",
            "context": "New context",
        });

        let request = crate::controller::api::tests::put_without_auth(
            &format!(
                "languages/{}/words/{}/{}/definitions/{}",
                code, slug, lemma, definition_id
            ),
            &update_body,
        );
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_delete_definition() {
        let ctx = create_test_context().await;
        let TestContext {
            token,
            mut app,
            language,
            word,
        } = ctx;

        let body = json!({
            "definition": "A test definition",
            "context": "Used in testing scenarios",
        });

        let code = language["code"].as_str().unwrap();
        let slug = word.get("slug").unwrap().as_str().unwrap();
        let lemma = word.get("lemma").unwrap().as_i64().unwrap();

        let request = post(
            &token,
            &format!("languages/{}/words/{}/{}/definitions", code, slug, lemma),
            body,
        )
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;

        let definition_id = body["id"].as_str().unwrap();

        let request = delete(
            &token,
            &format!(
                "languages/{}/words/{}/{}/definitions/{}",
                code, slug, lemma, definition_id
            ),
        );
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn test_delete_definition_unauthorized() {
        let ctx = create_test_context().await;
        let TestContext {
            token,
            mut app,
            language,
            word,
        } = ctx;

        let body = json!({
            "definition": "A test definition",
            "context": "Used in testing scenarios",
        });

        let code = language["code"].as_str().unwrap();
        let slug = word.get("slug").unwrap().as_str().unwrap();
        let lemma = word.get("lemma").unwrap().as_i64().unwrap();

        let request = post(
            &token,
            &format!("languages/{}/words/{}/{}/definitions", code, slug, lemma),
            body,
        )
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;

        let definition_id = body["id"].as_str().unwrap();

        let request = delete_without_auth(&format!(
            "languages/{}/words/{}/{}/definitions/{}",
            code, slug, lemma, definition_id
        ));
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
