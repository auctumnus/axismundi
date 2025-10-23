use crate::{
    err::{AppError, AppResult, bad_request},
    model::users::{CreateUser, User, UserRepository, UserSearch},
    pagination::{PaginatedRequest, PaginatedResponse},
    util::{AppState, extract_session::Session, s3::S3},
};
use axum::{
    extract::{Path, Query, State}, http::StatusCode, Json
};
use axum_extra::extract::Multipart;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

type ApiResponse<T> = AppResult<T>;
type PaginatedApiResponse<T> = AppResult<PaginatedResponse<T>>;

pub async fn get_user(
    State(state): State<AppState>,
    Path(username): Path<String>,
) -> ApiResponse<Json<User>> {
    UserRepository::new(state)
        .find_by_username(&username)
        .await
        .map(Json)
}

pub async fn create_user(
    State(state): State<AppState>,
    Json(user): Json<CreateUser>,
) -> ApiResponse<Json<User>> {
    let user_repo = UserRepository::new(state);
    user_repo.create(user).await.map(Json)
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

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| bad_request(format!("Multipart error: {e}")))?
    {
        let field_name = field.name().unwrap_or("");

        if field_name == "image" {
            let data = field
                .bytes()
                .await
                .map_err(|e| bad_request(format!("Field bytes error: {e}")))?;
            file_data = Some(data.to_vec());
            break;
        }
    }

    let file_data = file_data.ok_or_else(|| bad_request("No image file provided"))?;

    if file_data.len() > MAX_FILE_SIZE {
        return Err(bad_request("File size exceeds the maximum limit of 5MB"));
    }

    // Upload to S3
    let object_key = S3.upload_profile_picture(user.id, &file_data).await?;

    // Update user record with new profile picture object key
    match users
        .update_profile_picture(requestor, user.id, &object_key)
        .await?
    {
        Some(_) => {
            let profile_picture_url = S3.get_object_url(&object_key);
            Ok(Json(ProfilePictureUploadResponse {
                profile_picture_url,
            }))
        }
        None => Err(crate::err::not_found("User not found")),
    }
}


#[axum::debug_handler(state = AppState)]
pub async fn search_users(
    users: UserRepository,
    pagination: PaginatedRequest,
    Query(query): Query<UserSearch>,
) -> PaginatedApiResponse<User> {
    users.search(pagination, query).await
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::http::StatusCode;
    use serde_json::json;
    use tower::Service;

    use crate::{
        controller::api::tests::{get, make_authed_user, post_without_auth},
        email::tests::MockEmailService,
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
        assert_eq!(body["username"], name);
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
        assert_eq!(body["username"], name);
        assert_eq!(body["display_name"], "Test User");
        assert_eq!(body["description"], "This is a test user account");
        assert_eq!(body["pronouns"], "they/them");
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

        let image_bytes = include_bytes!("../../../resources/profile-picture.png");

        // Create proper multipart form data
        let boundary = "----ThisWillNotAppearInAnActualBody";
        let mut body = Vec::new();
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        body.extend_from_slice(
            b"Content-Disposition: form-data; name=\"image\"; filename=\"profile-picture.png\"\r\n",
        );
        body.extend_from_slice(b"Content-Type: image/png\r\n\r\n");
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
        let profile_picture_response = reqwest::get(profile_picture_url).await.unwrap();
        assert_eq!(profile_picture_response.status(), StatusCode::OK);
    }
}
