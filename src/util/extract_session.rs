use axum::{
    extract::FromRequestParts,
    routing::get,
    Router,
    http::{
        StatusCode,
        header::{HeaderValue, USER_AGENT},
        request::Parts,
    },
};
use axum_extra::extract::CookieJar;

use crate::model::session::{Session, SessionRepository};

use super::AppState;

pub struct ExtractSession(pub Session);

pub const SESSION_COOKIE_NAME: &'static str = "session_token";

impl FromRequestParts<AppState> for ExtractSession
{
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let session_repo = SessionRepository::new(state.pool.clone());
        // both token auth and cookie auth are supported
        let session_token = parts
            .headers
            .get("Authorization")
            .and_then(|h| h.to_str().ok())
            .and_then(|s| s.strip_prefix("Bearer "));

        let session = if let Some(token) = session_token {
            session_repo.find(token).await.map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Failed to find session"))?
        } else if let Some(session_cookie) = CookieJar::from_request_parts(parts, state)
            .await
            .ok()
            .and_then(|jar| jar.get(SESSION_COOKIE_NAME).map(|c| c.value().to_string())) {
            session_repo.find(&session_cookie).await.map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Failed to find session"))?
        } else {
            return Err((StatusCode::UNAUTHORIZED, "No session token provided"));
        };

        if let Some(session) = session {
            Ok(ExtractSession(session))
        } else {
            Err((StatusCode::UNAUTHORIZED, "Invalid session token"))
        }
    }
}