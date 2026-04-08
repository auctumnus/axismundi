use crate::{
    err::{AppResult, unauthorized_no_session},
    model::{
        languages::LanguageRepository,
        phonology_tables::{
            Body, CreatePhonologyTable, PhonologyTable, PhonologyTableRepository,
            SearchPhonologyTable, UpdatePhonologyTable,
        },
    },
    pagination::{PaginatedRequest, PaginatedResponse},
    util::extract_session::Session,
};
use axum::{Json, extract::Path, http::StatusCode};
use serde::Deserialize;
use uuid::Uuid;

pub fn create_router() -> axum::Router<crate::util::AppState> {
    axum::Router::new()
        .route(
            "/languages/{code}/phonology-tables",
            axum::routing::post(create_phonology_table).get(list_phonology_tables),
        )
        .route(
            "/languages/{code}/phonology-tables/swap",
            axum::routing::post(swap_phonology_tables),
        )
        .route(
            "/languages/{code}/phonology-tables/{id}",
            axum::routing::get(get_phonology_table)
                .put(edit_phonology_table)
                .delete(delete_phonology_table),
        )
}

type ApiResponse<T> = AppResult<T>;
type PaginatedApiResponse<T> = AppResult<PaginatedResponse<T>>;

#[derive(Debug, Deserialize)]
pub(super) struct CreatePhonologyTableRequest {
    name: String,
    description: Option<String>,
    body: Body,
}

#[derive(Debug, Deserialize)]
pub(super) struct SwapRequest {
    id1: Uuid,
    id2: Uuid,
}

pub async fn create_phonology_table(
    s: Session,
    languages: LanguageRepository,
    tables: PhonologyTableRepository,
    Path(code): Path<String>,
    Json(req): Json<CreatePhonologyTableRequest>,
) -> ApiResponse<Json<PhonologyTable>> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    let language = languages.find_by_code(&code).await?;

    tables
        .create(
            requestor,
            CreatePhonologyTable {
                language_id: language.id,
                name: req.name,
                description: req.description,
                body: req.body,
            },
        )
        .await
        .map(Json)
}

pub async fn list_phonology_tables(
    languages: LanguageRepository,
    tables: PhonologyTableRepository,
    Path(code): Path<String>,
    pagination: PaginatedRequest,
    axum::extract::Query(query): axum::extract::Query<SearchPhonologyTable>,
) -> PaginatedApiResponse<PhonologyTable> {
    let language = languages.find_by_code(&code).await?;

    tables.search(&language, pagination, query).await
}

pub async fn get_phonology_table(
    languages: LanguageRepository,
    tables: PhonologyTableRepository,
    Path((code, id)): Path<(String, Uuid)>,
) -> ApiResponse<Json<PhonologyTable>> {
    let language = languages.find_by_code(&code).await?;
    tables.get(&language, id).await.map(Json)
}

pub async fn edit_phonology_table(
    s: Session,
    tables: PhonologyTableRepository,
    Path((_code, id)): Path<(String, Uuid)>,
    Json(updates): Json<UpdatePhonologyTable>,
) -> ApiResponse<Json<PhonologyTable>> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    tables.update(requestor, id, updates).await.map(Json)
}

