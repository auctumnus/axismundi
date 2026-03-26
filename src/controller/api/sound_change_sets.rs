use crate::{
    err::{AppResult, internal_error, unauthorized_no_session}, lexurgy::{self, send_scv1}, model::{
        languages::LanguageRepository,
        sound_change_sets::{
            NewSoundChangeSet, SearchSoundChangeSets, SoundChangeSet,
            SoundChangeSetRepository, UpdateSoundChangeSet,
        },
    }, pagination::{PaginatedRequest, PaginatedResponse}, util::extract_session::Session
};
use axum::{Json, extract::Path, http::StatusCode};
use serde::Deserialize;
use uuid::Uuid;

pub fn create_router() -> axum::Router<crate::util::AppState> {
    axum::Router::new()
        .route(
            "/languages/{code}/sound-change-sets",
            axum::routing::post(create_sound_change_set).get(list_sound_change_sets),
        )
        .route(
            "/languages/{code}/sound-change-sets/{id}",
            axum::routing::get(get_sound_change_set)
                .put(edit_sound_change_set)
                .delete(delete_sound_change_set),
        )
        .route(
            "/languages/{code}/sound-change-sets/{id}/run",
            axum::routing::post(run_sound_change_set_from_db),
        )
        .route(
            "/sound-change-sets/run",
            axum::routing::post(run_sound_change_set),
        )
}

type ApiResponse<T> = AppResult<T>;
type PaginatedApiResponse<T> = AppResult<PaginatedResponse<T>>;

#[derive(Debug, Deserialize)]
pub struct RunSoundChangeSetRequest {
    input_words: Vec<String>,
}

pub async fn create_sound_change_set(
    s: Session,
    languages: LanguageRepository,
    sets: SoundChangeSetRepository,
    Path(code): Path<String>,
    Json(req): Json<NewSoundChangeSet>,
) -> ApiResponse<Json<SoundChangeSet>> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    let language = languages.find_by_code(&code).await?;

    sets.create(requestor, &language, req).await.map(Json)
}

pub async fn list_sound_change_sets(
    languages: LanguageRepository,
    sets: SoundChangeSetRepository,
    Path(code): Path<String>,
    pagination: PaginatedRequest,
    axum::extract::Query(query): axum::extract::Query<SearchSoundChangeSets>,
) -> PaginatedApiResponse<SoundChangeSet> {
    let language = languages.find_by_code(&code).await?;

    sets.search(&language, pagination, query).await
}

pub async fn get_sound_change_set(
    languages: LanguageRepository,
    sets: SoundChangeSetRepository,
    Path((code, id)): Path<(String, Uuid)>,
) -> ApiResponse<Json<SoundChangeSet>> {
    let _language = languages.find_by_code(&code).await?;
    let set = sets.get(id).await?;

    match set {
        Some(set) => Ok(Json(set)),
        None => Err(crate::err::not_found(format!(
            "sound change set with id {id}"
        ))),
    }
}

pub async fn edit_sound_change_set(
    s: Session,
    sets: SoundChangeSetRepository,
    Path((_code, id)): Path<(String, Uuid)>,
    Json(updates): Json<UpdateSoundChangeSet>,
) -> ApiResponse<Json<SoundChangeSet>> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    sets.update(requestor, &id, updates).await.map(Json)
}

