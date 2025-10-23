use crate::{
    err::{AppResult, unauthorized_no_session},
    model::{
        language::LanguageRepository,
        word::{CreateWord, UpdateWord, Word, WordRepository, WordSearch},
        word_class::WordClassRepository,
    },
    pagination::{PaginatedRequest, PaginatedResponse},
    util::extract_session::Session,
};
use axum::{Json, extract::Path, http::StatusCode};
use serde::Deserialize;
use serde_json::Value as JsonValue;
use validator::Validate;

type ApiResponse<T> = AppResult<T>;
type PaginatedApiResponse<T> = AppResult<PaginatedResponse<T>>;

#[derive(Deserialize)]
pub struct WordSearchQuery {
    pub q: Option<String>,
    pub word_class: Option<String>,
}

#[derive(Deserialize, Validate)]
pub struct CreateWordRequest {
    pub word_class: Option<String>,

    #[validate(length(min = 1, max = 200))]
    pub word: String,

    #[validate(length(min = 1, max = 200))]
    pub slug: String,

    #[validate(length(min = 1, max = 5000))]
    pub definition: String,

    #[validate(length(max = 200))]
    pub ipa: Option<String>,

    #[validate(length(max = 10000))]
    pub notes: Option<String>,

    pub extra: Option<JsonValue>,
}

pub async fn create_word(
    s: Session,
    languages: LanguageRepository,
    words: WordRepository,
    word_classes: WordClassRepository,
    Path(code): Path<String>,
    Json(req): Json<CreateWordRequest>,
) -> ApiResponse<Json<Word>> {
    req.validate()?;

    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    let language = languages.find_by_code(&code).await?;

    let word_class_uuid = if let Some(ref abbr) = req.word_class {
        let classes = word_classes.list_by_language(language.id).await?;
        classes
            .iter()
            .find(|c| c.abbreviation == *abbr)
            .map(|c| c.id)
    } else {
        None
    };

    let create = CreateWord {
        language: language.id,
        word_class: word_class_uuid,
        word: req.word,
        slug: req.slug,
        definition: req.definition,
        ipa: req.ipa,
        notes: req.notes,
        extra: req.extra,
    };

    words.create(requestor, create).await.map(Json)
}

pub async fn list_words(
    languages: LanguageRepository,
    words: WordRepository,
    word_classes: WordClassRepository,
    Path(code): Path<String>,
    pagination: PaginatedRequest,
    axum::extract::Query(query): axum::extract::Query<WordSearchQuery>,
) -> PaginatedApiResponse<Word> {
    let language = languages.find_by_code(&code).await?;

    let word_class_uuid = if let Some(ref abbr) = query.word_class {
        let classes = word_classes.list_by_language(language.id).await?;
        classes
            .iter()
            .find(|c| c.abbreviation == *abbr)
            .map(|c| c.id)
    } else {
        None
    };

    let search = WordSearch {
        pagination,
        text_query: query.q,
        word_class: word_class_uuid,
    };

    words.search(language.id, search).await
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
