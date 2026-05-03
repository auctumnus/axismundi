use crate::{
    err::{AppResult, not_found, unauthorized_no_session},
    model::{
        languages::LanguageRepository,
        word_categories::{
            CreateWordCategory, UpdateWordCategory, WordCategory, WordCategoryRepository,
            WordCategorySearch,
        },
    },
    pagination::{PaginatedRequest, PaginatedResponse},
    util::extract_session::Session,
};
use axum::{Json, extract::Path, http::StatusCode};

pub fn create_router() -> axum::Router<crate::util::AppState> {
    axum::Router::new()
        .route(
            "/languages/{code}/word-categories",
            axum::routing::post(create_word_category).get(list_word_categories),
        )
        .route(
            "/languages/{code}/word-categories/{abbreviation}",
            axum::routing::get(get_word_category)
                .put(edit_word_category)
                .delete(delete_word_category),
        )
}

type ApiResponse<T> = AppResult<T>;
type PaginatedApiResponse<T> = AppResult<PaginatedResponse<T>>;

pub async fn create_word_category(
    s: Session,
    word_categories: WordCategoryRepository,
    Path(code): Path<String>,
    Json(create): Json<CreateWordCategory>,
) -> ApiResponse<Json<WordCategory>> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    word_categories
        .create(requestor, &code, create)
        .await
        .map(Json)
}

pub async fn list_word_categories(
    languages: LanguageRepository,
    word_categories: WordCategoryRepository,
    Path(code): Path<String>,
    pagination: PaginatedRequest,
    axum::extract::Query(query): axum::extract::Query<WordCategorySearch>,
) -> PaginatedApiResponse<WordCategory> {
    let language = languages.find_by_code(&code).await?;

    word_categories.search(language.id, pagination, query).await
}

pub async fn get_word_category(
    languages: LanguageRepository,
    word_categories: WordCategoryRepository,
    Path((code, abbreviation)): Path<(String, String)>,
) -> ApiResponse<Json<WordCategory>> {
    let language = languages.find_by_code(&code).await?;
    word_categories
        .find_by_abbreviation(&language.id, &abbreviation)
        .await
        .map(Json)
}

pub async fn edit_word_category(
    s: Session,
    languages: LanguageRepository,
    word_categories: WordCategoryRepository,
    Path((code, abbreviation)): Path<(String, String)>,
    Json(updates): Json<UpdateWordCategory>,
) -> ApiResponse<Json<WordCategory>> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    let language = languages.find_by_code(&code).await?;
    let word_category = word_categories
        .find_by_abbreviation(&language.id, &abbreviation)
        .await
        .map_err(|_| not_found(format!("word category '{abbreviation}'")))?;

    word_categories
        .update(requestor, word_category.id, updates)
        .await
        .map(Json)
}