pub async fn delete_sound_change_set(
    s: Session,
    sets: SoundChangeSetRepository,
    Path((_code, id)): Path<(String, Uuid)>,
) -> ApiResponse<StatusCode> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    sets.delete(requestor, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn run_sound_change_set_from_db(
    sets: SoundChangeSetRepository,
    Path((_code, id)): Path<(String, Uuid)>,
    Json(req): Json<RunSoundChangeSetRequest>,
) -> ApiResponse<Json<crate::lexurgy::Response>> {
    let response = sets.run_from_db(&id, req.input_words).await?;
    Ok(Json(response))
}

pub async fn run_sound_change_set(
    Json(req): Json<lexurgy::Request>,
) -> ApiResponse<Json<crate::lexurgy::Response>> {
    let response = send_scv1(&req).await?;
    match response {
        Ok(response) => Ok(Json(response)),
        Err(err) => Err(err.into()),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use reqwest::StatusCode;
    use serde_json::json;
    use tower::Service;

    use crate::controller::api::tests::{
        create_test_language, delete, delete_without_auth, get, make_authed_user, post,
        post_without_auth, put, put_without_auth,
    };
    use crate::email::MockEmailService;

    struct TestContext {
        owner_token: String,
        other_token: String,
        app: axum::routing::RouterIntoService<axum::body::Body>,
        language_code: String,
    }

    async fn create_test_context() -> TestContext {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let (app, _state) = crate::tests::test_app_with_email_service_state(&email_service_trait)
            .await
            .unwrap();

        let suffix = crate::tests::random_code();
        let owner_token =
            make_authed_user(&format!("scown_{suffix}"), &app, email_service.clone()).await;
        let other_token =
            make_authed_user(&format!("scoth_{suffix}"), &app, email_service.clone()).await;

        let mut app = app;
        let lang = create_test_language(&owner_token, &mut app).await;
        let language_code = lang["code"].as_str().unwrap().to_string();

        TestContext {
            owner_token,
            other_token,
            app,
            language_code,
        }
    }

    async fn create_test_sound_change_set(
        app: &mut axum::routing::RouterIntoService<axum::body::Body>,
        token: &str,
        language_code: &str,
    ) -> serde_json::Value {
        let body = json!({
            "name": crate::tests::random_name(),
            "changes": "rule:\n  a => b",
        });
        let request = post(
            token,
            &format!("languages/{language_code}/sound-change-sets"),
            body,
        )
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        crate::tests::response_to_value(response.into_body()).await
    }

    // ── Create ──────────────────────────────────────────────

    #[tokio::test]
    async fn test_create_sound_change_set() {
        let mut ctx = create_test_context().await;

        let body = json!({
            "name": "Great Vowel Shift",
            "description": "A major vowel change",
            "changes": "rule:\n  a => e",
        });
        let request = post(
            &ctx.owner_token,
            &format!("languages/{}/sound-change-sets", ctx.language_code),
            body,
        )
        .await;
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let value = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(value["name"], "Great Vowel Shift");
        assert_eq!(value["description"], "A major vowel change");
        assert_eq!(value["changes"], "rule:\n  a => e");
        assert!(value["id"].is_string());
    }

    #[tokio::test]
    async fn test_create_sound_change_set_unauthorized() {
        let mut ctx = create_test_context().await;

        let body = json!({
            "name": "Great Vowel Shift",
            "changes": "rule:\n  a => e",
        });
        let request = post_without_auth(
            &format!("languages/{}/sound-change-sets", ctx.language_code),
            body,
        )
        .await;
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_create_sound_change_set_forbidden() {
        let mut ctx = create_test_context().await;

        let body = json!({
            "name": "Great Vowel Shift",
            "changes": "rule:\n  a => e",
        });
        let request = post(
            &ctx.other_token,
            &format!("languages/{}/sound-change-sets", ctx.language_code),
            body,
        )
        .await;
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    // ── Get / List ──────────────────────────────────────────

    #[tokio::test]
    async fn test_get_sound_change_set() {
        let mut ctx = create_test_context().await;
        let set =
            create_test_sound_change_set(&mut ctx.app, &ctx.owner_token, &ctx.language_code).await;
        let id = set["id"].as_str().unwrap();

        let request = get(&format!(
            "languages/{}/sound-change-sets/{id}",
            ctx.language_code
        ))
        .await;
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let value = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(value["id"], set["id"]);
        assert_eq!(value["name"], set["name"]);
    }

    #[tokio::test]
    async fn test_list_sound_change_sets() {
        let mut ctx = create_test_context().await;
        create_test_sound_change_set(&mut ctx.app, &ctx.owner_token, &ctx.language_code).await;
        create_test_sound_change_set(&mut ctx.app, &ctx.owner_token, &ctx.language_code).await;

        let request = get(&format!(
            "languages/{}/sound-change-sets",
            ctx.language_code
        ))
        .await;
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let value = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(value["items"].as_array().unwrap().len(), 2);
    }

    // ── Update ──────────────────────────────────────────────

    #[tokio::test]
    async fn test_edit_sound_change_set() {
        let mut ctx = create_test_context().await;
        let set =
            create_test_sound_change_set(&mut ctx.app, &ctx.owner_token, &ctx.language_code).await;
        let id = set["id"].as_str().unwrap();

        let update_body = json!({"name": "Renamed Set"});
        let request = put(
            &ctx.owner_token,
            &format!("languages/{}/sound-change-sets/{id}", ctx.language_code),
            &update_body,
        );
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let value = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(value["name"], "Renamed Set");
    }

    #[tokio::test]
    async fn test_edit_sound_change_set_unauthorized() {
        let mut ctx = create_test_context().await;
        let set =
            create_test_sound_change_set(&mut ctx.app, &ctx.owner_token, &ctx.language_code).await;
        let id = set["id"].as_str().unwrap();

        let update_body = json!({"name": "Renamed Set"});
        let request = put_without_auth(
            &format!("languages/{}/sound-change-sets/{id}", ctx.language_code),
            &update_body,
        );
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_edit_sound_change_set_forbidden() {
        let mut ctx = create_test_context().await;
        let set =
            create_test_sound_change_set(&mut ctx.app, &ctx.owner_token, &ctx.language_code).await;
        let id = set["id"].as_str().unwrap();

        let update_body = json!({"name": "Renamed Set"});
        let request = put(
            &ctx.other_token,
            &format!("languages/{}/sound-change-sets/{id}", ctx.language_code),
            &update_body,
        );
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    // ── Delete ──────────────────────────────────────────────

    #[tokio::test]
    async fn test_delete_sound_change_set() {
        let mut ctx = create_test_context().await;
        let set =
            create_test_sound_change_set(&mut ctx.app, &ctx.owner_token, &ctx.language_code).await;
        let id = set["id"].as_str().unwrap();

        let request = delete(
            &ctx.owner_token,
            &format!("languages/{}/sound-change-sets/{id}", ctx.language_code),
        );
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn test_delete_sound_change_set_unauthorized() {
        let mut ctx = create_test_context().await;
        let set =
            create_test_sound_change_set(&mut ctx.app, &ctx.owner_token, &ctx.language_code).await;
        let id = set["id"].as_str().unwrap();

        let request = delete_without_auth(&format!(
            "languages/{}/sound-change-sets/{id}",
            ctx.language_code
        ));
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_run_sound_change_set() {
        let mut ctx = create_test_context().await;
        let set =
            create_test_sound_change_set(&mut ctx.app, &ctx.owner_token, &ctx.language_code).await;
        let id = set["id"].as_str().unwrap();

        let input_words = vec!["cat".to_string(), "bat".to_string()];
        let request = post(
            &ctx.owner_token,
            &format!("languages/{}/sound-change-sets/{id}/run", ctx.language_code),
            json!({ "input_words": input_words }),
        ).await;
        let response = ctx.app.call(request).await.unwrap();
        if response.status() != StatusCode::OK {
            let status = response.status();
            let error_value: String = str::from_utf8(axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap().as_ref()).unwrap().to_string();
            panic!("Expected 200 OK, got {}: {:#?}", status, error_value);
        }
        assert_eq!(response.status(), StatusCode::OK);

        let value = crate::tests::response_to_value(response.into_body()).await;
        assert!(value["outputWords"].is_array());
    }
}
