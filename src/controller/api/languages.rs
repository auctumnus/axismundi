use crate::{
    err::{AppResult, unauthorized_no_session},
    model::{
        language_permissions::LanguagePermissionRepository,
        languages::{CreateLanguage, Language, LanguageRepository, LanguageSearch, UpdateLanguage},
        users::{User, UserRepository, UserSearch},
    },
    pagination::{PaginatedRequest, PaginatedResponse},
    util::{AppState, extract_session::Session},
};
use axum::{
    Json,
    extract::{Path, Query},
    http::StatusCode,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;

pub fn create_router() -> axum::Router<AppState> {
    axum::Router::new()
        .route("/languages", axum::routing::post(create_language))
        .route("/languages", axum::routing::get(list_languages))
        .route("/languages/{code}", axum::routing::get(get_language))
        .route("/languages/{code}", axum::routing::put(edit_language))
        .route("/languages/{code}", axum::routing::delete(delete_language))
        .route(
            "/languages/{code}/owner",
            axum::routing::get(get_language_owner),
        )
        .route(
            "/languages/{code}/editors",
            axum::routing::get(get_language_editors),
        )
        .route("/languages/{code}/like", axum::routing::post(like_language))
        .route(
            "/languages/{code}/unlike",
            axum::routing::post(unlike_language),
        )
}

type ApiResponse<T> = AppResult<T>;
type PaginatedApiResponse<T> = AppResult<Json<PaginatedResponse<T>>>;

pub async fn create_language(
    s: Session,
    languages: LanguageRepository,
    Json(create): Json<CreateLanguage>,
) -> ApiResponse<Json<Language>> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    let language = languages.create(requestor, create).await?;

    Ok(Json(language))
}

pub async fn get_language(
    languages: LanguageRepository,
    Path(code): Path<String>,
) -> ApiResponse<Json<Language>> {
    languages.find_by_code(&code).await.map(Json)
}

#[derive(Deserialize)]
pub struct LanguageSearchQuery {
    pub owned_by: Option<String>,
    pub edited_by: Option<String>,
    pub q: Option<String>,
    pub created_before: Option<DateTime<Utc>>,
    pub created_after: Option<DateTime<Utc>>,
    pub in_family: Option<String>,
}

pub async fn list_languages(
    languages: LanguageRepository,
    _users: UserRepository,
    _permissions: LanguagePermissionRepository,
    pagination: PaginatedRequest,
    axum::extract::Query(query): axum::extract::Query<LanguageSearchQuery>,
) -> PaginatedApiResponse<Language> {
    let edited_by = query
        .edited_by
        .map(|s| s.split(',').map(String::from).collect());

    let search = LanguageSearch {
        q: query.q,
        owned_by: query.owned_by,
        edited_by,
        created_before: query.created_before,
        created_after: query.created_after,
        in_family: query.in_family,
    };

    languages.search(pagination, search).await.map(Json)
}

pub async fn edit_language(
    s: Session,
    languages: LanguageRepository,
    Path(code): Path<String>,
    Json(updates): Json<UpdateLanguage>,
) -> ApiResponse<Json<Language>> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    let language = languages.find_by_code(&code).await?;

    languages
        .update(requestor, language.id, updates)
        .await
        .map(Json)
}

pub async fn delete_language(
    s: Session,
    languages: LanguageRepository,
    Path(code): Path<String>,
) -> ApiResponse<StatusCode> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    let language = languages.find_by_code(&code).await?;

    languages.delete(requestor, language.id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn get_language_owner(
    languages: LanguageRepository,
    users: UserRepository,
    Path(code): Path<String>,
) -> ApiResponse<axum::response::Redirect> {
    let language = languages.find_by_code(&code).await?;
    let owner = users.find_by_id(language.created_by).await?;
    Ok(axum::response::Redirect::to(&format!(
        "/users/{}",
        owner.username
    )))
}

#[axum::debug_handler(state = crate::util::AppState)]
pub async fn get_language_editors(
    languages: LanguageRepository,
    Path(code): Path<String>,
    pagination: PaginatedRequest,
    Query(search): Query<UserSearch>,
) -> PaginatedApiResponse<User> {
    let language = languages.find_by_code(&code).await?;
    languages
        .search_editors_of_language(language.id, pagination, search)
        .await
        .map(Json)
}

#[derive(serde::Serialize)]
pub struct LikeLanguageResponse {
    pub liked: bool,
    pub like_count: i64,
}

pub async fn like_language(
    s: Session,
    languages: LanguageRepository,
    Path(code): Path<String>,
) -> ApiResponse<Json<LikeLanguageResponse>> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    let language = languages.find_by_code(&code).await?;

    let like_count = languages.like_language(language.id, requestor.id).await?;
    let response = LikeLanguageResponse {
        liked: true,
        like_count: like_count.unwrap_or(language.like_count),
    };
    Ok(Json(response))
}

