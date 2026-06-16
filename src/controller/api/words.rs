use crate::{
    err::{AppResult, unauthorized_no_session},
    model::{
        languages::LanguageRepository,
        words::{
            CreateWord, CrossLanguageSearchResponse, UpdateWord, Word, WordRepository, WordSearch,
            WordWithCategories,
        },
    },
    pagination::{PaginatedRequest, PaginatedResponse},
    util::{ensure_verified, extract_session::Session},
};
use axum::{
    Json,
    extract::Path,
    http::StatusCode,
    routing::{delete, get, post, put},
};
use validator::Validate;

const CATEGORY_LIMIT: i64 = 5;

async fn wrap_with_categories(words: &WordRepository, word: Word) -> AppResult<WordWithCategories> {
    let categories = words.load_categories(word.id, CATEGORY_LIMIT).await?;
    Ok(WordWithCategories { word, categories })
}

async fn wrap_listing_with_categories(
    words: &WordRepository,
    items: Vec<Word>,
) -> AppResult<Vec<WordWithCategories>> {
    let ids: Vec<uuid::Uuid> = items.iter().map(|w| w.id).collect();
    let category_lists = words.load_categories_batch(&ids, CATEGORY_LIMIT).await?;
    Ok(items
        .into_iter()
        .zip(category_lists)
        .map(|(word, categories)| WordWithCategories { word, categories })
        .collect())
}

pub fn create_router() -> axum::Router<crate::util::AppState> {
    axum::Router::new()
        .route("/words/search", get(search_words_across_languages))
        .route("/languages/{code}/words", post(create_word))
        .route("/languages/{code}/words", get(search_words))
        .route("/languages/{code}/words/{slug}/{lemma}", get(get_word))
        .route("/languages/{code}/words/{slug}/{lemma}", put(edit_word))
        .route(
            "/languages/{code}/words/{slug}/{lemma}",
            delete(delete_word),
        )
        .route(
            "/languages/{code}/words/{slug}/{lemma}/like",
            post(like_word),
        )
        .route(
            "/languages/{code}/words/{slug}/{lemma}/unlike",
            post(unlike_word),
        )
}

type ApiResponse<T> = AppResult<T>;
type PaginatedApiResponse<T> = AppResult<PaginatedResponse<T>>;

pub async fn create_word(
    s: Session,
    Path(code): Path<String>,
    languages: LanguageRepository,
    words: WordRepository,
    Json(req): Json<CreateWord>,
) -> ApiResponse<Json<WordWithCategories>> {
    req.validate()?;

    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    let language = languages.find_by_code(&code).await?;

    let word = words.create(requestor, language.id, req).await?;
    let result = wrap_with_categories(&words, word).await?;
    Ok(Json(result))
}

pub async fn get_word(
    s: Session,
    languages: LanguageRepository,
    words: WordRepository,
    Path((code, slug, lemma)): Path<(String, String, i32)>,
) -> ApiResponse<Json<WordWithCategories>> {
    let language = languages.find_by_code(&code).await?;
    let word = words
        .find_by_slug_and_lemma(s.user(), language.id, &slug, lemma)
        .await?;
    let result = wrap_with_categories(&words, word).await?;
    Ok(Json(result))
}

pub async fn search_words(
    languages: LanguageRepository,
    words: WordRepository,
    Path(code): Path<String>,
    pagination: PaginatedRequest,
    axum_extra::extract::Query(query): axum_extra::extract::Query<WordSearch>,
) -> PaginatedApiResponse<WordWithCategories> {
    let language = languages.find_by_code(&code).await?;

    let response = words.search(&language.id, pagination, query).await?;
    let items = wrap_listing_with_categories(&words, response.items).await?;
    Ok(PaginatedResponse {
        items,
        total: response.total,
        offset: response.offset,
        limit: response.limit,
        has_more: response.has_more,
    })
}