pub async fn delete_word_category(
    s: Session,
    languages: LanguageRepository,
    word_categories: WordCategoryRepository,
    Path((code, abbreviation)): Path<(String, String)>,
) -> ApiResponse<StatusCode> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    let language = languages.find_by_code(&code).await?;
    let word_category = word_categories
        .find_by_abbreviation(&language.id, &abbreviation)
        .await
        .map_err(|_| not_found(format!("word category '{abbreviation}'")))?;

    word_categories.delete(requestor, word_category.id).await?;
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
    use crate::email::MockEmailService;

    #[tokio::test]
    async fn test_create_word_category() {
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
            "abbreviation": "m",
            "name": "masculine",
        });

        let request = post(
            &token,
            &format!("languages/{lang_code}/word-categories"),
            body,
        )
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(body["abbreviation"], "m");
        assert_eq!(body["name"], "masculine");
    }

    #[tokio::test]
    async fn test_create_word_category_unauthorized() {
        let mut app = crate::tests::test_app().await.unwrap();

        let body = json!({
            "abbreviation": "m",
            "name": "masculine",
        });

        let request = crate::controller::api::tests::post_without_auth(
            "languages/test/word-categories",
            body,
        )
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_create_word_category_language_not_found() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let username = crate::tests::random_name();
        let token = make_authed_user(&username, &app, email_service.clone()).await;

        let body = json!({
            "abbreviation": "m",
            "name": "masculine",
        });

        let request = post(&token, "languages/nonexistent/word-categories", body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_list_word_categories() {
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

        for (abbr, name) in [("m", "masculine"), ("f", "feminine"), ("n", "neuter")] {
            let body = json!({
                "abbreviation": abbr,
                "name": name,
            });

            let request = post(
                &token,
                &format!("languages/{lang_code}/word-categories"),
                body,
            )
            .await;
            let response = app.call(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
        }

        let request = get(&format!("languages/{lang_code}/word-categories")).await;
        let response = app.call(request).await.unwrap();

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert!(body["items"].is_array());
        assert_eq!(body["items"].as_array().unwrap().len(), 3);
    }

    #[tokio::test]
    async fn test_list_word_categories_with_search() {
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

        for (abbr, name) in [("m", "masculine"), ("f", "feminine")] {
            let body = json!({
                "abbreviation": abbr,
                "name": name,
            });

            let request = post(
                &token,
                &format!("languages/{lang_code}/word-categories"),
                body,
            )
            .await;
            let response = app.call(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
        }

        let request = get(&format!("languages/{lang_code}/word-categories?q=mas")).await;
        let response = app.call(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert!(body["items"].is_array());
        let items = body["items"].as_array().unwrap();
        assert!(!items.is_empty());
        assert!(items.iter().any(|item| item["name"] == "masculine"));
    }

    #[tokio::test]
    async fn test_get_word_category() {
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
            "abbreviation": "anim",
            "name": "animate",
        });

        let request = post(
            &token,
            &format!("languages/{lang_code}/word-categories"),
            body,
        )
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let request = get(&format!("languages/{lang_code}/word-categories/anim")).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(body["abbreviation"], "anim");
        assert_eq!(body["name"], "animate");
    }

    #[tokio::test]
    async fn test_get_word_category_not_found() {
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

        let request = get(&format!(
            "languages/{lang_code}/word-categories/nonexistent"
        ))
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_edit_word_category() {
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
            "abbreviation": "form",
            "name": "formal",
        });

        let request = post(
            &token,
            &format!("languages/{lang_code}/word-categories"),
            body,
        )
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = crate::tests::response_to_value(response.into_body()).await;
        assert!(body["bookmark"].is_string());
        let bookmark = body["bookmark"].as_str().unwrap().to_string();

        let update_body = json!({
            "name": "Formal",
        });

        let request = crate::controller::api::tests::put(
            &token,
            &format!("languages/{lang_code}/word-categories/form"),
            &update_body,
        );
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(body["name"], "Formal");
        assert_eq!(body["bookmark"], bookmark);
    }

    #[tokio::test]
    async fn test_edit_word_category_unauthorized() {
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
            "abbreviation": "casu",
            "name": "casual",
        });

        let request = post(
            &token,
            &format!("languages/{lang_code}/word-categories"),
            body,
        )
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let update_body = json!({
            "name": "Casual",
        });

        let request = put_without_auth(
            &format!("languages/{lang_code}/word-categories/casu"),
            &update_body,
        );
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_edit_word_category_not_found() {
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

        let update_body = json!({
            "name": "Animate",
        });

        let request = crate::controller::api::tests::put(
            &token,
            &format!("languages/{lang_code}/word-categories/nonexistent"),
            &update_body,
        );
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_delete_word_category() {
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
            "abbreviation": "plural",
            "name": "plural",
        });

        let request = post(
            &token,
            &format!("languages/{lang_code}/word-categories"),
            body,
        )
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let request = crate::controller::api::tests::delete(
            &token,
            &format!("languages/{lang_code}/word-categories/plural"),
        );
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        let request = get(&format!("languages/{lang_code}/word-categories/plural")).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_delete_word_category_unauthorized() {
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
            "abbreviation": "dual",
            "name": "dual",
        });

        let request = post(
            &token,
            &format!("languages/{lang_code}/word-categories"),
            body,
        )
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let request = delete_without_auth(&format!("languages/{lang_code}/word-categories/dual"));
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_delete_word_category_not_found() {
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

        let request = crate::controller::api::tests::delete(
            &token,
            &format!("languages/{lang_code}/word-categories/nonexistent"),
        );
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_word_response_includes_categories_array() {
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

        // create two categories
        for (abbr, name) in [("m", "masculine"), ("f", "feminine")] {
            let body = json!({
                "abbreviation": abbr,
                "name": name,
            });
            let request = post(
                &token,
                &format!("languages/{lang_code}/word-categories"),
                body,
            )
            .await;
            let response = app.call(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
        }

        let body = json!({
            "word": "kitten",
            "word_class": "n",
            "categories": ["m", "f"],
        });

        let request = post(&token, &format!("languages/{lang_code}/words"), body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let request = get(&format!("languages/{lang_code}/words/kitten/1")).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert!(body["categories"].is_array());
        let cats = body["categories"].as_array().unwrap();
        assert_eq!(cats.len(), 2);
        let abbrevs: Vec<&str> = cats
            .iter()
            .map(|c| c["abbreviation"].as_str().unwrap())
            .collect();
        assert!(abbrevs.contains(&"m"));
        assert!(abbrevs.contains(&"f"));
    }
}
