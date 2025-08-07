use axum::{extract::{Path, State}, http::StatusCode, routing::{get, post, put}, Json, Router};
use axum_extra::extract::{cookie::Cookie, CookieJar, Multipart};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use crate::{model::{session::{Session, SessionRepository}, user::{CreateUser, User, UserRepository}}, util::{extract_session::SESSION_COOKIE_NAME, AppState, ExtractSession}};

pub fn create_api_controller(pool: PgPool, s3: crate::util::s3::S3Config) -> Router<AppState> {
    Router::new()
        .route("/users", post(create_user))
        .route("/users/{id}/verify", post(verify_user))
        .route("/users/{id}/profile-picture", put(upload_profile_picture))
        .route("/sessions", post(login))
        .route("/sessions", get(get_sessions))
        .with_state(AppState { pool, s3 })
}

async fn create_user(
    State(AppState { pool, .. }): State<AppState>,
    Json(user): Json<CreateUser>,
) -> Result<Json<User>, (StatusCode, String)> {
    let user_repo = UserRepository::new(pool.clone());
    match user_repo.create(user).await {
        Ok(created_user) => Ok(Json(created_user)),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

#[derive(Deserialize)]
struct LoginCredentials {
    email: String,
    password: String,
}


#[derive(Serialize)]
struct LoginResponse {
    token: String,
    expires_at: DateTime<Utc>,
}

async fn login(
    State(AppState { pool, .. }): State<AppState>,
    jar: CookieJar,
    Json(credentials): Json<LoginCredentials>,
) -> Result<(CookieJar, Json<LoginResponse>), (StatusCode, String)> {
    let session_repo = SessionRepository::new(pool.clone());
    match session_repo.login(&credentials.email, &credentials.password).await {
            Ok(Some((token, session))) => {
                let jar = jar.add(Cookie::build((SESSION_COOKIE_NAME, token.clone()))
                    .path("/")
                    .http_only(true)
                    .secure(true)
                    );
                let response = Json(LoginResponse {
                    token,
                    expires_at: session.expires_at,
                });
                Ok((jar, response))
        },
        Ok(None) => Err((StatusCode::UNAUTHORIZED, "Invalid credentials".to_string())),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

#[derive(Deserialize)]
struct VerifyEmail {
    token: String,
    email: String,
}

async fn verify_user(
    State(AppState { pool, .. }): State<AppState>,
    Path(id): Path<i32>,
    Json(VerifyEmail { token, email }): Json<VerifyEmail>,
) -> Result<StatusCode, (StatusCode, String)> {
    let user_repo = UserRepository::new(pool.clone());
    let user = user_repo.find_by_id(id).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if let Some(user) = user {
        let id = user.id;
        user_repo.verify(id, &email, &token).await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .map_or(Err((StatusCode::NOT_FOUND, "Verification failed".to_string())), |_| {
                Ok(StatusCode::OK)
            })
    } else {
        return Err((StatusCode::NOT_FOUND, "User not found".to_string()));
    }
}

async fn get_sessions(
    State(AppState { pool, .. }): State<AppState>,
    ExtractSession(session): ExtractSession,
) -> Result<Json<Vec<Session>>, (StatusCode, String)> {
    let session_repo = SessionRepository::new(pool);
    let sessions = session_repo.find_by_user_id(session.user_id).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    
    Ok(Json(sessions))
}

const MAX_FILE_SIZE: usize = 5 * 1024 * 1024; // 5MB
const ALLOWED_CONTENT_TYPES: &[&str] = &["image/jpeg", "image/jpg", "image/png", "image/gif", "image/webp"];

fn validate_image(content_type: &str, data: &[u8]) -> Result<(), String> {
    if !ALLOWED_CONTENT_TYPES.contains(&content_type) {
        return Err("Invalid file type. Only JPEG, PNG, GIF, and WebP images are allowed.".to_string());
    }

    if data.len() > MAX_FILE_SIZE {
        return Err("File too large. Maximum size is 5MB.".to_string());
    }

    // Basic magic number validation
    match content_type {
        "image/jpeg" | "image/jpg" => {
            if !data.starts_with(&[0xFF, 0xD8, 0xFF]) {
                return Err("Invalid JPEG file format.".to_string());
            }
        }
        "image/png" => {
            if !data.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
                return Err("Invalid PNG file format.".to_string());
            }
        }
        "image/gif" => {
            if !(data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a")) {
                return Err("Invalid GIF file format.".to_string());
            }
        }
        "image/webp" => {
            if !(data.starts_with(b"RIFF") && data.len() >= 12 && &data[8..12] == b"WEBP") {
                return Err("Invalid WebP file format.".to_string());
            }
        }
        _ => return Err("Unsupported file type.".to_string()),
    }

    Ok(())
}

#[derive(Serialize)]
struct ProfilePictureUploadResponse {
    message: String,
    profile_picture_url: String,
    object_key: String,
}

async fn upload_profile_picture(
    State(AppState { pool, s3 }): State<AppState>,
    Path(user_id): Path<i32>,
    ExtractSession(session): ExtractSession,
    mut multipart: Multipart,
) -> Result<Json<ProfilePictureUploadResponse>, (StatusCode, String)> {
    // Check if user is uploading their own profile picture
    if session.user_id != user_id {
        return Err((StatusCode::FORBIDDEN, "You can only upload your own profile picture".to_string()));
    }

    let mut file_data: Option<Vec<u8>> = None;
    let mut content_type: Option<String> = None;

    while let Some(field) = multipart.next_field().await.map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))? {
        let field_name = field.name().unwrap_or("");
        
        if field_name == "image" {
            content_type = field.content_type().map(|ct| ct.to_string());
            let data = field.bytes().await.map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
            file_data = Some(data.to_vec());
            break;
        }
    }

    let file_data = file_data.ok_or((StatusCode::BAD_REQUEST, "No image file provided".to_string()))?;
    let content_type = content_type.ok_or((StatusCode::BAD_REQUEST, "No content type provided".to_string()))?;

    // Validate the image
    validate_image(&content_type, &file_data).map_err(|e| (StatusCode::BAD_REQUEST, e))?;

    // Upload to S3
    let object_key = s3.upload_profile_picture(user_id, &file_data, &content_type)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Upload failed: {}", e)))?;

    // Update user record with new profile picture object key
    let user_repo = UserRepository::new(pool);
    match user_repo.update_profile_picture(user_id, &object_key).await {
        Ok(Some(user)) => {
            let profile_picture_url = s3.get_profile_picture_url(&object_key);
            Ok(Json(ProfilePictureUploadResponse {
                message: "Profile picture uploaded successfully".to_string(),
                profile_picture_url,
                object_key,
            }))
        }
        Ok(None) => Err((StatusCode::NOT_FOUND, "User not found".to_string())),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}