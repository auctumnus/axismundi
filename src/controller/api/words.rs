use crate::{
    err::{AppResult, unauthorized_no_session},
    model::{
        languages::LanguageRepository,
        words::{CreateWord, UpdateWord, Word, WordRepository, WordSearch},
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
        .route("/languages/{code}/words", post(create_word))
        .route("/languages/{code}/words", get(search_words))
        .route("/languages/{code}/words/{slug}", put(edit_word))
        .route("/languages/{code}/words/{slug}", delete(delete_word))
}

type ApiResponse<T> = AppResult<T>;
type PaginatedApiResponse<T> = AppResult<PaginatedResponse<T>>;

pub async fn create_word(
    s: Session,
    Path(code): Path<String>,
    languages: LanguageRepository,
    words: WordRepository,
    Json(req): Json<CreateWord>,
) -> ApiResponse<Json<Word>> {
    req.validate()?;

    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    let language = languages.find_by_code(&code).await?;

    words.create(requestor, language.id, req).await.map(Json)
}

pub async fn search_words(
    languages: LanguageRepository,
    words: WordRepository,
    Path(code): Path<String>,
    pagination: PaginatedRequest,
    axum::extract::Query(query): axum::extract::Query<WordSearch>,
) -> PaginatedApiResponse<Word> {
    let language = languages.find_by_code(&code).await?;

    words.search(language.id, pagination, query).await
}

pub async fn edit_word(
    s: Session,
    languages: LanguageRepository,
    words: WordRepository,
    Path((code, slug)): Path<(String, String)>,
    Json(updates): Json<UpdateWord>,
) -> ApiResponse<Json<Word>> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    let language = languages.find_by_code(&code).await?;
    let word = words.find_by_slug(language.id, &slug).await?;

    words.update(requestor, word.id, updates).await.map(Json)
}