#[derive(serde::Deserialize)]
pub struct CrossLanguageSearchQuery {
    pub q: String,
    pub exclude_id: Option<uuid::Uuid>,
    #[allow(dead_code)]
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_limit() -> i64 {
    10
}

pub async fn search_words_across_languages(
    s: Session,
    words: WordRepository,
    pagination: PaginatedRequest,
    axum::extract::Query(query): axum::extract::Query<CrossLanguageSearchQuery>,
) -> ApiResponse<Json<CrossLanguageSearchResponse>> {
    let Some(user) = s.user() else {
        return Err(unauthorized_no_session());
    };
    ensure_verified(user)?;

    let results = words
        .search_across_languages(user, &query.q, query.exclude_id, pagination.limit as i64)
        .await?;

    Ok(Json(results))
}

pub async fn edit_word(
    s: Session,
    languages: LanguageRepository,
    words: WordRepository,
    Path((code, slug, lemma)): Path<(String, String, i32)>,
    Json(updates): Json<UpdateWord>,
) -> ApiResponse<Json<WordWithCategories>> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    let language = languages.find_by_code(&code).await?;

    let word = words
        .update_by_lemma(requestor, language.id, &slug, lemma, updates)
        .await?;
    let result = wrap_with_categories(&words, word).await?;
    Ok(Json(result))
}

pub async fn delete_word(
    s: Session,
    languages: LanguageRepository,
    words: WordRepository,
    Path((code, slug, lemma)): Path<(String, String, i32)>,
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

#[derive(serde::Serialize)]
pub struct LikeWordResponse {
    pub liked: bool,
    pub like_count: i64,
}

pub async fn like_word(
    s: Session,
    languages: LanguageRepository,
    words: WordRepository,
    Path((code, slug, lemma)): Path<(String, String, i32)>,
) -> ApiResponse<Json<LikeWordResponse>> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    let language = languages.find_by_code(&code).await?;
    let word = words
        .find_by_slug_and_lemma(s.user(), language.id, &slug, lemma)
        .await?;

    let like_count = words.like_word(word.id, requestor.id).await?;
    let response = LikeWordResponse {
        liked: true,
        like_count: like_count.unwrap_or(word.like_count),
    };
    Ok(Json(response))
}

