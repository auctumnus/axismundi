use crate::{
    err::{bad_request, AppError, AppResult},
    model::{email_verification_tokens::EmailVerificationTokenRepository, password_reset_tokens::PasswordResetTokenRepository, users::{CreateUser, UpdateUser, User, UserRepository, UserSearch}},
    pagination::{PaginatedRequest, PaginatedResponse},
    util::{extract_session::Session, s3::S3, AppState},
};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post, put},
};
use axum_extra::extract::Multipart;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

type ApiResponse<T> = AppResult<T>;
type PaginatedApiResponse<T> = AppResult<PaginatedResponse<T>>;

pub fn create_users_router() -> (Router<AppState>, Router<AppState>) {
    let secure_routes = Router::new()
        .route("/users", post(create_user))
        .route("/verify/{id}", post(verify_user))
        .route("/resend-verification/{id}", post(resend_verification_email))
        .route("/users/{username}", put(update_user))
        .route("/reset-password/start", post(reset_password_start))
        .route("/reset-password/complete", post(reset_password_complete));


    let default_routes = Router::new()
        .route("/users/{username}", get(get_user))
        .route("/users", get(search_users))
        .route(
            "/users/{username}/profile-picture",
            put(upload_profile_picture),
        );

    (secure_routes, default_routes)
}

pub async fn get_user(
    State(state): State<AppState>,
    Path(username): Path<String>,
) -> ApiResponse<Json<User>> {
    UserRepository::new(state)
        .find_by_username(&username)
        .await
        .map(Json)
}

#[derive(Serialize)]
pub struct CreateUserResponse {
    pub user: User,
    pub resend_token: Uuid,
}

pub async fn create_user(
    State(state): State<AppState>,
    Json(user): Json<CreateUser>,
) -> ApiResponse<Json<CreateUserResponse>> {
    let user_repo = UserRepository::new(state);
    user_repo.create(user).await.map(|(u, token)| Json(CreateUserResponse { user: u, resend_token: token.id }))
}

pub async fn update_user(
    s: Session,
    users: UserRepository,
    Path(username): Path<String>,
    Json(update): Json<UpdateUser>,
) -> ApiResponse<Json<User>> {
    let Some(requestor) = s.user() else {
        return Err(crate::err::unauthorized_no_session());
    };

    let user = users.find_by_username(&username).await?;

    users.update(requestor, user.id, update).await.map(Json)
}

#[derive(Deserialize)]
pub(crate) struct VerifyEmail {
    token: String,
    email: String,
}

pub async fn verify_user(
    users: UserRepository,
    Path(id): Path<Uuid>,
    Json(VerifyEmail { token, email }): Json<VerifyEmail>,
) -> ApiResponse<StatusCode> {
    users
        .verify(id, &email, &token)
        .await
        .map(|_| StatusCode::OK)
}

pub async fn resend_verification_email(
    tokens: EmailVerificationTokenRepository,
    Path(token_id): Path<Uuid>,
) -> ApiResponse<StatusCode> {
    tokens.resend(token_id).await.map(|_| StatusCode::OK)
}

const MAX_FILE_SIZE: usize = 5 * 1024 * 1024; // 5MB

#[derive(Serialize)]
pub(crate) struct ProfilePictureUploadResponse {
    profile_picture_url: String,
}

