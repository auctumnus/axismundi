use crate::{
    err::{AppResult, unauthorized_no_session},
    model::sessions::{SessionObj, SessionRepository},
    util::extract_session::{SESSION_COOKIE_NAME, Session},
};
use axum::Json;
use axum_extra::extract::{CookieJar, cookie::Cookie};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub fn create_router() -> axum::Router<crate::util::AppState> {
    axum::Router::new()
        .route("/sessions", axum::routing::post(login))
        .route("/sessions", axum::routing::get(get_sessions))
}

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

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use serde_json::json;
    use std::sync::Arc;
    use tower::Service;

    use crate::controller::api::tests::{get_with_auth, make_authed_user};
    use crate::email::MockEmailService;

    #[tokio::test]
    async fn test_login() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let username = crate::tests::random_name();
        let password = "TestPassword123!";
        let email = format!("{username}@example.com");

        // create user
        let user_body = json!({
            "username": username,
            "email": email,
            "password": password,
        });

        let request = crate::controller::api::tests::post_without_auth("users", user_body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // verify email
        let sent_emails = email_service.get_sent_emails();
        let email_lowercase = email.to_lowercase();
        let verification_email = sent_emails
            .iter()
            .find(|e| {
                e.to == email_lowercase
                    && e.email_type == crate::email::EmailType::Verification
            })
            .unwrap();

        let user_id = verification_email.user_id;
        let verify_body = json!({
            "token": verification_email.token,
            "email": email_lowercase,
        });

        let request = crate::controller::api::tests::post_without_auth(
            &format!("verify/{user_id}"),
            verify_body,
        )
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // login
        let login_body = json!({
            "email": email,
            "password": password,
        });

        let request =
            crate::controller::api::tests::post_without_auth("sessions", login_body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert!(body["token"].is_string());
        assert!(body["expires_at"].is_string());
    }

    #[tokio::test]
    async fn test_login_invalid_credentials() {
        let mut app = crate::tests::test_app().await.unwrap();

        let login_body = json!({
            "email": "nonexistent@example.com",
            "password": "wrongpassword",
        });

        let request =
            crate::controller::api::tests::post_without_auth("sessions", login_body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_login_unverified_user() {
        let mut app = crate::tests::test_app().await.unwrap();

        let username = crate::tests::random_name();
        let password = "TestPassword123!";
        let email = format!("{username}@example.com");

        // create user but don't verify
        let user_body = json!({
            "username": username,
            "email": email,
            "password": password,
        });

        let request = crate::controller::api::tests::post_without_auth("users", user_body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // try to login
        let login_body = json!({
            "email": email,
            "password": password,
        });

        let request =
            crate::controller::api::tests::post_without_auth("sessions", login_body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_get_sessions() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let username = crate::tests::random_name();
        let token = make_authed_user(&username, &app, email_service.clone()).await;

        let request = get_with_auth(&token, "sessions").await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert!(body.is_array());
        assert!(!body.as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_get_sessions_unauthorized() {
        let mut app = crate::tests::test_app().await.unwrap();

        let request = crate::controller::api::tests::get("sessions").await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
