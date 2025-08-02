use axum::{extract::{Path, State}, http::StatusCode, routing::{get, post}, Json, Router};
use axum_extra::extract::{cookie::Cookie, CookieJar};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use crate::{model::{session::{Session, SessionRepository}, user::{CreateUser, User, UserRepository}}, util::{extract_session::SESSION_COOKIE_NAME, AppState, ExtractSession}};

pub fn create_api_controller(pool: PgPool) -> Router<AppState> {
    Router::new()
        .route("/users", post(create_user))
        .route("/users/{id}/verify", post(verify_user))
        .route("/sessions", post(login))
        .route("/sessions", get(get_sessions))
        .with_state(AppState { pool })
}

async fn create_user(
    State(AppState { pool }): State<AppState>,
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
    State(AppState { pool }): State<AppState>,
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
    State(AppState { pool }): State<AppState>,
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
    State(AppState { pool }): State<AppState>,
    ExtractSession(session): ExtractSession,
) -> Result<Json<Vec<Session>>, (StatusCode, String)> {
    let session_repo = SessionRepository::new(pool);
    let sessions = session_repo.find_by_user_id(session.user_id).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    
    Ok(Json(sessions))
}