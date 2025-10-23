use crate::{
    err::{AppResult, unauthorized_no_session},
    model::session::{SessionObj, SessionRepository},
    util::extract_session::{Session, SESSION_COOKIE_NAME},
};
use axum::Json;
use axum_extra::extract::{CookieJar, cookie::Cookie};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

type ApiResponse<T> = AppResult<T>;

#[derive(Deserialize)]
pub struct LoginCredentials {
    pub email: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub expires_at: DateTime<Utc>,
}

pub async fn login(
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

pub async fn get_sessions(
    s: Session,
    sessions: SessionRepository,
) -> ApiResponse<Json<Vec<SessionObj>>> {
    let Some(session) = s.session() else {
        return Err(unauthorized_no_session());
    };

    sessions.find_by_user_id(session.user_id).await.map(Json)
}