pub async fn unlike_language(
    s: Session,
    languages: LanguageRepository,
    Path(code): Path<String>,
) -> ApiResponse<Json<LikeLanguageResponse>> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    let language = languages.find_by_code(&code).await?;

    let like_count = languages.unlike_language(language.id, requestor.id).await?;
    let response = LikeLanguageResponse {
        liked: false,
        like_count: like_count.unwrap_or(language.like_count),
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
        delete_without_auth, get, make_authed_user, post, print_response_body, put_without_auth,
    };
    use crate::email::MockEmailService;

    #[tokio::test]
    async fn test_create_language() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let username = crate::tests::random_name();
        let token = make_authed_user(&username, &app, email_service.clone()).await;

        let code = crate::tests::random_code();
        let body = json!({
            "code": code,
            "name": "Test Language",
        });

        let request = post(&token, "languages", body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(body["code"], code);
        assert_eq!(body["name"], "Test Language");
    }

    #[tokio::test]
    async fn test_create_language_unauthorized() {
        let mut app = crate::tests::test_app().await.unwrap();

        let body = json!({
            "code": "test",
            "name": "Test Language",
        });

        let request = crate::controller::api::tests::post_without_auth("languages", body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_get_language() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let username = crate::tests::random_name();
        let token = make_authed_user(&username, &app, email_service.clone()).await;

        let code = crate::tests::random_code();
        let body = json!({
            "code": code,
            "name": "Test Language",
        });

        let request = post(&token, "languages", body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let request = get(&format!("languages/{code}")).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(body["code"], code);
        assert_eq!(body["name"], "Test Language");
    }

    #[tokio::test]
    async fn test_get_language_not_found() {
        let mut app = crate::tests::test_app().await.unwrap();

        let request = get("languages/nonexistent").await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_list_languages() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let username = crate::tests::random_name();
        let token = make_authed_user(&username, &app, email_service.clone()).await;

        // create a few languages
        for i in 0..3 {
            let code = format!("{}_{}", crate::tests::random_code(), i);
            let body = json!({
                "code": code,
                "name": format!("Test Language {}", i),
            });

            let request = post(&token, "languages", body).await;
            let response = app.call(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
        }

        let request = get("languages").await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert!(body["items"].is_array());
    }

    #[tokio::test]
    async fn test_list_languages_with_search() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let username = crate::tests::random_name();
        let token = make_authed_user(&username, &app, email_service.clone()).await;

        let code = crate::tests::random_code();
        let body = json!({
            "code": code,
            "name": "Unique Language Name",
        });

        let request = post(&token, "languages", body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let request = get("languages?q=Unique").await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert!(body["items"].is_array());
    }

    #[tokio::test]
    async fn test_edit_language() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let username = crate::tests::random_name();
        let token = make_authed_user(&username, &app, email_service.clone()).await;

        let code = crate::tests::random_code();
        let body = json!({
            "code": code,
            "name": "Test Language",
        });

        let request = post(&token, "languages", body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = crate::tests::response_to_value(response.into_body()).await;
        assert!(body["bookmark"].is_string());
        let bookmark = body["bookmark"].as_str().unwrap();

        let update_body = json!({
            "name": "Updated Language Name",
        });

        let request =
            crate::controller::api::tests::put(&token, &format!("languages/{code}"), &update_body);
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(body["name"], "Updated Language Name");
        assert_eq!(body["bookmark"], bookmark);
    }

    #[tokio::test]
    async fn test_edit_language_unauthorized() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let username = crate::tests::random_name();
        let token = make_authed_user(&username, &app, email_service.clone()).await;

        let code = crate::tests::random_code();
        let body = json!({
            "code": code,
            "name": "Test Language",
        });

        let request = post(&token, "languages", body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let update_body = json!({
            "name": "Updated Language Name",
        });

        let request = put_without_auth(&format!("languages/{code}"), &update_body);
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_delete_language() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let username = crate::tests::random_name();
        let token = make_authed_user(&username, &app, email_service.clone()).await;

        let code = crate::tests::random_code();
        let body = json!({
            "code": code,
            "name": "Test Language",
        });

        let request = post(&token, "languages", body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let request = crate::controller::api::tests::delete(&token, &format!("languages/{code}"));
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        // verify it's deleted
        let request = get(&format!("languages/{code}")).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_delete_language_unauthorized() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let username = crate::tests::random_name();
        let token = make_authed_user(&username, &app, email_service.clone()).await;

        let code = crate::tests::random_code();
        let body = json!({
            "code": code,
            "name": "Test Language",
        });

        let request = post(&token, "languages", body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let request = delete_without_auth(&format!("languages/{code}"));
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_delete_language_not_editor() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let owner_username = crate::tests::random_name();
        let owner_token = make_authed_user(&owner_username, &app, email_service.clone()).await;

        let code = crate::tests::random_code();
        let body = json!({
            "code": code,
            "name": "Test Language",
        });

        let request = post(&owner_token, "languages", body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let other_username = crate::tests::random_name();
        let other_token = make_authed_user(&other_username, &app, email_service.clone()).await;

        let request =
            crate::controller::api::tests::delete(&other_token, &format!("languages/{code}"));
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_delete_language_as_editor() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let owner_username = crate::tests::random_name();
        let owner_token = make_authed_user(&owner_username, &app, email_service.clone()).await;

        let code = crate::tests::random_code();
        let body = json!({
            "code": code,
            "name": "Test Language",
        });

        let request = post(&owner_token, "languages", body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let other_username = crate::tests::random_name();
        let other_token = make_authed_user(&other_username, &app, email_service.clone()).await;

        // Add other user as editor
        let invite_body = json!({
            "permission_level": "editor",
        });
        let request = post(
            &owner_token,
            &format!("languages/{code}/invites/{other_username}"),
            invite_body,
        )
        .await;
        let response = app.call(request).await.unwrap();
        let status = response.status();
        print_response_body(response).await;
        assert_eq!(status, StatusCode::OK);

        // accept invite
        let request = crate::controller::api::tests::post(
            &other_token,
            &format!("languages/{code}/accept-invite"),
            json!({}),
        )
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let request =
            crate::controller::api::tests::delete(&other_token, &format!("languages/{code}"));
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_delete_language_as_admin() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let owner_username = crate::tests::random_name();
        let owner_token = make_authed_user(&owner_username, &app, email_service.clone()).await;

        let code = crate::tests::random_code();
        let body = json!({
            "code": code,
            "name": "Test Language",
        });

        let request = post(&owner_token, "languages", body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let other_username = crate::tests::random_name();
        let other_token = make_authed_user(&other_username, &app, email_service.clone()).await;

        // Add other user as admin
        let invite_body = json!({
            "permission_level": "admin",
        });
        let request = post(
            &owner_token,
            &format!("languages/{code}/invites/{other_username}"),
            invite_body,
        )
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // accept invite
        let request = crate::controller::api::tests::post(
            &other_token,
            &format!("languages/{code}/accept-invite"),
            json!({}),
        )
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let request =
            crate::controller::api::tests::delete(&other_token, &format!("languages/{code}"));
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_get_language_owner() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let username = crate::tests::random_name();
        let token = make_authed_user(&username, &app, email_service.clone()).await;

        let code = crate::tests::random_code();
        let body = json!({
            "code": code,
            "name": "Test Language",
        });

        let request = post(&token, "languages", body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let request = get(&format!("languages/{code}/owner")).await;
        let response = app.call(request).await.unwrap();
        // Should redirect
        assert!(response.status().is_redirection());
    }

    #[tokio::test]
    async fn test_code_cannot_be_search() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let username = crate::tests::random_name();
        let token = make_authed_user(&username, &app, email_service.clone()).await;

        let body = json!({
            "code": "search",
            "name": "Test Language",
        });

        let request = post(&token, "languages", body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_cannot_update_code_to_be_search() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let username = crate::tests::random_name();
        let token = make_authed_user(&username, &app, email_service.clone()).await;

        let code = crate::tests::random_code();
        let body = json!({
            "code": code,
            "name": "Test Language",
        });

        let request = post(&token, "languages", body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let update_body = json!({
            "code": "search",
        });

        let request =
            crate::controller::api::tests::put(&token, &format!("languages/{code}"), &update_body);
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_can_update_code() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let username = crate::tests::random_name();
        let token = make_authed_user(&username, &app, email_service.clone()).await;

        let code = crate::tests::random_code();
        let body = json!({
            "code": code,
            "name": "Test Language",
        });

        let request = post(&token, "languages", body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let new_code = crate::tests::random_code();

        let update_body = json!({
            "code": new_code,
        });

        let request =
            crate::controller::api::tests::put(&token, &format!("languages/{code}"), &update_body);
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let request = get(&format!("languages/{new_code}")).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_like_unlike_language() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let username = crate::tests::random_name();
        let token = make_authed_user(&username, &app, email_service.clone()).await;

        let code = crate::tests::random_code();
        let body = json!({
            "code": code,
            "name": "Test Language",
        });

        let request = post(&token, "languages", body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let request = crate::controller::api::tests::post(
            &token,
            &format!("languages/{code}/like"),
            json!({}),
        )
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(body["liked"], true);
        assert_eq!(body["like_count"], 1);

        let request = crate::controller::api::tests::post(
            &token,
            &format!("languages/{code}/unlike"),
            json!({}),
        )
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(body["liked"], false);
        assert_eq!(body["like_count"], 0);
    }

    /// Helper struct for admin/mod override tests
    struct PermOverrideTestContext {
        pool: sqlx::PgPool,
        app: axum::routing::RouterIntoService<axum::body::Body>,
        _owner_token: String,
        override_user_token: String,
        override_user_id: uuid::Uuid,
        language_id: String,
        language_code: String,
    }

    /// Creates a test context with:
    /// - A language owned by one user
    /// - An admin or moderator user (depending on `user_tag` parameter)
    async fn make_perm_override_test_context(user_tag: &str) -> PermOverrideTestContext {
        use crate::CONFIG;
        use crate::util::AppState;
        use sqlx::PgPool;

        let pool = PgPool::connect(&CONFIG.database_url).await.unwrap();
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let app_state = AppState {
            pool: pool.clone(),
            email_service: email_service_trait,
        };
        let mut app = crate::create_router(app_state).into_service();

        // Create language owned by one user
        let owner_username = crate::tests::random_name();
        let owner_token = make_authed_user(&owner_username, &app, email_service.clone()).await;

        let code = crate::tests::random_code();
        let body = json!({
            "code": code,
            "name": "Test Language",
            "description": "Original description"
        });

        let request = post(&owner_token, "languages", body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let created_language = crate::tests::response_to_value(response.into_body()).await;
        let language_id = created_language["id"].as_str().unwrap().to_string();

        // Create override user (admin or moderator) and give them the appropriate tag
        let override_username = crate::tests::random_name();
        let override_user_token =
            make_authed_user(&override_username, &app, email_service.clone()).await;

        let override_user_id: uuid::Uuid = sqlx::query_scalar!(
            "select id from users where username = $1",
            override_username
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        sqlx::query!(
            "insert into user_tags (user_id, tag) values ($1, $2)",
            override_user_id,
            user_tag
        )
        .execute(&pool)
        .await
        .unwrap();

        PermOverrideTestContext {
            pool,
            app,
            _owner_token: owner_token,
            override_user_token,
            override_user_id,
            language_id,
            language_code: code,
        }
    }

    #[tokio::test]
    async fn test_edit_language_as_admin_creates_audit_log() {
        let ctx = make_perm_override_test_context("admin").await;
        let PermOverrideTestContext {
            pool,
            mut app,
            override_user_token,
            override_user_id,
            language_id,
            language_code,
            ..
        } = ctx;

        // Admin should be able to edit the language even without language-level permissions
        let update_body = json!({
            "name": "Updated by Admin",
            "description": "Updated description"
        });

        let request = crate::controller::api::tests::put(
            &override_user_token,
            &format!("languages/{}", language_code),
            &update_body,
        );
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let updated_language = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(updated_language["name"], "Updated by Admin");
        assert_eq!(updated_language["description"], "Updated description");

        // Verify an audit log was created
        let audit_logs: Vec<serde_json::Value> = sqlx::query!(
            r#"
            select
                id,
                user_id,
                action as "action: crate::model::audit_log::AuditActionType",
                action_at,
                resource_type as "resource_type: crate::model::audit_log::AuditableResource",
                resource_id,
                details
            from audit_logs
            where resource_type = 'language' and resource_id = $1
            order by action_at desc
            "#,
            uuid::Uuid::parse_str(&language_id).unwrap()
        )
        .fetch_all(&pool)
        .await
        .unwrap()
        .into_iter()
        .map(|row| {
            json!({
                "id": row.id,
                "user_id": row.user_id,
                "action": format!("{:?}", row.action),
                "action_at": row.action_at,
                "resource_type": format!("{:?}", row.resource_type),
                "resource_id": row.resource_id,
                "details": row.details
            })
        })
        .collect();

        // Should have at least one audit log (the update)
        assert!(!audit_logs.is_empty(), "No audit logs found");

        // Find the most recent "Updated" action
        let update_log = audit_logs
            .iter()
            .find(|log| log["action"].as_str() == Some("Updated"))
            .expect("No update audit log found");

        // Verify the audit log details
        assert_eq!(update_log["user_id"], override_user_id.to_string());
        assert_eq!(update_log["resource_type"], "Language");
        assert_eq!(update_log["resource_id"], language_id);

        let details = &update_log["details"];
        assert_eq!(details["updates"]["name"], "Updated by Admin");
        assert_eq!(details["updates"]["description"], "Updated description");
    }

    #[tokio::test]
    async fn test_edit_language_as_moderator_creates_audit_log() {
        let ctx = make_perm_override_test_context("moderator").await;
        let PermOverrideTestContext {
            pool,
            mut app,
            override_user_token,
            language_id,
            language_code,
            ..
        } = ctx;

        // Moderator should be able to edit the language even without language-level permissions
        let update_body = json!({
            "name": "Updated by Moderator",
        });

        let request = crate::controller::api::tests::put(
            &override_user_token,
            &format!("languages/{}", language_code),
            &update_body,
        );
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let updated_language = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(updated_language["name"], "Updated by Moderator");

        // Verify an audit log was created
        let audit_log_count: i64 = sqlx::query_scalar!(
            r#"
            select count(*)
            from audit_logs
            where resource_type = 'language'
              and resource_id = $1
            "#,
            uuid::Uuid::parse_str(&language_id).unwrap()
        )
        .fetch_one(&pool)
        .await
        .unwrap()
        .unwrap_or(0);

        assert!(audit_log_count > 0, "No audit log found");
    }

    #[tokio::test]
    async fn test_delete_language_as_admin_creates_audit_log() {
        let ctx = make_perm_override_test_context("admin").await;
        let PermOverrideTestContext {
            pool,
            mut app,
            override_user_token,
            language_id,
            language_code,
            ..
        } = ctx;

        // Admin should be able to delete the language even without language-level permissions
        let request = crate::controller::api::tests::delete(
            &override_user_token,
            &format!("languages/{}", language_code),
        );
        let response = app.call(request).await.unwrap();
        let status = response.status();
        assert!(
            status == StatusCode::OK || status == StatusCode::NO_CONTENT,
            "Expected OK or NO_CONTENT, got {:?}",
            status
        );

        // Verify an audit log was created
        let audit_log_count: i64 = sqlx::query_scalar!(
            r#"
            select count(*)
            from audit_logs
            where resource_type = 'language'
              and resource_id = $1
              and action = 'deleted'
            "#,
            uuid::Uuid::parse_str(&language_id).unwrap()
        )
        .fetch_one(&pool)
        .await
        .unwrap()
        .unwrap_or(0);

        assert!(
            audit_log_count == 1,
            "Expected exactly one delete audit log, got {}",
            audit_log_count
        );
    }
}
