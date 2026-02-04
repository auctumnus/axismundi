use axum::Router;
#[cfg(not(test))]
use governor::middleware::NoOpMiddleware;

use crate::util::AppState;
#[cfg(not(test))]
use std::sync::Arc;
#[cfg(not(test))]
use tower_governor::governor::GovernorConfig;

mod audit_logs;
mod bookmarks;
mod definitions;
mod language_invites;
mod language_families;
mod language_family_invites;
mod language_family_members;
mod language_family_permissions;
mod language_permissions;
mod languages;
mod quotation_suggestions;
mod quotations;
mod reports;
mod sessions;
mod translatable;
mod translations;
mod user_activities;
mod user_bans;
mod user_tags;
mod users;
mod word_classes;
mod word_relations;
mod words;

// pretty sure i need that there, actually...
#[allow(clippy::needless_return)]
pub fn create_api_controller() -> Router<AppState> {
    let (secure_user_routes, normal_user_routes) = users::create_users_router();
    let (secure_user_tag_routes, normal_user_tag_routes) = user_tags::create_router();
    let (secure_user_ban_routes, normal_user_ban_routes) = user_bans::create_router();
    let (secure_report_routes, normal_report_routes) = reports::create_router();
    let (secure_audit_log_routes, normal_audit_log_routes) = audit_logs::create_router();

    let secure_routes = Router::<AppState>::new()
        .merge(sessions::create_router())
        .merge(secure_user_routes)
        .merge(secure_user_tag_routes)
        .merge(secure_user_ban_routes)
        .merge(secure_report_routes)
        .merge(secure_audit_log_routes);

    let normal_routes = Router::<AppState>::new()
        .merge(normal_user_routes)
        .merge(normal_user_tag_routes)
        .merge(normal_user_ban_routes)
        .merge(normal_report_routes)
        .merge(normal_audit_log_routes)
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
        .merge(user_activities::create_router())
        .merge(language_families::create_router())
        .merge(language_family_members::create_router())
        .merge(language_family_invites::create_router())
        .merge(language_family_permissions::create_router());

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
pub(crate) mod tests {
    use axum::{body::Body, http::Request, routing::RouterIntoService};
    use reqwest::StatusCode;
    use serde_json::json;
    use std::sync::Arc;
    use tower::Service as _;

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
    pub async fn make_authed_user(
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
        if resp.status() != 200 {
            crate::controller::api::tests::print_response_body(resp).await;
            panic!("Failed to create user {username}");
        }
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

    pub(crate) async fn create_test_language(
        token: &str,
        app: &mut axum::routing::RouterIntoService<axum::body::Body>,
    ) -> serde_json::Value {
        let code = crate::tests::random_code();
        let body = json!({
            "code": code,
            "name": "Test Language",
        });
        let request = post(token, "languages", body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        crate::tests::response_to_value(response.into_body()).await
    }

    pub(crate) async fn create_test_word(
        token: &str,
        app: &mut axum::routing::RouterIntoService<axum::body::Body>,
        language_code: &str,
    ) -> serde_json::Value {
        let body = json!({
            "word": crate::tests::random_name(),
            "word_class": "n",
        });
        let request = post(token, &format!("languages/{language_code}/words"), body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        crate::tests::response_to_value(response.into_body()).await
    }

    pub(crate) async fn create_test_definition(
        token: &str,
        app: &mut axum::routing::RouterIntoService<axum::body::Body>,
        language_code: &str,
        word_slug: &str,
        word_lemma: i64,
    ) -> serde_json::Value {
        let body = json!({
            "definition": "A test definition",
        });
        let request = post(
            token,
            &format!("languages/{language_code}/words/{word_slug}/{word_lemma}/definitions"),
            body,
        )
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        crate::tests::response_to_value(response.into_body()).await
    }

    pub(crate) async fn create_test_translatable(
        token: &str,
        app: &mut axum::routing::RouterIntoService<axum::body::Body>,
        _language_code: &str,
    ) -> serde_json::Value {
        let body = json!({
            "title": "A test translatable",
            "english": "test"
        });
        let request = post(token, "translatable", body).await;
        let response = app.call(request).await.unwrap();
        if response.status() != StatusCode::OK {
            print_response_body(response).await;
            panic!("Failed to create test translatable");
        }
        assert_eq!(response.status(), StatusCode::OK);
        crate::tests::response_to_value(response.into_body()).await
    }

    pub(crate) async fn create_test_translation(
        token: &str,
        app: &mut axum::routing::RouterIntoService<axum::body::Body>,
        translatable_id: &str,
        language_code: &str,
    ) -> serde_json::Value {
        let body = json!({
            "translated_text": "A test translation",
        });
        let request = post(
            token,
            &format!("translatable/{translatable_id}/translations/{language_code}"),
            body,
        )
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        crate::tests::response_to_value(response.into_body()).await
    }
}