pub async fn upload_profile_picture(
    s: Session,
    users: UserRepository,
    Path(username): Path<String>,
    mut multipart: Multipart,
) -> ApiResponse<Json<ProfilePictureUploadResponse>> {
    let Some(requestor) = s.user() else {
        return Err(crate::err::unauthorized_no_session());
    };

    let user = users.find_by_username(&username).await?;

    // Check if user is uploading their own profile picture
    if requestor.id != user.id {
        return Err(AppError::new(
            "You can only upload your own profile picture".to_string(),
            StatusCode::FORBIDDEN,
        ));
    }

    let mut file_data: Option<Vec<u8>> = None;
    let mut content_type: Option<String> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| bad_request(format!("Multipart error: {e}")))?
    {
        let field_name = field.name().unwrap_or("");

        if field_name == "image" {
            content_type = field.content_type().map(|s| s.to_string());
            let data = field
                .bytes()
                .await
                .map_err(|e| bad_request(format!("Field bytes error: {e}")))?;
            file_data = Some(data.to_vec());
            break;
        }
    }

    let file_data = file_data.ok_or_else(|| bad_request("No image file provided"))?;
    let content_type = content_type.ok_or_else(|| bad_request("No content type provided"))?;

    if file_data.len() > MAX_FILE_SIZE {
        return Err(bad_request("File size exceeds the maximum limit of 5MB"));
    }

    // Upload to S3 (uploads the original to minio)
    let filename = S3.upload_profile_picture(user.id, &file_data, &content_type).await?;

    // Update user record with new profile picture filename
    match users
        .update_profile_picture(requestor, user.id, &filename)
        .await?
    {
        Some(_) => {
            let profile_picture_url = S3.get_profile_picture_url(&filename);
            Ok(Json(ProfilePictureUploadResponse {
                profile_picture_url,
            }))
        }
        None => Err(crate::err::not_found("User not found")),
    }
}

pub async fn search_users(
    users: UserRepository,
    pagination: PaginatedRequest,
    Query(query): Query<UserSearch>,
) -> PaginatedApiResponse<User> {
    users.search(pagination, query).await
}

#[derive(Deserialize)]
pub(crate) struct ResetPasswordRequest {
    email: String,
}

pub async fn reset_password_start(
    users: UserRepository,
    tokens: PasswordResetTokenRepository,
    Json(ResetPasswordRequest { email }): Json<ResetPasswordRequest>,
) -> ApiResponse<StatusCode> {
    println!("started password reset");
    let Ok(user) = users.find_by_email(&email).await else {
        // Do not reveal whether the email exists
        // TODO: consider adding a small delay here to mitigate timing attacks
        return Ok(StatusCode::OK);
    };
    let token = tokens.create(user.id).await?;

    println!("sending password reset email to {}", user.email);

    if let Err(e) = tokens.send(user.id, &user.email, &token).await {
        eprintln!("Failed to send password reset email: {e}");
    }

    Ok(StatusCode::OK)
}

#[derive(Deserialize)]
pub(crate) struct PasswordResetComplete {
    uuid: Uuid,
    token: String,
    new_password: String
}