pub async fn unlike_word(
    s: Session,
    languages: LanguageRepository,
    words: WordRepository,
    Path((code, slug, lemma)): Path<(String, String, i32)>,
) -> ApiResponse<Json<LikeWordResponse>> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    let language = languages.find_by_code(&code).await?;
    let word = words
        .find_by_slug_and_lemma(s.user(), language.id, &slug, lemma)
        .await?;

    let like_count = words.unlike_word(word.id, requestor.id).await?;
    let response = LikeWordResponse {
        liked: false,
        like_count: like_count.unwrap_or(word.like_count),
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
        create_test_language, delete_without_auth, get, make_authed_user, post, put_without_auth,
    };
    use crate::email::MockEmailService;

    struct TestContext {
        token: String,
        #[allow(dead_code)]
        username: String,
        app: axum::routing::RouterIntoService<axum::body::Body>,
        language: serde_json::Value,
    }

    async fn create_test_context() -> TestContext {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let username = crate::tests::random_name();
        let token = make_authed_user(&username, &app, email_service.clone()).await;

        let language = create_test_language(&token, &mut app).await;

        TestContext {
            token,
            username,
            app,
            language,
        }
    }

    #[tokio::test]
    async fn test_create_word() {
        let mut ctx = create_test_context().await;
        let lang_code = ctx.language["code"].as_str().unwrap();

        let body = json!({
            "slug": "test",
            "word_class": "n",
            "word": "test",
        });

        let request = post(&ctx.token, &format!("languages/{lang_code}/words"), body).await;
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(body["slug"], "test");
    }

    #[tokio::test]
    async fn test_create_word_with_inline_definitions() {
        let mut ctx = create_test_context().await;
        let lang_code = ctx.language["code"].as_str().unwrap().to_string();

        let body = json!({
            "word_class": "n",
            "word": "inlinedefs",
            "definitions": [
                { "definition": "first gloss" },
                { "definition": "second gloss", "context": "formal" },
            ],
        });

        let request = post(&ctx.token, &format!("languages/{lang_code}/words"), body).await;
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // The definitions should have been created in order, in the same request.
        let request = get(&format!(
            "languages/{lang_code}/words/inlinedefs/1/definitions"
        ))
        .await;
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        let items = body["items"].as_array().unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["definition"], "first gloss");
        assert_eq!(items[0]["position"], 0);
        assert_eq!(items[1]["definition"], "second gloss");
        assert_eq!(items[1]["context"], "formal");
        assert_eq!(items[1]["position"], 1);
    }

    #[tokio::test]
    async fn test_create_word_unauthorized() {
        let mut app = crate::tests::test_app().await.unwrap();

        let body = json!({
            "slug": "test",
            "word_class": "n",
            "word": "test",
        });

        let request =
            crate::controller::api::tests::post_without_auth("languages/test/words", body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_list_words() {
        let mut ctx = create_test_context().await;
        let lang_code = ctx.language["code"].as_str().unwrap();

        // create a few words
        for i in 0..3 {
            let body = json!({
                "slug": format!("test{}", i),
                "word_class": "n",
                "word": format!("test{}", i),
            });

            let request = post(&ctx.token, &format!("languages/{lang_code}/words"), body).await;
            let response = ctx.app.call(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
        }

        let request = get(&format!("languages/{lang_code}/words")).await;
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert!(body["items"].is_array());
        assert_eq!(body["items"].as_array().unwrap().len(), 3);
    }

    #[tokio::test]
    async fn test_list_words_with_search() {
        let mut ctx = create_test_context().await;
        let lang_code = ctx.language["code"].as_str().unwrap();

        // create words
        let body = json!({
            "slug": "unique",
            "word_class": "n",
            "word": "unique",
        });

        let request = post(&ctx.token, &format!("languages/{lang_code}/words"), body).await;
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let request = get(&format!("languages/{lang_code}/words?q=unique")).await;
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert!(body["items"].is_array());
    }

    #[tokio::test]
    async fn test_list_words_filter_by_class() {
        let mut ctx = create_test_context().await;
        let lang_code = ctx.language["code"].as_str().unwrap();

        // create noun
        let body = json!({
            "slug": "noun1",
            "word_class": "n",
            "word": "noun1",
        });
        let request = post(&ctx.token, &format!("languages/{lang_code}/words"), body).await;
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // create verb
        let body = json!({
            "slug": "verb1",
            "word_class": "v",
            "word": "verb1",
        });
        let request = post(&ctx.token, &format!("languages/{lang_code}/words"), body).await;
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // filter by noun
        let request = get(&format!("languages/{lang_code}/words?word_class=n")).await;
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert!(body["items"].is_array());
        let items = body["items"].as_array().unwrap();
        assert!(items.iter().all(|item| item["slug"] == "noun1"));
    }

    #[tokio::test]
    async fn test_edit_word() {
        let mut ctx = create_test_context().await;
        let lang_code = ctx.language["code"].as_str().unwrap();

        let body = json!({
            "word_class": "n",
            "word": "test",
        });

        let request = post(&ctx.token, &format!("languages/{lang_code}/words"), body).await;
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let create_body = crate::tests::response_to_value(response.into_body()).await;
        assert!(create_body["bookmark"].is_string());
        let bookmark = create_body["bookmark"].as_str().unwrap().to_string();

        let update_body = json!({
            "word": "test123",
        });

        let request = crate::controller::api::tests::put(
            &ctx.token,
            &format!("languages/{lang_code}/words/test/1"),
            &update_body,
        );
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let update_response = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(update_response["bookmark"], bookmark);
    }

    #[tokio::test]
    async fn test_edit_word_unauthorized() {
        let mut ctx = create_test_context().await;
        let lang_code = ctx.language["code"].as_str().unwrap();

        let body = json!({
            "slug": "test",
            "word_class": "n",
            "word": "test",
        });

        let request = post(&ctx.token, &format!("languages/{lang_code}/words"), body).await;
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let update_body = json!({
            "word": "test_new",
        });

        let request =
            put_without_auth(&format!("languages/{lang_code}/words/test/1"), &update_body);
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_delete_word() {
        let mut ctx = create_test_context().await;
        let lang_code = ctx.language["code"].as_str().unwrap();

        let body = json!({
            "slug": "test",
            "word_class": "n",
            "word": "test",
        });

        let request = post(&ctx.token, &format!("languages/{lang_code}/words"), body).await;
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let request = crate::controller::api::tests::delete(
            &ctx.token,
            &format!("languages/{lang_code}/words/test/1"),
        );
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn test_delete_word_unauthorized() {
        let mut ctx = create_test_context().await;
        let lang_code = ctx.language["code"].as_str().unwrap();

        let body = json!({
            "slug": "test",
            "word_class": "n",
            "word": "test",
        });

        let request = post(&ctx.token, &format!("languages/{lang_code}/words"), body).await;
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let request = delete_without_auth(&format!("languages/{lang_code}/words/test/1"));
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
