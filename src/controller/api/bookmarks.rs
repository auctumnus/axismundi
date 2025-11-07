use crate::{
    err::AppResult,
    model::bookmarks::{BookmarkRepository, LinkType},
    util::AppState,
};
use axum::{
    extract::Path,
    response::Redirect,
};

pub fn create_router() -> axum::Router<crate::util::AppState> {
    axum::Router::new().route(
        "/bookmarks/{slug}",
        axum::routing::get(get_bookmark),
    )
}

async fn get_bookmark(
    Path(slug): Path<String>,
    axum::extract::State(state): axum::extract::State<AppState>,
) -> AppResult<Redirect> {
    let bookmarks = BookmarkRepository::new(state);
    let bookmark = bookmarks.get_by_slug(&slug).await?;
    let to = bookmarks.resolve_bookmark(bookmark.item, bookmark.resource, LinkType::Api).await?;
    Ok(Redirect::temporary(&to))
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use serde_json::json;
    use std::sync::Arc;
    use tower::Service;

    use crate::controller::api::tests::{
        get, make_authed_user, post, put
    };
    use crate::email::MockEmailService;

    #[tokio::test]
    async fn test_get_bookmark_user() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let username = crate::tests::random_name();
        let token = make_authed_user(&username, &app, email_service.clone()).await;

        let user_request = get(&format!("users/{username}")).await;
        let user_response = app.call(user_request).await.unwrap();

        assert_eq!(user_response.status(), StatusCode::OK);
        let body = crate::tests::response_to_value(user_response.into_body()).await;

        assert_eq!(body["username"], username);
        assert!(body["bookmark"].is_string());

        println!("Bookmark body: {body}");

        let bookmark = body["bookmark"].as_str().unwrap();

        let change_username_request = put(
            &token,
            &format!("users/{username}"),
            &json!({
                "username": format!("{username}_new")
            }),
        );
        let change_username_response = app.call(change_username_request).await.unwrap();
        assert_eq!(change_username_response.status(), StatusCode::OK);

        let get_bookmark_request = get(&format!("bookmarks/{bookmark}")).await;
        let get_bookmark_response = app.call(get_bookmark_request).await.unwrap();

        assert_eq!(get_bookmark_response.status(), StatusCode::TEMPORARY_REDIRECT);
        let location = get_bookmark_response
            .headers()
            .get("location")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(location.ends_with(&format!("users/{username}_new")));
    }

    #[tokio::test]
    async fn test_get_bookmark_language() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let username = crate::tests::random_name();
        let token = make_authed_user(&username, &app, email_service.clone()).await;

        let code = crate::tests::random_code();
        let body = json!({
            "code": code,
            "name": "Test Language",
        });

        let request = post(&token, "languages", body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = crate::tests::response_to_value(response.into_body()).await;
        assert!(body["bookmark"].is_string());
        let bookmark = body["bookmark"].as_str().unwrap().to_string();

        let new_code = crate::tests::random_code();

        let update_body = json!({
            "code": new_code,
        });

        let request =
            crate::controller::api::tests::put(&token, &format!("languages/{code}"), &update_body);
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let get_bookmark_request = get(&format!("bookmarks/{bookmark}")).await;
        let get_bookmark_response = app.call(get_bookmark_request).await.unwrap();

        assert_eq!(get_bookmark_response.status(), StatusCode::TEMPORARY_REDIRECT);
        let location = get_bookmark_response
            .headers()
            .get("location")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(location.ends_with(&format!("languages/{new_code}")));
    }

    #[tokio::test]
    async fn test_get_bookmark_word() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let username = crate::tests::random_name();
        let token = make_authed_user(&username, &app, email_service.clone()).await;

        let lang_code = crate::tests::random_code();
        let body = json!({
            "code": lang_code,
            "name": "Test Language",
        });

        let request = post(&token, "languages", body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = json!({
            "abbreviation": "n",
            "name": "Noun",
        });

        let request = post(&token, &format!("languages/{lang_code}/word-classes"), body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = json!({
            "word_class": "n",
            "word": "test",
            "definition": "test definition",
        });

        let request = post(&token, &format!("languages/{lang_code}/words"), body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = crate::tests::response_to_value(response.into_body()).await;
        assert!(body["bookmark"].is_string());
        let bookmark = body["bookmark"].as_str().unwrap().to_string();

        let update_body = json!({
            "word": "test_new",
        });

        let request = crate::controller::api::tests::put(
            &token,
            &format!("languages/{lang_code}/words/test/1"),
            &update_body,
        );
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let get_bookmark_request = get(&format!("bookmarks/{bookmark}")).await;
        let get_bookmark_response = app.call(get_bookmark_request).await.unwrap();

        assert_eq!(get_bookmark_response.status(), StatusCode::TEMPORARY_REDIRECT);
        let location = get_bookmark_response
            .headers()
            .get("location")
            .unwrap()
            .to_str()
            .unwrap();
        println!("Location: {location}");
        println!("Expected to end with: languages/{lang_code}/words/test_new/1");
        assert!(location.ends_with(&format!("languages/{lang_code}/words/test_new/1")));
    }

    #[tokio::test]
    async fn test_get_bookmark_word_class() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let username = crate::tests::random_name();
        let token = make_authed_user(&username, &app, email_service.clone()).await;

        let lang_code = crate::tests::random_code();
        let body = json!({
            "code": lang_code,
            "name": "Test Language",
        });

        let request = post(&token, "languages", body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = json!({
            "abbreviation": "n",
            "name": "noun",
        });

        let request = post(&token, &format!("languages/{lang_code}/word-classes"), body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = crate::tests::response_to_value(response.into_body()).await;
        assert!(body["bookmark"].is_string());
        let bookmark = body["bookmark"].as_str().unwrap().to_string();

        let update_body = json!({
            "abbreviation": "n2",
        });

        let request = crate::controller::api::tests::put(
            &token,
            &format!("languages/{lang_code}/word-classes/n"),
            &update_body,
        );
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let get_bookmark_request = get(&format!("bookmarks/{bookmark}")).await;
        let get_bookmark_response = app.call(get_bookmark_request).await.unwrap();

        assert_eq!(get_bookmark_response.status(), StatusCode::TEMPORARY_REDIRECT);
        let location = get_bookmark_response
            .headers()
            .get("location")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(location.ends_with(&format!("languages/{lang_code}/word-classes/n2")));
    }

}
