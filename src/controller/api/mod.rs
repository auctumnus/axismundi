use axum::{
    Router,
    routing::{delete, get, post, put},
};

use crate::util::AppState;

#[cfg(not(test))]
use governor::middleware::NoOpMiddleware;
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

// pretty sure i need that there, actually...
#[allow(clippy::needless_return)]
pub fn create_api_controller() -> Router<AppState> {
    let secure_routes = Router::<AppState>::new()
        // users
        .route("/users", post(users::create_user))
        .route("/users/{id}/verify", post(users::verify_user))
        .route(
            "/users/{username}/profile-picture",
            put(users::upload_profile_picture),
        )
        // sessions
        .route("/sessions", post(sessions::login))
        .route("/sessions", get(sessions::get_sessions))
        // languages
        .route("/languages", post(languages::create_language))
        .route("/languages/{code}", put(languages::edit_language))
        .route("/languages/{code}", delete(languages::delete_language))
        // language permissions
        .route(
            "/languages/{code}/permissions/{username}",
            put(language_permissions::edit_user_permissions),
        )
        .route(
            "/languages/{code}/permissions/{username}",
            delete(language_permissions::delete_user_permissions),
        )
        // language invites
        .route(
            "/languages/{code}/invites/{username}",
            post(language_invites::invite_user_to_language),
        )
        .route(
            "/languages/{code}/invites/{username}",
            delete(language_invites::delete_language_invite),
        )
        .route(
            "/languages/{code}/accept-invite",
            post(language_invites::accept_language_invite),
        )
        // word classes
        .route(
            "/languages/{code}/word-classes",
            post(word_classes::create_word_class),
        )
        .route(
            "/languages/{code}/word-classes/{abbreviation}",
            put(word_classes::edit_word_class),
        )
        .route(
            "/languages/{code}/word-classes/{abbreviation}",
            delete(word_classes::delete_word_class),
        )
        // words
        .route("/languages/{code}/words", post(words::create_word))
        .route("/languages/{code}/words/{slug}", put(words::edit_word))
        .route("/languages/{code}/words/{slug}", delete(words::delete_word));

    let normal_routes = Router::<AppState>::new()
        // users
        .route("/users/{username}", get(users::get_user))
        .route("/users", get(users::search_users))
        // languages
        .route("/languages/{code}", get(languages::get_language))
        .route("/languages", get(languages::list_languages))
        .route(
            "/languages/{code}/owner",
            get(languages::get_language_owner),
        )
        .route(
            "/languages/{code}/editors",
            get(languages::get_language_editors),
        )
        .route(
            "/languages/{code}/permissions",
            get(language_permissions::get_language_permissions),
        )
        .route(
            "/languages/{code}/permissions/{username}",
            get(language_permissions::get_user_language_permissions),
        )
        // word classes
        .route(
            "/languages/{code}/word-classes",
            get(word_classes::list_word_classes),
        )
        .route(
            "/languages/{code}/word-classes/{abbreviation}",
            get(word_classes::get_word_class),
        )
        // words
        .route("/languages/{code}/words", get(words::list_words));

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
    use std::sync::Arc;

    use crate::email::tests::MockEmailService;

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

        // verify email
        let sent_emails = email_service.get_sent_emails();
        let email_lowercase = email.to_lowercase();
        let verification_email = sent_emails.iter().find(|e| e.to == email_lowercase && e.email_type == crate::email::tests::EmailType::Verification)
            .unwrap_or_else(|| panic!("No verification email found for {email_lowercase}. Sent emails: {sent_emails:?}"));

        let user_id = verification_email.user_id;
        let verify_body = serde_json::json!({
            "token": verification_email.token,
            "email": email_lowercase,
        });
        let path = format!("users/{user_id}/verify");
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
}