pub async fn reset_password_complete(
    users: UserRepository,
    tokens: PasswordResetTokenRepository,
    Json(PasswordResetComplete { uuid, token, new_password }): Json<PasswordResetComplete>,
) -> ApiResponse<StatusCode> {
    let Ok(user) = users.find_by_id(uuid).await else {
        return Err(bad_request("Invalid or expired password reset token"));
    };
    let Some(token) = tokens.find_by_token(uuid, &token).await? else {
        return Err(bad_request("Invalid or expired password reset token"));
    };
    users.reset_password(user.id, token, &new_password).await?;

    Ok(StatusCode::OK)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::http::StatusCode;
    use serde_json::json;
    use sqlx::PgPool;
    use tower::Service;
    use uuid::Uuid;

    use crate::{
        config::CONFIG, controller::api::tests::{get, make_authed_user, post_without_auth}, create_router, email::{self, MockEmailService}, util::AppState
    };

    #[tokio::test]
    async fn test_create_user() {
        let mut app = crate::tests::test_app().await.unwrap();

        let name = crate::tests::random_name();
        let email = format!("{}@example.com", name.clone());

        let body = json!({
            "username": name,
            "email": email,
            "password": "MyVerySecureAndUniquePassword2024!"
        });

        let request = post_without_auth("users", body).await;

        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(body["user"]["username"], name);

        // verify user has a default profile picture
        let profile_picture_url = body["user"].get("profile_picture_url");
        assert!(profile_picture_url.is_some());
        let url = profile_picture_url.unwrap().as_str().unwrap();
        assert!(url.contains("default-pfps"));

        // verify resend_token is present
        assert!(body.get("resend_token").is_some());
    }

    #[tokio::test]
    async fn test_create_user_with_optional_fields() {
        let mut app = crate::tests::test_app().await.unwrap();

        let name = crate::tests::random_name();
        let email = format!("{}@example.com", name.clone());

        let body = json!({
            "username": name,
            "email": email,
            "password": "MyVerySecureAndUniquePassword2024!",
            "display_name": "Test User",
            "description": "This is a test user account",
            "pronouns": "they/them",
            "gender": "abc123"
        });

        let request = post_without_auth("users", body).await;

        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(body["user"]["username"], name);
        assert_eq!(body["user"]["display_name"], "Test User");
        assert_eq!(body["user"]["description"], "This is a test user account");
        assert_eq!(body["user"]["pronouns"], "they/them");
    }

    #[tokio::test]
    async fn test_create_user_duplicate_username() {
        let mut app = crate::tests::test_app().await.unwrap();

        let name = crate::tests::random_name();
        let email1 = format!("{}@example.com", name.clone());
        let email2 = format!("{}_2@example.com", name.clone());

        let body1 = json!({
            "username": name,
            "email": email1,
            "password": "MyVerySecureAndUniquePassword2024!"
        });

        let request1 = post_without_auth("users", body1).await;
        let response1 = app.call(request1).await.unwrap();
        assert_eq!(response1.status(), StatusCode::OK);

        // Try to create another user with the same username
        let body2 = json!({
            "username": name,
            "email": email2,
            "password": "MyVerySecureAndUniquePassword2024!"
        });

        let request2 = post_without_auth("users", body2).await;
        let response2 = app.call(request2).await.unwrap();
        assert_eq!(response2.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_create_user_duplicate_email() {
        let mut app = crate::tests::test_app().await.unwrap();

        let name1 = crate::tests::random_name();
        let name2 = crate::tests::random_name();
        let email = format!("{}@example.com", name1.clone());

        let body1 = json!({
            "username": name1,
            "email": email,
            "password": "MyVerySecureAndUniquePassword2024!"
        });

        let request1 = post_without_auth("users", body1).await;
        let response1 = app.call(request1).await.unwrap();
        assert_eq!(response1.status(), StatusCode::OK);

        // Try to create another user with the same email
        let body2 = json!({
            "username": name2,
            "email": email,
            "password": "MyVerySecureAndUniquePassword2024!"
        });

        let request2 = post_without_auth("users", body2).await;
        let response2 = app.call(request2).await.unwrap();
        assert_eq!(response2.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_create_user_weak_password() {
        let mut app = crate::tests::test_app().await.unwrap();

        let name = crate::tests::random_name();
        let email = format!("{}@example.com", name.clone());

        let body = json!({
            "username": name,
            "email": email,
            "password": "password"
        });

        let request = post_without_auth("users", body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_create_user_invalid_username() {
        let mut app = crate::tests::test_app().await.unwrap();

        let email = "test@example.com";

        // Username with uppercase letters (invalid)
        let body = json!({
            "username": "InvalidUser",
            "email": email,
            "password": "MyVerySecureAndUniquePassword2024!"
        });

        let request = post_without_auth("users", body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_create_user_invalid_email() {
        let mut app = crate::tests::test_app().await.unwrap();

        let name = crate::tests::random_name();

        let body = json!({
            "username": name,
            "email": "notanemail",
            "password": "MyVerySecureAndUniquePassword2024!"
        });

        let request = post_without_auth("users", body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_create_user_short_password() {
        let mut app = crate::tests::test_app().await.unwrap();

        let name = crate::tests::random_name();
        let email = format!("{}@example.com", name.clone());

        let body = json!({
            "username": name,
            "email": email,
            "password": "short"
        });

        let request = post_without_auth("users", body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_get_user_not_found() {
        let mut app = crate::tests::test_app().await.unwrap();

        let request = get("users/nonexistentuser").await;

        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_get_user_success() {
        let mut app = crate::tests::test_app().await.unwrap();

        let name = crate::tests::random_name();
        let email = format!("{}@example.com", name.clone());

        // Create user
        let body = json!({
            "username": name,
            "email": email,
            "password": "MyVerySecureAndUniquePassword2024!"
        });

        let request = post_without_auth("users", body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // Get user
        let request = get(&format!("users/{name}")).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(body["username"], name);
    }

    #[tokio::test]
    async fn test_get_user_sensitive_fields_not_exposed() {
        let mut app = crate::tests::test_app().await.unwrap();

        let name = crate::tests::random_name();
        let email = format!("{}@example.com", name.clone());

        // Create user
        let body = json!({
            "username": name,
            "email": email,
            "password": "MyVerySecureAndUniquePassword2024!"
        });

        let request = post_without_auth("users", body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // Get user and check sensitive fields are not exposed
        let request = get(&format!("users/{name}")).await;
        let response = app.call(request).await.unwrap();

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert!(body.get("password_hash").is_none());
        assert!(body.get("password").is_none());
        assert!(body.get("email").is_none());
        assert!(body.get("id").is_none());
    }

    #[tokio::test]
    async fn test_search_users_no_query() {
        let mut app = crate::tests::test_app().await.unwrap();

        let request = get("users").await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert!(body["items"].is_array());
    }

    #[tokio::test]
    async fn test_search_users_with_text_query() {
        let mut app = crate::tests::test_app().await.unwrap();

        let name = crate::tests::random_name();
        let email = format!("{}@example.com", name.clone());

        // Create user
        let body = json!({
            "username": name,
            "email": email,
            "password": "MyVerySecureAndUniquePassword2024!"
        });

        let request = post_without_auth("users", body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // Search for user
        let request = get(&format!("users?q={name}")).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert!(body["items"].is_array());
    }

    #[tokio::test]
    async fn test_search_users_pagination() {
        let mut app = crate::tests::test_app().await.unwrap();

        // Create multiple users
        for _ in 0..3 {
            let name = crate::tests::random_name();
            let email = format!("{}@example.com", name.clone());

            let body = json!({
                "username": name,
                "email": email,
                "password": "MyVerySecureAndUniquePassword2024!"
            });

            let request = post_without_auth("users", body).await;
            let response = app.call(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
        }

        // Search with limit
        let request = get("users?limit=2").await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert!(body["items"].is_array());
        assert!(body["items"].as_array().unwrap().len() <= 2);
    }

    #[tokio::test]
    async fn test_upload_profile_picture() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let name = crate::tests::random_name();

        let token = make_authed_user(&name, &app, email_service.clone()).await;

        let image_bytes = include_bytes!("../../../resources/default-pfps/1.webp");

        // Create proper multipart form data
        let boundary = "----ThisWillNotAppearInAnActualBody";
        let mut body = Vec::new();
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        body.extend_from_slice(
            b"Content-Disposition: form-data; name=\"image\"; filename=\"1.webp\"\r\n",
        );
        body.extend_from_slice(b"Content-Type: image/webp\r\n\r\n");
        body.extend_from_slice(image_bytes);
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

        let request = crate::controller::api::tests::put_multipart(
            &token,
            &format!("users/{name}/profile-picture"),
            body,
        );

        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let get_user_request = get(&format!("users/{name}")).await;
        let get_user_response = app.call(get_user_request).await.unwrap();
        assert_eq!(get_user_response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(get_user_response.into_body()).await;
        let profile_picture_url = body.get("profile_picture_url");
        assert!(profile_picture_url.is_some());

        // verify we can access the url;
        // TODO: we should also check that the image is "the same" as what we uploaded
        // in the future, we may also do some transformation (resizing, re-encoding, etc)
        // so we probably need a visual hash or something?

        let profile_picture_url = profile_picture_url.unwrap().as_str().unwrap();
        println!("Profile picture URL: {}", profile_picture_url);
        let profile_picture_response = reqwest::get(profile_picture_url).await.unwrap();
        assert_eq!(profile_picture_response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_upload_profile_picture_unauthorized() {
        let mut app = crate::tests::test_app().await.unwrap();

        let name = crate::tests::random_name();

        let image_bytes = include_bytes!("../../../resources/default-pfps/1.webp");

        // Create proper multipart form data
        let boundary = "----ThisWillNotAppearInAnActualBody";
        let mut body = Vec::new();
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        body.extend_from_slice(
            b"Content-Disposition: form-data; name=\"image\"; filename=\"1.webp\"\r\n",
        );
        body.extend_from_slice(b"Content-Type: image/webp\r\n\r\n");
        body.extend_from_slice(image_bytes);
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

        let request = crate::controller::api::tests::put_multipart_no_auth(
            &format!("users/{name}/profile-picture"),
            body,
        );

        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_upload_profile_picture_other_user() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let name1 = crate::tests::random_name();
        let name2 = crate::tests::random_name();

        let token = make_authed_user(&name1, &app, email_service.clone()).await;

        make_authed_user(&name2, &app, email_service).await;

        let image_bytes = include_bytes!("../../../resources/default-pfps/1.webp");

        // Create proper multipart form data
        let boundary = "----ThisWillNotAppearInAnActualBody";
        let mut body = Vec::new();
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        body.extend_from_slice(
            b"Content-Disposition: form-data; name=\"image\"; filename=\"1.webp\"\r\n",
        );
        body.extend_from_slice(b"Content-Type: image/webp\r\n\r\n");
        body.extend_from_slice(image_bytes);
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

        let request = crate::controller::api::tests::put_multipart(
            &token,
            &format!("users/{name2}/profile-picture"),
            body,
        );

        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_update_user_username() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let name = crate::tests::random_name();

        let token = make_authed_user(&name, &app, email_service.clone()).await;

        let user = get(&format!("users/{name}")).await;
        let user_response = app.call(user).await.unwrap();
        let body = crate::tests::response_to_value(user_response.into_body()).await;
        let bookmark = body["bookmark"].as_str().unwrap().to_string();

        let new_display_name = "Updated Display Name";

        let body = json!({
            "display_name": new_display_name
        });

        let request = crate::controller::api::tests::put(&token, &format!("users/{name}"), &body);

        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(body["display_name"], new_display_name);
        assert!(body["bookmark"].is_string());
        assert_eq!(body["bookmark"], bookmark);
    }

    #[tokio::test]
    async fn test_update_user_unauthorized() {
        let mut app = crate::tests::test_app().await.unwrap();
        let name = crate::tests::random_name();
        let body = json!({
            "display_name": "Should Not Work"
        });
        let request =
            crate::controller::api::tests::put_without_auth(&format!("users/{name}"), &body);
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_update_user_other_user() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let name1 = crate::tests::random_name();
        let name2 = crate::tests::random_name();

        let token = make_authed_user(&name1, &app, email_service.clone()).await;

        make_authed_user(&name2, &app, email_service).await;

        let body = json!({
            "display_name": "Should Not Work"
        });

        let request = crate::controller::api::tests::put(&token, &format!("users/{name2}"), &body);

        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_password_reset() {
        // 1. setup app
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        // 2. create user and get session token
        let username = crate::tests::random_name();
        let token = make_authed_user(&username, &app, email_service.clone()).await;

        // verify we're logged in
        let request = crate::controller::api::tests::get_with_auth(&token, "sessions").await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // 3. initiate password reset
        email_service.clear(); // clear verification emails
        let request = post_without_auth(
            "reset-password/start",
            json!({ "email": format!("{username}@example.com") }),
        )
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // 4. find token and uuid in mock email service
        let sent_emails = email_service.get_sent_emails();
        let password_reset_email = sent_emails
            .iter()
            .find(|e| e.email_type == crate::email::EmailType::PasswordReset)
            .expect("password reset email should be sent");

        let reset_token = password_reset_email.token.clone();
        let user_id = password_reset_email.user_id;

        // 5. complete password reset
        let new_password = "NewSecurePassword123!";
        let reset_body = json!({
            "uuid": user_id,
            "token": reset_token,
            "new_password": new_password
        });
        let request = post_without_auth("reset-password/complete", reset_body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // 6. verify old session is invalidated
        let request = crate::controller::api::tests::get_with_auth(&token, "sessions").await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        // 7. login with new password
        let login_body = json!({
            "email": format!("{username}@example.com"),
            "password": new_password
        });
        let request = post_without_auth("sessions", login_body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        let new_token = body.get("token").unwrap().as_str().unwrap();

        // 8. verify we are the correct user by getting sessions
        let request = crate::controller::api::tests::get_with_auth(new_token, "sessions").await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert!(body.is_array());
        assert!(!body.as_array().unwrap().is_empty());

        // verify we can access our user profile
        let request = get(&format!("users/{username}")).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(body["username"], username);
    }

    #[tokio::test]
    async fn test_password_reset_expired_token() {
        // 1. setup app
        let pool = PgPool::connect(&CONFIG.database_url).await.unwrap();
        let email_service = std::sync::Arc::new(email::MockEmailService::new());
        let app_state = AppState {
            pool: pool.clone(),
            email_service: email_service.clone(),
        };
        let mut app = create_router(app_state).into_service();

        // 2. create user
        let username = crate::tests::random_name();
        make_authed_user(&username, &app, email_service.clone()).await;

        // 3. start password reset
        email_service.clear();
        let request = post_without_auth(
            "reset-password/start",
            json!({ "email": format!("{username}@example.com") }),
        )
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // 4. get token from email
        let sent_emails = email_service.get_sent_emails();
        let password_reset_email = sent_emails
            .iter()
            .find(|e| e.email_type == crate::email::EmailType::PasswordReset)
            .expect("password reset email should be sent");

        let reset_token = password_reset_email.token.clone();
        let user_id = password_reset_email.user_id;

        // 5. manually expire token in database
        sqlx::query!(
            "UPDATE password_reset_tokens SET expires_at = NOW() - INTERVAL '1 day' WHERE user_id = $1",
            user_id
        )
        .execute(&pool)
        .await
        .unwrap();

        // 6. attempt to complete password reset and expect failure
        let reset_body = json!({
            "uuid": user_id,
            "token": reset_token,
            "new_password": "NewSecurePassword123!"
        });
        let request = post_without_auth("reset-password/complete", reset_body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_password_reset_invalid_token() {
        // 1. setup app
        let pool = PgPool::connect(&CONFIG.database_url).await.unwrap();
        let email_service = std::sync::Arc::new(email::MockEmailService::new());
        let app_state = AppState {
            pool: pool.clone(),
            email_service: email_service.clone(),
        };
        let mut app = create_router(app_state).into_service();

        // 2. create user
        let username = crate::tests::random_name();
        make_authed_user(&username, &app, email_service.clone()).await;

        // 3. start password reset
        email_service.clear();
        let request = post_without_auth(
            "reset-password/start",
            json!({ "email": format!("{username}@example.com") }),
        )
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // 4. get token from email
        let sent_emails = email_service.get_sent_emails();
        let password_reset_email = sent_emails
            .iter()
            .find(|e| e.email_type == crate::email::EmailType::PasswordReset)
            .expect("password reset email should be sent");

        let user_id = password_reset_email.user_id;

        // 5. attempt to complete password reset with wrong token
        let reset_body = json!({
            "uuid": user_id,
            "token": "totally_wrong_token_12345",
            "new_password": "NewSecurePassword123!"
        });
        let request = post_without_auth("reset-password/complete", reset_body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_password_reset_wrong_uuid() {
        // 1. setup app
        let pool = PgPool::connect(&CONFIG.database_url).await.unwrap();
        let email_service = std::sync::Arc::new(email::MockEmailService::new());
        let app_state = AppState {
            pool: pool.clone(),
            email_service: email_service.clone(),
        };
        let mut app = create_router(app_state).into_service();

        // 2. create user
        let username = crate::tests::random_name();
        make_authed_user(&username, &app, email_service.clone()).await;

        // 3. start password reset
        email_service.clear();
        let request = post_without_auth(
            "reset-password/start",
            json!({ "email": format!("{username}@example.com") }),
        )
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // 4. get token from email
        let sent_emails = email_service.get_sent_emails();
        let password_reset_email = sent_emails
            .iter()
            .find(|e| e.email_type == crate::email::EmailType::PasswordReset)
            .expect("password reset email should be sent");

        let reset_token = password_reset_email.token.clone();

        // 5. attempt to complete password reset with wrong uuid
        let wrong_uuid = uuid::Uuid::new_v4();
        let reset_body = json!({
            "uuid": wrong_uuid,
            "token": reset_token,
            "new_password": "NewSecurePassword123!"
        });
        let request = post_without_auth("reset-password/complete", reset_body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_resend_verification_email() {
        // 1. setup app
        let pool = PgPool::connect(&CONFIG.database_url).await.unwrap();
        let email_service = Arc::new(email::MockEmailService::new());
        let app_state = AppState {
            pool: pool.clone(),
            email_service: email_service.clone(),
        };
        let mut app = create_router(app_state).into_service();

        // 2. create user
        let username = crate::tests::random_name();
        let email = format!("{username}@example.com");
        let body = json!({
            "username": username,
            "email": email,
            "password": "MyVerySecureAndUniquePassword2024!"
        });
        let request = post_without_auth("users", body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        let token_id: Uuid = serde_json::from_value(body["resend_token"].clone()).unwrap();

        // 3. get original verification email
        let sent_emails = email_service.get_sent_emails();
        let original_email = sent_emails
            .iter()
            .find(|e| e.email_type == crate::email::EmailType::Verification)
            .expect("verification email should be sent");
        let original_token = original_email.token.clone();
        let user_id = original_email.user_id;

        // 5. resend verification email
        email_service.clear();
        let request = post_without_auth(
            &format!("resend-verification/{token_id}"),
            json!({}),
        )
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // 6. verify new email was sent
        let sent_emails = email_service.get_sent_emails();
        let new_email = sent_emails
            .iter()
            .find(|e| e.email_type == crate::email::EmailType::Verification)
            .expect("new verification email should be sent");
        let new_token = new_email.token.clone();

        // tokens should be different
        assert_ne!(original_token, new_token);

        // 7. verify old token is invalidated
        let old_token_invalidated: bool = sqlx::query_scalar!(
            "SELECT invalidated_at IS NOT NULL FROM email_verification_tokens WHERE id = $1",
            token_id
        )
        .fetch_one(&pool)
        .await
        .unwrap()
        .unwrap();
        assert!(old_token_invalidated);

        // 8. verify new token works
        let verify_body = json!({
            "token": new_token,
            "email": email
        });
        let request = post_without_auth(
            &format!("verify/{}", user_id),
            verify_body,
        )
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_resend_verification_email_invalid_token() {
        // 1. setup app
        let pool = PgPool::connect(&CONFIG.database_url).await.unwrap();
        let email_service = Arc::new(email::MockEmailService::new());
        let app_state = AppState {
            pool: pool.clone(),
            email_service: email_service.clone(),
        };
        let mut app = create_router(app_state).into_service();

        // 2. attempt to resend with random uuid
        let random_uuid = uuid::Uuid::new_v4();
        let request = post_without_auth(
            &format!("resend-verification/{random_uuid}"),
            json!({}),
        )
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_resend_verification_email_already_invalidated() {
        // 1. setup app
        let pool = PgPool::connect(&CONFIG.database_url).await.unwrap();
        let email_service = Arc::new(email::MockEmailService::new());
        let app_state = AppState {
            pool: pool.clone(),
            email_service: email_service.clone(),
        };
        let mut app = create_router(app_state).into_service();

        // 2. create user
        let username = crate::tests::random_name();
        let email = format!("{username}@example.com");
        let body = json!({
            "username": username,
            "email": email,
            "password": "MyVerySecureAndUniquePassword2024!"
        });
        let request = post_without_auth("users", body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        let token_id: Uuid = serde_json::from_value(body["resend_token"].clone()).unwrap();

        sqlx::query!(
            "UPDATE email_verification_tokens SET invalidated_at = NOW() WHERE id = $1",
            token_id
        )
        .execute(&pool)
        .await
        .unwrap();

        // 5. attempt to resend with invalidated token
        let request = post_without_auth(
            &format!("resend-verification/{token_id}"),
            json!({}),
        )
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

}
