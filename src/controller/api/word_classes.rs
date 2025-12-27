use crate::{
    err::{AppResult, not_found, unauthorized_no_session},
    model::{
        languages::LanguageRepository,
        word_classes::{
            CreateWordClass, UpdateWordClass, WordClass, WordClassRepository, WordClassSearch,
        },
    },
    pagination::{PaginatedRequest, PaginatedResponse},
    util::extract_session::Session,
};
use axum::{Json, extract::Path, http::StatusCode};

pub fn create_router() -> axum::Router<crate::util::AppState> {
    axum::Router::new()
        .route(
            "/languages/{code}/word-classes",
            axum::routing::post(create_word_class).get(list_word_classes),
        )
        .route(
            "/languages/{code}/word-classes/{abbreviation}",
            axum::routing::get(get_word_class)
                .put(edit_word_class)
                .delete(delete_word_class),
        )
}

type ApiResponse<T> = AppResult<T>;
type PaginatedApiResponse<T> = AppResult<PaginatedResponse<T>>;

pub async fn create_word_class(
    s: Session,
    word_classes: WordClassRepository,
    Path(code): Path<String>,
    Json(create): Json<CreateWordClass>,
) -> ApiResponse<Json<WordClass>> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    word_classes
        .create(requestor, &code, create)
        .await
        .map(Json)
}

pub async fn list_word_classes(
    languages: LanguageRepository,
    word_classes: WordClassRepository,
    Path(code): Path<String>,
    pagination: PaginatedRequest,
    axum::extract::Query(query): axum::extract::Query<WordClassSearch>,
) -> PaginatedApiResponse<WordClass> {
    let language = languages.find_by_code(&code).await?;

    word_classes.search(language.id, pagination, query).await
}

pub async fn get_word_class(
    languages: LanguageRepository,
    word_classes: WordClassRepository,
    Path((code, abbreviation)): Path<(String, String)>,
) -> ApiResponse<Json<WordClass>> {
    let language = languages.find_by_code(&code).await?;
    word_classes
        .find_by_abbreviation(&language.id, &abbreviation)
        .await
        .map(Json)
}

pub async fn edit_word_class(
    s: Session,
    languages: LanguageRepository,
    word_classes: WordClassRepository,
    Path((code, abbreviation)): Path<(String, String)>,
    Json(updates): Json<UpdateWordClass>,
) -> ApiResponse<Json<WordClass>> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    let language = languages.find_by_code(&code).await?;
    let word_class = word_classes
        .find_by_abbreviation(&language.id, &abbreviation)
        .await
        .map_err(|_| not_found(format!("word class '{abbreviation}'")))?;

    word_classes
        .update(requestor, word_class.id, updates)
        .await
        .map(Json)
}