pub async fn delete_phonology_table(
    s: Session,
    tables: PhonologyTableRepository,
    Path((_code, id)): Path<(String, Uuid)>,
) -> ApiResponse<StatusCode> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    tables.delete(requestor, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn swap_phonology_tables(
    s: Session,
    languages: LanguageRepository,
    tables: PhonologyTableRepository,
    Path(code): Path<String>,
    Json(req): Json<SwapRequest>,
) -> ApiResponse<StatusCode> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    let language = languages.find_by_code(&code).await?;
    tables
        .swap(requestor, language.id, req.id1, req.id2)
        .await?;
    Ok(StatusCode::NO_CONTENT)
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
            make_authed_user(&format!("phown_{suffix}"), &app, email_service.clone()).await;
        let other_token =
            make_authed_user(&format!("photh_{suffix}"), &app, email_service.clone()).await;

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

    fn valid_body() -> serde_json::Value {
        json!({
            "columns": [
                {"type": "Individual", "heading": "Bilabial"},
                {"type": "Individual", "heading": "Alveolar"}
            ],
            "rows": [
                {"type": "Individual",
                    "heading": "Plosive",
                    "cells": [
                        {"phonemes": [{"text": "p", "annotations": []}]},
                        {"phonemes": [{"text": "t", "annotations": []}]}
                    ]
                }
            ],
            "annotations": []
        })
    }

    async fn create_test_phonology_table(
        app: &mut axum::routing::RouterIntoService<axum::body::Body>,
        token: &str,
        language_code: &str,
    ) -> serde_json::Value {
        let body = json!({
            "name": crate::tests::random_name(),
            "body": valid_body(),
        });
        let request = post(
            token,
            &format!("languages/{language_code}/phonology-tables"),
            body,
        )
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        crate::tests::response_to_value(response.into_body()).await
    }

    // ── Create ──────────────────────────────────────────────

    #[tokio::test]
    async fn test_create_phonology_table() {
        let mut ctx = create_test_context().await;

        let body = json!({
            "name": "Consonants",
            "body": valid_body(),
        });
        let request = post(
            &ctx.owner_token,
            &format!("languages/{}/phonology-tables", ctx.language_code),
            body,
        )
        .await;
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let value = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(value["name"], "Consonants");
        assert!(value["id"].is_string());
        assert!(value["body"].is_object());
    }

    #[tokio::test]
    async fn test_create_phonology_table_unauthorized() {
        let mut ctx = create_test_context().await;

        let body = json!({
            "name": "Consonants",
            "body": valid_body(),
        });
        let request = post_without_auth(
            &format!("languages/{}/phonology-tables", ctx.language_code),
            body,
        )
        .await;
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_create_phonology_table_forbidden() {
        let mut ctx = create_test_context().await;

        let body = json!({
            "name": "Consonants",
            "body": valid_body(),
        });
        let request = post(
            &ctx.other_token,
            &format!("languages/{}/phonology-tables", ctx.language_code),
            body,
        )
        .await;
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_create_phonology_table_invalid_body() {
        let mut ctx = create_test_context().await;

        // 2 columns but only 1 cell in the row — should fail validation
        let body = json!({
            "name": "Bad Table",
            "body": {
                "columns": [
                    {"type": "Individual", "heading": "Bilabial"},
                    {"type": "Individual", "heading": "Alveolar"}
                ],
                "rows": [
                    {"type": "Individual",
                        "heading": "Plosive",
                        "cells": [
                            {"phonemes": [{"text": "p", "annotations": []}]}
                        ]
                    }
                ],
                "annotations": []
            }
        });
        let request = post(
            &ctx.owner_token,
            &format!("languages/{}/phonology-tables", ctx.language_code),
            body,
        )
        .await;
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    // ── Get / List ──────────────────────────────────────────

    #[tokio::test]
    async fn test_get_phonology_table() {
        let mut ctx = create_test_context().await;
        let table =
            create_test_phonology_table(&mut ctx.app, &ctx.owner_token, &ctx.language_code).await;
        let id = table["id"].as_str().unwrap();

        let request = get(&format!(
            "languages/{}/phonology-tables/{id}",
            ctx.language_code
        ))
        .await;
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let value = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(value["id"], table["id"]);
        assert_eq!(value["name"], table["name"]);
    }

    #[tokio::test]
    async fn test_list_phonology_tables() {
        let mut ctx = create_test_context().await;
        create_test_phonology_table(&mut ctx.app, &ctx.owner_token, &ctx.language_code).await;
        create_test_phonology_table(&mut ctx.app, &ctx.owner_token, &ctx.language_code).await;

        let request = get(&format!("languages/{}/phonology-tables", ctx.language_code)).await;
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let value = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(value["items"].as_array().unwrap().len(), 2);
    }

    // ── Update ──────────────────────────────────────────────

    #[tokio::test]
    async fn test_edit_phonology_table() {
        let mut ctx = create_test_context().await;
        let table =
            create_test_phonology_table(&mut ctx.app, &ctx.owner_token, &ctx.language_code).await;
        let id = table["id"].as_str().unwrap();

        let update_body = json!({"name": "Renamed Table"});
        let request = put(
            &ctx.owner_token,
            &format!("languages/{}/phonology-tables/{id}", ctx.language_code),
            &update_body,
        );
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let value = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(value["name"], "Renamed Table");
    }

    #[tokio::test]
    async fn test_edit_phonology_table_unauthorized() {
        let mut ctx = create_test_context().await;
        let table =
            create_test_phonology_table(&mut ctx.app, &ctx.owner_token, &ctx.language_code).await;
        let id = table["id"].as_str().unwrap();

        let update_body = json!({"name": "Renamed Table"});
        let request = put_without_auth(
            &format!("languages/{}/phonology-tables/{id}", ctx.language_code),
            &update_body,
        );
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_edit_phonology_table_forbidden() {
        let mut ctx = create_test_context().await;
        let table =
            create_test_phonology_table(&mut ctx.app, &ctx.owner_token, &ctx.language_code).await;
        let id = table["id"].as_str().unwrap();

        let update_body = json!({"name": "Renamed Table"});
        let request = put(
            &ctx.other_token,
            &format!("languages/{}/phonology-tables/{id}", ctx.language_code),
            &update_body,
        );
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    // ── Delete ──────────────────────────────────────────────

    #[tokio::test]
    async fn test_delete_phonology_table() {
        let mut ctx = create_test_context().await;
        let table =
            create_test_phonology_table(&mut ctx.app, &ctx.owner_token, &ctx.language_code).await;
        let id = table["id"].as_str().unwrap();

        let request = delete(
            &ctx.owner_token,
            &format!("languages/{}/phonology-tables/{id}", ctx.language_code),
        );
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn test_delete_phonology_table_unauthorized() {
        let mut ctx = create_test_context().await;
        let table =
            create_test_phonology_table(&mut ctx.app, &ctx.owner_token, &ctx.language_code).await;
        let id = table["id"].as_str().unwrap();

        let request = delete_without_auth(&format!(
            "languages/{}/phonology-tables/{id}",
            ctx.language_code
        ));
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_delete_positions_reorder() {
        let mut ctx = create_test_context().await;
        let t0 =
            create_test_phonology_table(&mut ctx.app, &ctx.owner_token, &ctx.language_code).await;
        let t1 =
            create_test_phonology_table(&mut ctx.app, &ctx.owner_token, &ctx.language_code).await;
        let t2 =
            create_test_phonology_table(&mut ctx.app, &ctx.owner_token, &ctx.language_code).await;

        assert_eq!(t0["position"], 0);
        assert_eq!(t1["position"], 1);
        assert_eq!(t2["position"], 2);

        // delete the middle one
        let id1 = t1["id"].as_str().unwrap();
        let request = delete(
            &ctx.owner_token,
            &format!("languages/{}/phonology-tables/{id1}", ctx.language_code),
        );
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        // list and verify positions are 0 and 1
        let request = get(&format!("languages/{}/phonology-tables", ctx.language_code)).await;
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let value = crate::tests::response_to_value(response.into_body()).await;
        let items = value["items"].as_array().unwrap();
        assert_eq!(items.len(), 2);

        let mut positions: Vec<i64> = items
            .iter()
            .map(|i| i["position"].as_i64().unwrap())
            .collect();
        positions.sort();
        assert_eq!(positions, vec![0, 1]);
    }

    // ── Swap ────────────────────────────────────────────────

    #[tokio::test]
    async fn test_swap_phonology_tables() {
        let mut ctx = create_test_context().await;
        let t0 =
            create_test_phonology_table(&mut ctx.app, &ctx.owner_token, &ctx.language_code).await;
        let t1 =
            create_test_phonology_table(&mut ctx.app, &ctx.owner_token, &ctx.language_code).await;

        assert_eq!(t0["position"], 0);
        assert_eq!(t1["position"], 1);

        let swap_body = json!({
            "id1": t0["id"],
            "id2": t1["id"],
        });
        let request = post(
            &ctx.owner_token,
            &format!("languages/{}/phonology-tables/swap", ctx.language_code),
            swap_body,
        )
        .await;
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        // verify positions swapped by fetching each
        let id0 = t0["id"].as_str().unwrap();
        let request = get(&format!(
            "languages/{}/phonology-tables/{id0}",
            ctx.language_code
        ))
        .await;
        let response = ctx.app.call(request).await.unwrap();
        let fetched0 = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(fetched0["position"], 1);

        let id1 = t1["id"].as_str().unwrap();
        let request = get(&format!(
            "languages/{}/phonology-tables/{id1}",
            ctx.language_code
        ))
        .await;
        let response = ctx.app.call(request).await.unwrap();
        let fetched1 = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(fetched1["position"], 0);
    }

    #[tokio::test]
    async fn test_swap_phonology_tables_unauthorized() {
        let mut ctx = create_test_context().await;
        let t0 =
            create_test_phonology_table(&mut ctx.app, &ctx.owner_token, &ctx.language_code).await;
        let t1 =
            create_test_phonology_table(&mut ctx.app, &ctx.owner_token, &ctx.language_code).await;

        let swap_body = json!({
            "id1": t0["id"],
            "id2": t1["id"],
        });
        let request = post_without_auth(
            &format!("languages/{}/phonology-tables/swap", ctx.language_code),
            swap_body,
        )
        .await;
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
