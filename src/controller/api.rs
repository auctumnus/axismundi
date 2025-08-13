use std::sync::Arc;

use crate::{
    err::{AppError, AppResult, bad_request, not_found, unauthorized_no_session},
    model::{
        session::{SessionObj, SessionRepository},
        user::{CreateUser, User, UserRepository},
    },
    util::{
        AppState,
        extract_session::{SESSION_COOKIE_NAME, Session},
        s3::S3,
    },
};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post, put},
};
use axum_extra::extract::{CookieJar, Multipart, cookie::Cookie};
use chrono::{DateTime, Utc};
use governor::middleware::NoOpMiddleware;
use serde::{Deserialize, Serialize};
use tower_governor::governor::GovernorConfig;

type ApiResponse<T> = AppResult<T>;

pub fn create_api_controller() -> Router<AppState> {
    let secure_governor = Arc::new(GovernorConfig::<_, NoOpMiddleware>::secure());
    let normal_governor = Arc::new(GovernorConfig::<_, NoOpMiddleware>::default());

    let secure_limiter = secure_governor.limiter().clone();
    let normal_limiter = normal_governor.limiter().clone();
    let interval = std::time::Duration::from_secs(60);

    std::thread::spawn(move || {
        loop {
            std::thread::sleep(interval);
            secure_limiter.retain_recent();
            normal_limiter.retain_recent();
        }
    });

    let secure_routes = Router::<AppState>::new()
        .route("/users", post(create_user))
        .route("/sessions", post(login))
        .route("/sessions", get(get_sessions))
        .route("/users/{id}/verify", post(verify_user))
        .route(
            "/users/{username}/profile-picture",
            put(upload_profile_picture),
        )
        .layer(tower_governor::GovernorLayer {
            config: secure_governor,
        });

    let normal_routes = Router::<AppState>::new()
        .route("/users/{username}", get(get_user))
        .layer(tower_governor::GovernorLayer {
            config: normal_governor,
        });

    Router::<AppState>::new()
        .merge(secure_routes)
        .merge(normal_routes)
}

async fn get_user(
    State(AppState { pool, .. }): State<AppState>,
    Path(username): Path<String>,
) -> ApiResponse<Json<User>> {
    UserRepository::new(pool)
        .find_by_username(&username)
        .await
        .map(Json)
}

async fn create_user(
    State(AppState { pool, .. }): State<AppState>,
    Json(user): Json<CreateUser>,
) -> ApiResponse<Json<User>> {
    let user_repo = UserRepository::new(pool.clone());
    user_repo.create(user).await.map(Json)
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
    jar: CookieJar,
    sessions: SessionRepository,
    Json(credentials): Json<LoginCredentials>,
) -> ApiResponse<(CookieJar, Json<LoginResponse>)> {
    sessions
        .login(&credentials.email, &credentials.password)
        .await
        .map(|(token, session)| {
            let jar = jar.add(
                Cookie::build((SESSION_COOKIE_NAME, token.clone()))
                    .path("/")
                    .http_only(true)
                    .secure(true),
            );
            let response = Json(LoginResponse {
                token,
                expires_at: session.expires_at,
            });

            (jar, response)
        })
}

#[derive(Deserialize)]
struct VerifyEmail {
    token: String,
    email: String,
}

async fn verify_user(
    users: UserRepository,
    Path(id): Path<i32>,
    Json(VerifyEmail { token, email }): Json<VerifyEmail>,
) -> ApiResponse<StatusCode> {
    users
        .verify(id, &email, &token)
        .await
        .map(|_| StatusCode::OK)
}

async fn get_sessions(
    s: Session,
    sessions: SessionRepository,
) -> ApiResponse<Json<Vec<SessionObj>>> {
    let Some(session) = s.session() else {
        return Err(unauthorized_no_session());
    };

    sessions.find_by_user_id(session.user_id).await.map(Json)
}

const MAX_FILE_SIZE: usize = 5 * 1024 * 1024; // 5MB

#[derive(Serialize)]
struct ProfilePictureUploadResponse {
    profile_picture_url: String,
}
async fn upload_profile_picture(
    s: Session,
    users: UserRepository,
    Path(username): Path<String>,
    mut multipart: Multipart,
) -> ApiResponse<Json<ProfilePictureUploadResponse>> {
    let Some(session) = s.session() else {
        return Err(unauthorized_no_session());
    };

    let user = users.find_by_username(&username).await?;

    // Check if user is uploading their own profile picture
    if session.user_id != user.id {
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

    // Upload to S3
    let object_key = S3.upload_profile_picture(user.id, &file_data).await?;

    // Update user record with new profile picture object key
    match users.update_profile_picture(user.id, &object_key).await? {
        Some(_user) => {
            let profile_picture_url = S3.get_object_url(&object_key);
            Ok(Json(ProfilePictureUploadResponse {
                profile_picture_url,
            }))
        }
        None => Err(not_found("User not found")),
    }
}