pub async fn delete_word_class(
    s: Session,
    languages: LanguageRepository,
    word_classes: WordClassRepository,
    Path((code, abbreviation)): Path<(String, String)>,
) -> ApiResponse<StatusCode> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    let language = languages.find_by_code(&code).await?;
    let word_class = word_classes
        .find_by_abbreviation(&language.id, &abbreviation)
        .await
        .map_err(|_| not_found(format!("word class '{abbreviation}'")))?;

    word_classes.delete(requestor, word_class.id).await?;
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
    async fn test_create_word_class() {
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
            "abbreviation": "pn",
            "name": "proper noun",
        });

        let request = post(&token, &format!("languages/{lang_code}/word-classes"), body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(body["abbreviation"], "pn");
        assert_eq!(body["name"], "proper noun");
    }

    #[tokio::test]
    async fn test_create_word_class_unauthorized() {
        let mut app = crate::tests::test_app().await.unwrap();

        let body = json!({
            "abbreviation": "pn",
            "name": "proper noun",
        });

        let request =
            crate::controller::api::tests::post_without_auth("languages/test/word-classes", body)
                .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_create_word_class_language_not_found() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let username = crate::tests::random_name();
        let token = make_authed_user(&username, &app, email_service.clone()).await;

        let body = json!({
            "abbreviation": "conj",
            "name": "conjunction",
        });

        let request = post(&token, "languages/nonexistent/word-classes", body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_list_word_classes() {
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

        // create a few word classes
        for (abbr, name) in [
            ("conj", "conjunction"),
            ("part", "particle"),
            ("interj", "interjection"),
        ] {
            let body = json!({
                "abbreviation": abbr,
                "name": name,
            });

            let request = post(&token, &format!("languages/{lang_code}/word-classes"), body).await;
            let response = app.call(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
        }

        let request = get(&format!("languages/{lang_code}/word-classes")).await;
        let response = app.call(request).await.unwrap();

        let body = crate::tests::response_to_value(response.into_body()).await;
        println!("List word classes body: {body}");
        assert!(body["items"].is_array());
        // Should have 3 custom + 7 default = 10 total
        assert_eq!(body["items"].as_array().unwrap().len(), 10);
    }

    #[tokio::test]
    async fn test_list_word_classes_with_search() {
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

        // create word classes
        for (abbr, name) in [("conj", "conjunction"), ("part", "particle")] {
            let body = json!({
                "abbreviation": abbr,
                "name": name,
            });

            let request = post(&token, &format!("languages/{lang_code}/word-classes"), body).await;
            let response = app.call(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
        }

        let request = get(&format!("languages/{lang_code}/word-classes?q=conj")).await;
        let response = app.call(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert!(body["items"].is_array());
        let items = body["items"].as_array().unwrap();
        assert!(!items.is_empty());
        assert!(items.iter().any(|item| item["name"] == "conjunction"));
    }

    #[tokio::test]
    async fn test_get_word_class() {
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
            "abbreviation": "det",
            "name": "determiner",
        });

        let request = post(&token, &format!("languages/{lang_code}/word-classes"), body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let request = get(&format!("languages/{lang_code}/word-classes/det")).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(body["abbreviation"], "det");
        assert_eq!(body["name"], "determiner");
    }

    #[tokio::test]
    async fn test_get_word_class_not_found() {
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

        let request = get(&format!("languages/{lang_code}/word-classes/nonexistent")).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_edit_word_class() {
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
            "abbreviation": "art",
            "name": "article",
        });

        let request = post(&token, &format!("languages/{lang_code}/word-classes"), body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = crate::tests::response_to_value(response.into_body()).await;
        assert!(body["bookmark"].is_string());
        let bookmark = body["bookmark"].as_str().unwrap().to_string();

        let update_body = json!({
            "name": "Article",
        });

        let request = crate::controller::api::tests::put(
            &token,
            &format!("languages/{lang_code}/word-classes/art"),
            &update_body,
        );
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(body["name"], "Article");
        assert_eq!(body["bookmark"], bookmark);
    }

    #[tokio::test]
    async fn test_edit_word_class_unauthorized() {
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
            "abbreviation": "aux",
            "name": "auxiliary",
        });

        let request = post(&token, &format!("languages/{lang_code}/word-classes"), body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let update_body = json!({
            "name": "Auxiliary",
        });

        let request = put_without_auth(
            &format!("languages/{lang_code}/word-classes/aux"),
            &update_body,
        );
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_edit_word_class_not_found() {
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
            "name": "Noun",
        });

        let request = crate::controller::api::tests::put(
            &token,
            &format!("languages/{lang_code}/word-classes/nonexistent"),
            &update_body,
        );
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_delete_word_class() {
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
            "abbreviation": "clf",
            "name": "classifier",
        });

        let request = post(&token, &format!("languages/{lang_code}/word-classes"), body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let request = crate::controller::api::tests::delete(
            &token,
            &format!("languages/{lang_code}/word-classes/clf"),
        );
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        // verify it's deleted
        let request = get(&format!("languages/{lang_code}/word-classes/clf")).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_delete_word_class_unauthorized() {
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
            "abbreviation": "cop",
            "name": "copula",
        });

        let request = post(&token, &format!("languages/{lang_code}/word-classes"), body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let request = delete_without_auth(&format!("languages/{lang_code}/word-classes/cop"));
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_delete_word_class_not_found() {
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
            &format!("languages/{lang_code}/word-classes/nonexistent"),
        );
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
