use axum::{
    Router,
};
#[cfg(not(test))]
use governor::middleware::NoOpMiddleware;

use crate::util::AppState;
#[cfg(not(test))]
use std::sync::Arc;
#[cfg(not(test))]
use tower_governor::governor::GovernorConfig;

mod language_invites;
mod language_permissions;
mod languages;
mod sessions;
mod users;
mod word_classes;
mod words;
mod bookmarks;
mod word_relations;
mod definitions;
mod translatable;
mod translations;
mod quotations;
mod quotation_suggestions;
mod user_activities;

// pretty sure i need that there, actually...
#[allow(clippy::needless_return)]
pub fn create_api_controller() -> Router<AppState> {
    let (secure_user_routes, normal_user_routes) = users::create_users_router();

    let secure_routes = Router::<AppState>::new()
        .merge(sessions::create_router())
        .merge(secure_user_routes);

    let normal_routes = Router::<AppState>::new()
        .merge(normal_user_routes)
        .merge(bookmarks::create_router())
        .merge(languages::create_router())
        .merge(language_permissions::create_router())
        .merge(language_invites::create_router())
        .merge(word_classes::create_router())
        .merge(words::create_router())
        .merge(word_relations::create_router())
        .merge(definitions::create_router())
        .merge(translatable::create_router())
        .merge(translations::create_router())
        .merge(quotations::create_router())
        .merge(quotation_suggestions::create_router())
        .merge(user_activities::create_router());

    // Only apply rate limiting in non-test builds
    #[cfg(not(test))]
    {
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

        return Router::<AppState>::new()
            .merge(secure_routes.layer(tower_governor::GovernorLayer {
                config: secure_governor,
            }))
            .merge(normal_routes.layer(tower_governor::GovernorLayer {
                config: normal_governor,
            }));
    }

    #[cfg(test)]
    Router::<AppState>::new()
        .merge(secure_routes)
        .merge(normal_routes)
}

#[cfg(test)]
mod tests {
    use axum::{body::Body, http::Request, routing::RouterIntoService};
    use reqwest::StatusCode;
    use std::sync::Arc;

    use crate::email::MockEmailService;

    pub(crate) async fn post_without_auth(uri: &str, body: serde_json::Value) -> Request<Body> {
        Request::builder()
            .uri(format!("/api/{uri}"))
            .method("POST")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap()
    }

    pub(crate) async fn post(token: &str, uri: &str, body: serde_json::Value) -> Request<Body> {
        Request::builder()
            .uri(format!("/api/{uri}"))
            .method("POST")
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {token}"))
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap()
    }

    pub(crate) async fn get(uri: &str) -> Request<Body> {
        Request::builder()
            .uri(format!("/api/{uri}"))
            .method("GET")
            .header("content-type", "application/json")
            .body(Body::empty())
            .unwrap()
    }

    pub(crate) async fn get_with_auth(token: &str, uri: &str) -> Request<Body> {
        Request::builder()
            .uri(format!("/api/{uri}"))
            .method("GET")
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap()
    }

    #[allow(dead_code)]
    pub(crate) fn post_multipart(token: &str, uri: &str, body: Vec<u8>) -> Request<Body> {
        Request::builder()
            .uri(format!("/api/{uri}"))
            .method("POST")
            .header(
                "content-type",
                "multipart/form-data; boundary=----ThisWillNotAppearInAnActualBody",
            )
            .header("authorization", format!("Bearer {token}"))
            .body(Body::from(body))
            .unwrap()
    }

    pub(crate) fn put_multipart(token: &str, uri: &str, body: Vec<u8>) -> Request<Body> {
        Request::builder()
            .uri(format!("/api/{uri}"))
            .method("PUT")
            .header(
                "content-type",
                "multipart/form-data; boundary=----ThisWillNotAppearInAnActualBody",
            )
            .header("authorization", format!("Bearer {token}"))
            .body(Body::from(body))
            .unwrap()
    }

    pub(crate) fn put_multipart_no_auth(uri: &str, body: Vec<u8>) -> Request<Body> {
        Request::builder()
            .uri(format!("/api/{uri}"))
            .method("PUT")
            .header(
                "content-type",
                "multipart/form-data; boundary=----ThisWillNotAppearInAnActualBody",
            )
            .body(Body::from(body))
            .unwrap()
    }

    pub(crate) fn put(token: &str, uri: &str, body: &serde_json::Value) -> Request<Body> {
        Request::builder()
            .uri(format!("/api/{uri}"))
            .method("PUT")
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {token}"))
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap()
    }

    pub(crate) fn put_without_auth(uri: &str, body: &serde_json::Value) -> Request<Body> {
        Request::builder()
            .uri(format!("/api/{uri}"))
            .method("PUT")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap()
    }

    pub(crate) fn delete(token: &str, uri: &str) -> Request<Body> {
        Request::builder()
            .uri(format!("/api/{uri}"))
            .method("DELETE")
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap()
    }

    pub(crate) fn delete_without_auth(uri: &str) -> Request<Body> {
        Request::builder()
            .uri(format!("/api/{uri}"))
            .method("DELETE")
            .header("content-type", "application/json")
            .body(Body::empty())
            .unwrap()
    }

    /// create user and log in
    pub(crate) async fn make_authed_user(
        username: &str,
        app: &RouterIntoService<Body>,
        email_service: Arc<MockEmailService>,
    ) -> String {
        use tower::ServiceExt;
        let password = "23rjklBFKNBdskjlfbsekf s23";
        let email = format!("{username}@example.com");
        let user_body = serde_json::json!({
            "username": username,
            "email": email,
            "password": password,
        });

        // make user
        let resp = app
            .clone()
            .oneshot(post_without_auth("users", user_body).await)
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);

        println!("made user {username}");

        // verify email
        let sent_emails = email_service.get_sent_emails();
        let email_lowercase = email.to_lowercase();
        let verification_email = sent_emails.iter().find(|e| e.to == email_lowercase && e.email_type == crate::email::EmailType::Verification)
            .unwrap_or_else(|| panic!("No verification email found for {email_lowercase}. Sent emails: {sent_emails:?}"));

        let user_id = verification_email.user_id;
        let verify_body = serde_json::json!({
            "token": verification_email.token,
            "email": email_lowercase,
        });
        let path = format!("verify/{user_id}");
        let resp = app
            .clone()
            .oneshot(post_without_auth(&path, verify_body).await)
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);

        // log in
        let login_body = serde_json::json!({
            "email": format!("{username}@example.com"),
            "password": password,
        });

        let resp = app
            .clone()
            .oneshot(post_without_auth("sessions", login_body).await)
            .await
            .unwrap();

        let body = crate::tests::response_to_value(resp.into_body()).await;

        body.get("token").unwrap().as_str().unwrap().to_string()
    }

    pub(crate) async fn print_response_body(response: axum::response::Response<Body>) {
        let body_bytes = axum::body::to_bytes(response.into_body(), 10_000)
            .await
            .unwrap();
        let body_string = String::from_utf8_lossy(&body_bytes);
        println!("{body_string}");
    }

    pub(crate) async fn assert_response_status(
        response: axum::response::Response<Body>,
        expected_status: StatusCode,
    ) -> axum::response::Response<Body> {
        let status = response.status();
        if status != expected_status {
            
            print_response_body(response).await;
            panic!("Expected status {expected_status}, got {status}");
        }
        response
    }
}