pub async fn delete_word(
    s: Session,
    languages: LanguageRepository,
    words: WordRepository,
    Path((code, slug)): Path<(String, String)>,
) -> ApiResponse<StatusCode> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    let language = languages.find_by_code(&code).await?;
    let word = words.find_by_slug(language.id, &slug).await?;

    words.delete(requestor, word.id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use serde_json::json;
    use std::sync::Arc;
    use tower::Service;

    use crate::controller::api::tests::{
        delete_without_auth, get, make_authed_user, post, put_without_auth,
    };
    use crate::email::tests::MockEmailService;

    #[tokio::test]
    async fn test_create_word() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let username = crate::tests::random_name();
        let token = make_authed_user(&username, &app, email_service.clone()).await;

        let lang_code = crate::tests::random_code();
        let body = json!({
            "code": lang_code,
            "name": "Test Language",
        });

        let request = post(&token, "languages", body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = json!({
            "abbreviation": "n",
            "name": "Noun",
        });

        let request = post(&token, &format!("languages/{lang_code}/word-classes"), body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = json!({
            "slug": "test",
            "word_class": "n",
            "word": "test",
            "definition": "test definition",
        });

        let request = post(&token, &format!("languages/{lang_code}/words"), body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(body["slug"], "test");
    }

    #[tokio::test]
    async fn test_create_word_unauthorized() {
        let mut app = crate::tests::test_app().await.unwrap();

        let body = json!({
            "slug": "test",
            "word_class": "n",
            "word": "test",
            "definition": "test definition",
        });

        let request =
            crate::controller::api::tests::post_without_auth("languages/test/words", body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_list_words() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let username = crate::tests::random_name();
        let token = make_authed_user(&username, &app, email_service.clone()).await;

        let lang_code = crate::tests::random_code();
        let body = json!({
            "code": lang_code,
            "name": "Test Language",
        });

        let request = post(&token, "languages", body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = json!({
            "abbreviation": "n",
            "name": "Noun",
        });

        let request = post(&token, &format!("languages/{lang_code}/word-classes"), body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // create a few words
        for i in 0..3 {
            let body = json!({
                "slug": format!("test{}", i),
                "word_class": "n",
                "word": format!("test{}", i),
                "definition": format!("test definition {}", i),
            });

            let request = post(&token, &format!("languages/{lang_code}/words"), body).await;
            let response = app.call(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
        }

        let request = get(&format!("languages/{lang_code}/words")).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert!(body["items"].is_array());
        assert_eq!(body["items"].as_array().unwrap().len(), 3);
    }

    #[tokio::test]
    async fn test_list_words_with_search() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let username = crate::tests::random_name();
        let token = make_authed_user(&username, &app, email_service.clone()).await;

        let lang_code = crate::tests::random_code();
        let body = json!({
            "code": lang_code,
            "name": "Test Language",
        });

        let request = post(&token, "languages", body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = json!({
            "abbreviation": "n",
            "name": "Noun",
        });

        let request = post(&token, &format!("languages/{lang_code}/word-classes"), body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // create words
        let body = json!({
            "slug": "unique",
            "word_class": "n",
            "word": "unique",
            "definition": "unique definition",
        });

        let request = post(&token, &format!("languages/{lang_code}/words"), body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let request = get(&format!("languages/{lang_code}/words?q=unique")).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert!(body["items"].is_array());
    }

    #[tokio::test]
    async fn test_list_words_filter_by_class() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let username = crate::tests::random_name();
        let token = make_authed_user(&username, &app, email_service.clone()).await;

        let lang_code = crate::tests::random_code();
        let body = json!({
            "code": lang_code,
            "name": "Test Language",
        });

        let request = post(&token, "languages", body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = json!({
            "abbreviation": "n",
            "name": "Noun",
        });

        let request = post(&token, &format!("languages/{lang_code}/word-classes"), body).await;
        let response = app.call(request).await.unwrap();
        let status = response.status();
        assert_eq!(status, StatusCode::OK);

        let body = json!({
            "abbreviation": "v",
            "name": "Verb",
        });

        let request = post(&token, &format!("languages/{lang_code}/word-classes"), body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // create noun
        let body = json!({
            "slug": "noun1",
            "word_class": "n",
            "word": "noun1",
            "definition": "noun definition",
        });
        let request = post(&token, &format!("languages/{lang_code}/words"), body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // create verb
        let body = json!({
            "slug": "verb1",
            "word_class": "v",
            "word": "verb1",
            "definition": "verb definition",
        });
        let request = post(&token, &format!("languages/{lang_code}/words"), body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // filter by noun
        let request = get(&format!("languages/{lang_code}/words?word_class=n")).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert!(body["items"].is_array());
        let items = body["items"].as_array().unwrap();
        assert!(items.iter().all(|item| item["slug"] == "noun1"));
    }

    #[tokio::test]
    async fn test_edit_word() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let username = crate::tests::random_name();
        let token = make_authed_user(&username, &app, email_service.clone()).await;

        let lang_code = crate::tests::random_code();
        let body = json!({
            "code": lang_code,
            "name": "Test Language",
        });

        let request = post(&token, "languages", body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = json!({
            "abbreviation": "n",
            "name": "Noun",
        });

        let request = post(&token, &format!("languages/{lang_code}/word-classes"), body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = json!({
            "slug": "test",
            "word_class": "n",
            "word": "test",
            "definition": "test definition",
        });

        let request = post(&token, &format!("languages/{lang_code}/words"), body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let update_body = json!({
            "definition": "updated definition",
        });

        let request = crate::controller::api::tests::put(
            &token,
            &format!("languages/{lang_code}/words/test"),
            &update_body,
        );
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(body["definition"], "updated definition");
    }

    #[tokio::test]
    async fn test_edit_word_unauthorized() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let username = crate::tests::random_name();
        let token = make_authed_user(&username, &app, email_service.clone()).await;

        let lang_code = crate::tests::random_code();
        let body = json!({
            "code": lang_code,
            "name": "Test Language",
        });

        let request = post(&token, "languages", body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = json!({
            "abbreviation": "n",
            "name": "Noun",
        });

        let request = post(&token, &format!("languages/{lang_code}/word-classes"), body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = json!({
            "slug": "test",
            "word_class": "n",
            "word": "test",
            "definition": "test definition",
        });

        let request = post(&token, &format!("languages/{lang_code}/words"), body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let update_body = json!({
            "definition": "updated definition",
        });

        let request = put_without_auth(&format!("languages/{lang_code}/words/test"), &update_body);
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_delete_word() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let username = crate::tests::random_name();
        let token = make_authed_user(&username, &app, email_service.clone()).await;

        let lang_code = crate::tests::random_code();
        let body = json!({
            "code": lang_code,
            "name": "Test Language",
        });

        let request = post(&token, "languages", body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = json!({
            "abbreviation": "n",
            "name": "Noun",
        });

        let request = post(&token, &format!("languages/{lang_code}/word-classes"), body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = json!({
            "slug": "test",
            "word_class": "n",
            "word": "test",
            "definition": "test definition",
        });

        let request = post(&token, &format!("languages/{lang_code}/words"), body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let request = crate::controller::api::tests::delete(
            &token,
            &format!("languages/{lang_code}/words/test"),
        );
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn test_delete_word_unauthorized() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let username = crate::tests::random_name();
        let token = make_authed_user(&username, &app, email_service.clone()).await;

        let lang_code = crate::tests::random_code();
        let body = json!({
            "code": lang_code,
            "name": "Test Language",
        });

        let request = post(&token, "languages", body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = json!({
            "abbreviation": "n",
            "name": "Noun",
        });

        let request = post(&token, &format!("languages/{lang_code}/word-classes"), body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = json!({
            "slug": "test",
            "word_class": "n",
            "word": "test",
            "definition": "test definition",
        });

        let request = post(&token, &format!("languages/{lang_code}/words"), body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let request = delete_without_auth(&format!("languages/{lang_code}/words/test"));
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
