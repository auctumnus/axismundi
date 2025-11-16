use crate::{
    err::{unauthorized_no_session, AppResult},
    model::{
        definitions::{CreateDefinition, Definition, DefinitionRepository, UpdateDefinition},
        languages::LanguageRepository,
        words::WordRepository,
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
        .route("/languages/{code}/words/{slug}/{lemma}/definitions", post(create_definition))
        .route("/languages/{code}/words/{slug}/{lemma}/definitions", get(list_definitions))
        .route("/languages/{code}/words/{slug}/{lemma}/definitions/{id}", get(get_definition))
        .route("/languages/{code}/words/{slug}/{lemma}/definitions/{id}", put(edit_definition))
        .route("/languages/{code}/words/{slug}/{lemma}/definitions/{id}", delete(delete_definition))
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
    let word = words.find_by_slug_and_lemma(Some(requestor), language.id, &slug, lemma).await?;

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
    let word = words.find_by_slug_and_lemma(None, language.id, &slug, lemma).await?;

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
    Path((_code, _slug, _lemma, id)): Path<(String, String, i32, Uuid)>,
    definitions: DefinitionRepository,
) -> ApiResponse<StatusCode> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    definitions.delete(requestor, id).await?;

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
        let language = crate::tests::response_to_value(response.into_body()).await;

        // add noun word class
        let body = json!({
            "name": "noun",
            "abbreviation": "n",
        });
        let request = crate::controller::api::tests::post(token, &format!("languages/{code}/word-classes"), body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        language
    }

    async fn create_test_word(token: &str, app: &mut axum::routing::RouterIntoService<axum::body::Body>, language_code: &str) -> serde_json::Value {
        let body = json!({
            "word": crate::tests::random_name(),
            "word_class": "n",
        });
        let request = post(token, &format!("languages/{language_code}/words"), body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let word = crate::tests::response_to_value(response.into_body()).await;

        word
    }


    #[tokio::test]
    async fn test_create_definition() {
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

        let body = json!({
            "definition": "A test definition",
            "context": "Used in testing scenarios",
        });

        let request = post(&token, &format!("languages/{}/words/{}/{}/definitions", language_code, word.get("slug").unwrap().as_str().unwrap(), word.get("lemma").unwrap().as_i64().unwrap()), body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(body["definition"], "A test definition");
        assert_eq!(body["context"], "Used in testing scenarios");
        assert!(body["id"].is_string());
    }

    #[tokio::test]
    async fn test_create_definition_unauthorized() {
        let mut app = crate::tests::test_app().await.unwrap();

        let body = json!({
            "definition": "A test definition",
        });

        let request = crate::controller::api::tests::post_without_auth("words/00000000-0000-0000-0000-000000000000/definitions", body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_get_definition() {
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

        let body = json!({
            "definition": "A specific test definition",
        });

        let request = post(&token, &format!("languages/{}/words/{}/{}/definitions", language_code, word.get("slug").unwrap().as_str().unwrap(), word.get("lemma").unwrap().as_i64().unwrap()), body).await;
        let response = app.call(request).await.unwrap();
        let created = crate::tests::response_to_value(response.into_body()).await;
        let definition_id = created["id"].as_str().unwrap();

        let request = get(&format!("languages/{}/words/{}/{}/definitions/{}", language_code, word.get("slug").unwrap().as_str().unwrap(), word.get("lemma").unwrap().as_i64().unwrap(), definition_id)).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(body["id"], definition_id);
        assert_eq!(body["definition"], "A specific test definition");
    }

    #[tokio::test]
    async fn test_get_definition_not_found() {
        let mut app = crate::tests::test_app().await.unwrap();

        let request = get("definitions/00000000-0000-0000-0000-000000000000").await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_list_definitions() {
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

        // Create multiple definitions
        for i in 0..3 {
            let body = json!({
                "definition": format!("Test definition {}", i),
            });
            let request = post(&token, &format!("languages/{}/words/{}/{}/definitions", language_code, word.get("slug").unwrap().as_str().unwrap(), word.get("lemma").unwrap().as_i64().unwrap()), body).await;
            let response = app.call(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
        }

        let request = get(&format!("languages/{}/words/{}/{}/definitions", language_code, word.get("slug").unwrap().as_str().unwrap(), word.get("lemma").unwrap().as_i64().unwrap())).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert!(body["items"].is_array());
        assert_eq!(body["items"].as_array().unwrap().len(), 3);
    }

    #[tokio::test]
    async fn test_edit_definition() {
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

        let body = json!({
            "definition": "Original definition",
        });

        let request = post(&token, &format!("languages/{}/words/{}/{}/definitions", language_code, word.get("slug").unwrap().as_str().unwrap(), word.get("lemma").unwrap().as_i64().unwrap()), body).await;
        let response = app.call(request).await.unwrap();
        let created = crate::tests::response_to_value(response.into_body()).await;
        let definition_id = created["id"].as_str().unwrap();

        let update_body = json!({
            "definition": "Updated definition",
            "context": "New context",
        });

        let request = crate::controller::api::tests::put(&token, &format!("languages/{}/words/{}/{}/definitions/{}", language_code, word.get("slug").unwrap().as_str().unwrap(), word.get("lemma").unwrap().as_i64().unwrap(), definition_id), &update_body);
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(body["definition"], "Updated definition");
        assert_eq!(body["context"], "New context");
    }

    #[tokio::test]
    async fn test_edit_definition_unauthorized() {
        let mut app = crate::tests::test_app().await.unwrap();

        let update_body = json!({
            "definition": "Should not work",
        });

        let request = crate::controller::api::tests::put_without_auth("definitions/00000000-0000-0000-0000-000000000000", &update_body);
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_delete_definition() {
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

        let body = json!({
            "definition": "To be deleted",
        });

        let request = post(&token, &format!("languages/{}/words/{}/{}/definitions", language_code, word.get("slug").unwrap().as_str().unwrap(), word.get("lemma").unwrap().as_i64().unwrap()), body).await;
        let response = app.call(request).await.unwrap();
        let created = crate::tests::response_to_value(response.into_body()).await;
        let definition_id = created["id"].as_str().unwrap();

        let request = delete(&token, &format!("languages/{}/words/{}/{}/definitions/{}", language_code, word.get("slug").unwrap().as_str().unwrap(), word.get("lemma").unwrap().as_i64().unwrap(), definition_id));
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        // Verify it's deleted
        let request = get(&format!("languages/{}/words/{}/{}/definitions/{}", language_code, word.get("slug").unwrap().as_str().unwrap(), word.get("lemma").unwrap().as_i64().unwrap(), definition_id)).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_delete_definition_unauthorized() {
        let mut app = crate::tests::test_app().await.unwrap();

        let request = crate::controller::api::tests::delete_without_auth("definitions/00000000-0000-0000-0000-000000000000");
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
