use axum::{Json, extract::Path, http::StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use tokio::time::{Duration, Instant, timeout_at};
use uuid::Uuid;

use crate::{
    err::{AppResult, unauthorized_no_session},
    model::{
        grammar_tables::{
            CreateGrammarTable, GrammarTable, GrammarTableRepository, UpdateGrammarTable,
        },
        language_permissions::LanguagePermissionRepository,
        languages::LanguageRepository,
        sound_change_sets::SoundChangeSetRepository,
        user_bans::UserBanRepository,
        words::WordRepository,
    },
    util::extract_session::Session,
};

pub fn create_router() -> axum::Router<crate::util::AppState> {
    axum::Router::new()
        .route(
            "/languages/{code}/grammar-tables",
            axum::routing::post(create).get(list),
        )
        .route(
            "/languages/{code}/grammar-tables/swap",
            axum::routing::post(swap),
        )
        .route(
            "/languages/{code}/grammar-tables/{id}",
            axum::routing::get(get).put(update).delete(delete),
        )
        .route(
            "/languages/{code}/grammar-tables/preview",
            axum::routing::post(preview),
        )
        .route(
            "/languages/{code}/words/{slug}/{lemma}/grammar-tables",
            axum::routing::get(render_for_word),
        )
        .route(
            "/languages/{code}/words/{slug}/{lemma}/grammar-tables/{id}",
            axum::routing::get(render_one_for_word),
        )
}

#[derive(Debug, Deserialize)]
struct GrammarPreviewRequest {
    input: String,
    #[serde(default)]
    ipa: Option<String>,
    #[serde(default)]
    extra: Option<JsonValue>,
    #[serde(default)]
    preamble: String,
    #[serde(default)]
    changes: String,
}

#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum GrammarPreviewResponse {
    Rendered {
        value: String,
        /// IPA is estimated from the inflected preview value, after the table
        /// rules have run.
        ipa: Option<String>,
    },
    TimedOut,
    Error {
        message: String,
    },
}

async fn preview(
    session: Session,
    languages: LanguageRepository,
    permissions: LanguagePermissionRepository,
    bans: UserBanRepository,
    sets: SoundChangeSetRepository,
    Path(code): Path<String>,
    Json(request): Json<GrammarPreviewRequest>,
) -> AppResult<Json<GrammarPreviewResponse>> {
    let Some(user) = session.user() else {
        return Err(unauthorized_no_session());
    };
    bans.ensure_not_banned(user.id).await?;
    let language = languages.find_by_code(&code).await?;
    let can_edit = permissions
        .has_permission(
            user.id,
            language.id,
            crate::model::language_invites::PermissionLevel::Editor,
        )
        .await?;
    if !can_edit {
        return Err(crate::err::forbidden(
            "You do not have permission to preview grammar tables for this language.",
        ));
    }
    if request.input.len() > 16 * 1024
        || request.preamble.len() > 32 * 1024
        || request.changes.len() > 16 * 1024
    {
        return Err(crate::err::bad_request(
            "The preview input or rules are too long.",
        ));
    }
    let deadline =
        Instant::now() + Duration::from_millis(crate::config::CONFIG.grammar.table_budget_ms);
    let changes =
        crate::model::grammar_tables::compose_changes(&request.preamble, &request.changes);
    let ipa_estimator = languages
        .get_ipa_estimator(language.id)
        .await?
        .map(|set| set.id);
    let spelling = request.input;
    let placeholders = crate::placeholders::Placeholders::for_spelling(&spelling)
        .with_ipa(request.ipa.as_deref())
        .with_extra(request.extra.as_ref());
    let response = match crate::grammar::GrammarEvaluator::default()
        .preview(&spelling, &placeholders, changes, deadline)
        .await
    {
        Ok(value) => match crate::grammar::estimate_ipa(
            &sets,
            ipa_estimator,
            vec![value.clone()],
            &placeholders,
            deadline,
        )
        .await
        {
            Ok(ipa) => GrammarPreviewResponse::Rendered {
                value,
                ipa: ipa.and_then(|mut words| words.pop()),
            },
            Err(error) => preview_error(error),
        },
        Err(error) => preview_error(error),
    };
    Ok(Json(response))
}

fn preview_error(error: crate::grammar::GrammarRenderError) -> GrammarPreviewResponse {
    match error {
        crate::grammar::GrammarRenderError::TimedOut => GrammarPreviewResponse::TimedOut,
        crate::grammar::GrammarRenderError::Failed(message) => {
            GrammarPreviewResponse::Error { message }
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum GrammarRenderResult {
    Rendered {
        table_id: Uuid,
        html: String,
    },
    TimedOut {
        table_id: Uuid,
        full_page_url: String,
    },
    Error {
        table_id: Uuid,
        message: String,
    },
}

#[derive(Debug, Serialize)]
struct GrammarCollectionResponse {
    results: Vec<GrammarRenderResult>,
    section_error: Option<String>,
}

async fn render_table(
    tables: GrammarTableRepository,
    sets: &SoundChangeSetRepository,
    ipa_estimator: Option<Uuid>,
    word: crate::model::words::Word,
    table: GrammarTable,
    deadline: Instant,
    full_page_url: String,
) -> GrammarRenderResult {
    match crate::grammar::GrammarEvaluator::default()
        .render(&tables, sets, ipa_estimator, &word, &table, deadline)
        .await
    {
        Ok(rendered) => match crate::grammar::render_html(&table, &rendered) {
            Ok(html) => GrammarRenderResult::Rendered {
                table_id: table.id,
                html,
            },
            Err(error) => GrammarRenderResult::Error {
                table_id: table.id,
                message: error.to_string(),
            },
        },
        Err(crate::grammar::GrammarRenderError::TimedOut) => GrammarRenderResult::TimedOut {
            table_id: table.id,
            full_page_url,
        },
        Err(crate::grammar::GrammarRenderError::Failed(message)) => GrammarRenderResult::Error {
            table_id: table.id,
            message,
        },
    }
}

async fn render_for_word(
    languages: LanguageRepository,
    sets: SoundChangeSetRepository,
    words: WordRepository,
    tables: GrammarTableRepository,
    Path((code, slug, lemma)): Path<(String, String, i32)>,
) -> AppResult<Json<GrammarCollectionResponse>> {
    let language = languages.find_by_code(&code).await?;
    let word = words
        .find_by_slug_and_lemma(None, language.id, &slug, lemma)
        .await?;
    let section_deadline =
        Instant::now() + Duration::from_millis(crate::config::CONFIG.grammar.section_budget_ms);
    let matching = match timeout_at(section_deadline, tables.matching_for_word(&word)).await {
        Ok(Ok(tables)) => tables,
        Ok(Err(error)) => return Err(error),
        Err(_) => {
            return Ok(Json(GrammarCollectionResponse {
                results: vec![],
                section_error: Some("grammar tables took too long to find".into()),
            }));
        }
    };
    let per_table = Duration::from_millis(crate::config::CONFIG.grammar.table_budget_ms);
    let ipa_estimator = languages
        .get_ipa_estimator(language.id)
        .await?
        .map(|set| set.id);
    let results = futures::future::join_all(matching.into_iter().map(|table| {
        let deadline = std::cmp::min(section_deadline, Instant::now() + per_table);
        let full_page_url = format!(
            "/languages/{code}/words/{slug}/{lemma}/grammar-tables/{}",
            table.id
        );
        let tables = tables.clone();
        let sets = &sets;
        let word = word.clone();
        async move {
            render_table(
                tables,
                sets,
                ipa_estimator,
                word,
                table,
                deadline,
                full_page_url,
            )
            .await
        }
    }))
    .await;
    Ok(Json(GrammarCollectionResponse {
        results,
        section_error: None,
    }))
}

async fn render_one_for_word(
    languages: LanguageRepository,
    sets: SoundChangeSetRepository,
    words: WordRepository,
    tables: GrammarTableRepository,
    Path((code, slug, lemma, id)): Path<(String, String, i32, Uuid)>,
) -> AppResult<Json<GrammarRenderResult>> {
    let language = languages.find_by_code(&code).await?;
    let word = words
        .find_by_slug_and_lemma(None, language.id, &slug, lemma)
        .await?;
    let table = tables
        .matching_table_for_word(&word, id)
        .await?
        .ok_or_else(|| crate::err::not_found("This grammar table does not apply to this word."))?;
    let deadline =
        Instant::now() + Duration::from_millis(crate::config::CONFIG.grammar.full_page_budget_ms);
    let ipa_estimator = languages
        .get_ipa_estimator(language.id)
        .await?
        .map(|set| set.id);
    Ok(Json(
        render_table(
            tables,
            &sets,
            ipa_estimator,
            word,
            table,
            deadline,
            format!("/languages/{code}/words/{slug}/{lemma}/grammar-tables/{id}"),
        )
        .await,
    ))
}

async fn create(
    session: Session,
    languages: LanguageRepository,
    tables: GrammarTableRepository,
    Path(code): Path<String>,
    Json(mut request): Json<CreateGrammarTable>,
) -> AppResult<Json<GrammarTable>> {
    let Some(user) = session.user() else {
        return Err(unauthorized_no_session());
    };
    let language = languages.find_by_code(&code).await?;
    // Language comes from the URL; accepting an arbitrary JSON language id here
    // would let a request accidentally create a table in a different language.
    request.language_id = language.id;
    tables.create(user, request).await.map(Json)
}

async fn list(
    languages: LanguageRepository,
    tables: GrammarTableRepository,
    Path(code): Path<String>,
) -> AppResult<Json<Vec<GrammarTable>>> {
    let language = languages.find_by_code(&code).await?;
    tables.list(&language).await.map(Json)
}

async fn get(
    languages: LanguageRepository,
    tables: GrammarTableRepository,
    Path((code, id)): Path<(String, Uuid)>,
) -> AppResult<Json<GrammarTable>> {
    let language = languages.find_by_code(&code).await?;
    tables.get(&language, id).await.map(Json)
}

async fn update(
    session: Session,
    languages: LanguageRepository,
    tables: GrammarTableRepository,
    Path((code, id)): Path<(String, Uuid)>,
    Json(request): Json<UpdateGrammarTable>,
) -> AppResult<Json<GrammarTable>> {
    let Some(user) = session.user() else {
        return Err(unauthorized_no_session());
    };
    let language = languages.find_by_code(&code).await?;
    // Resolve through the URL language before authorizing or mutating. This
    // prevents an ID from another language being edited through this route.
    tables.get(&language, id).await?;
    tables.update(user, id, request).await.map(Json)
}

async fn delete(
    session: Session,
    languages: LanguageRepository,
    tables: GrammarTableRepository,
    Path((code, id)): Path<(String, Uuid)>,
) -> AppResult<StatusCode> {
    let Some(user) = session.user() else {
        return Err(unauthorized_no_session());
    };
    let language = languages.find_by_code(&code).await?;
    // See `update`: table IDs are only meaningful within their language URL.
    tables.get(&language, id).await?;
    tables.delete(user, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
struct SwapRequest {
    id1: Uuid,
    id2: Uuid,
}

async fn swap(
    session: Session,
    languages: LanguageRepository,
    tables: GrammarTableRepository,
    Path(code): Path<String>,
    Json(request): Json<SwapRequest>,
) -> AppResult<StatusCode> {
    let Some(user) = session.user() else {
        return Err(unauthorized_no_session());
    };
    let language = languages.find_by_code(&code).await?;
    tables
        .swap(user, language.id, request.id1, request.id2)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use reqwest::StatusCode;
    use serde_json::json;
    use tower::Service;

    use crate::{
        controller::api::tests::{
            create_test_language, create_test_word, delete, delete_without_auth, get,
            make_authed_user, post, post_without_auth, put, put_without_auth,
        },
        email::MockEmailService,
    };

    struct TestContext {
        owner_token: String,
        other_token: String,
        app: axum::routing::RouterIntoService<axum::body::Body>,
        language_code: String,
        other_language_code: String,
    }

    async fn create_test_context() -> TestContext {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let (app, _state) = crate::tests::test_app_with_email_service_state(&email_service_trait)
            .await
            .unwrap();
        let suffix = crate::tests::random_code();
        let owner_token =
            make_authed_user(&format!("gtown_{suffix}"), &app, email_service.clone()).await;
        let other_token =
            make_authed_user(&format!("gtoth_{suffix}"), &app, email_service.clone()).await;
        let mut app = app;
        let language = create_test_language(&owner_token, &mut app).await;
        let other_language = create_test_language(&owner_token, &mut app).await;

        TestContext {
            owner_token,
            other_token,
            app,
            language_code: language["code"].as_str().unwrap().to_owned(),
            other_language_code: other_language["code"].as_str().unwrap().to_owned(),
        }
    }

    async fn default_word_class_id(
        app: &mut axum::routing::RouterIntoService<axum::body::Body>,
        language_code: &str,
    ) -> String {
        let request = get(&format!("languages/{language_code}/word-classes")).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let classes = crate::tests::response_to_value(response.into_body()).await;
        classes["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|class| class["abbreviation"] == "n")
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    fn table_request(name: &str, word_class_id: &str) -> serde_json::Value {
        json!({
            "name": name,
            "body": {
                "columns": [{"type": "Individual", "heading": "number"}],
                "rows": [{
                    "type": "Individual",
                    "heading": "singular",
                    "cells": [{"changes": ""}]
                }]
            },
            "word_class_ids": [word_class_id],
        })
    }

    async fn create_test_table(
        app: &mut axum::routing::RouterIntoService<axum::body::Body>,
        token: &str,
        language_code: &str,
        name: &str,
    ) -> serde_json::Value {
        let word_class_id = default_word_class_id(app, language_code).await;
        let request = post(
            token,
            &format!("languages/{language_code}/grammar-tables"),
            table_request(name, &word_class_id),
        )
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        crate::tests::response_to_value(response.into_body()).await
    }

    #[tokio::test]
    async fn create_requires_an_editor_and_uses_the_url_language() {
        let mut ctx = create_test_context().await;
        let class_id = default_word_class_id(&mut ctx.app, &ctx.language_code).await;
        let body = table_request("declension", &class_id);

        let request = post_without_auth(
            &format!("languages/{}/grammar-tables", ctx.language_code),
            body.clone(),
        )
        .await;
        assert_eq!(
            ctx.app.call(request).await.unwrap().status(),
            StatusCode::UNAUTHORIZED
        );

        let request = post(
            &ctx.other_token,
            &format!("languages/{}/grammar-tables", ctx.language_code),
            body.clone(),
        )
        .await;
        assert_eq!(
            ctx.app.call(request).await.unwrap().status(),
            StatusCode::FORBIDDEN
        );

        // A bogus JSON language id must not override the language in the URL.
        let mut body = body;
        body["language_id"] = json!(uuid::Uuid::new_v4());
        let request = post(
            &ctx.owner_token,
            &format!("languages/{}/grammar-tables", ctx.language_code),
            body,
        )
        .await;
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let table = crate::tests::response_to_value(response.into_body()).await;
        let id = table["id"].as_str().unwrap();

        let request = get(&format!(
            "languages/{}/grammar-tables/{id}",
            ctx.language_code
        ))
        .await;
        assert_eq!(
            ctx.app.call(request).await.unwrap().status(),
            StatusCode::OK
        );
        let request = get(&format!(
            "languages/{}/grammar-tables/{id}",
            ctx.other_language_code
        ))
        .await;
        assert_eq!(
            ctx.app.call(request).await.unwrap().status(),
            StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn update_and_delete_require_auth_and_cannot_cross_the_url_language() {
        let mut ctx = create_test_context().await;
        let table = create_test_table(
            &mut ctx.app,
            &ctx.owner_token,
            &ctx.language_code,
            "declension",
        )
        .await;
        let id = table["id"].as_str().unwrap();
        let class_id = default_word_class_id(&mut ctx.app, &ctx.language_code).await;
        let update = table_request("renamed", &class_id);

        let request = put_without_auth(
            &format!("languages/{}/grammar-tables/{id}", ctx.language_code),
            &update,
        );
        assert_eq!(
            ctx.app.call(request).await.unwrap().status(),
            StatusCode::UNAUTHORIZED
        );
        let request = put(
            &ctx.other_token,
            &format!("languages/{}/grammar-tables/{id}", ctx.language_code),
            &update,
        );
        assert_eq!(
            ctx.app.call(request).await.unwrap().status(),
            StatusCode::FORBIDDEN
        );
        let request = delete_without_auth(&format!(
            "languages/{}/grammar-tables/{id}",
            ctx.language_code
        ));
        assert_eq!(
            ctx.app.call(request).await.unwrap().status(),
            StatusCode::UNAUTHORIZED
        );
        let request = delete(
            &ctx.other_token,
            &format!("languages/{}/grammar-tables/{id}", ctx.language_code),
        );
        assert_eq!(
            ctx.app.call(request).await.unwrap().status(),
            StatusCode::FORBIDDEN
        );

        let request = put(
            &ctx.owner_token,
            &format!("languages/{}/grammar-tables/{id}", ctx.other_language_code),
            &update,
        );
        assert_eq!(
            ctx.app.call(request).await.unwrap().status(),
            StatusCode::NOT_FOUND
        );
        let request = delete(
            &ctx.owner_token,
            &format!("languages/{}/grammar-tables/{id}", ctx.other_language_code),
        );
        assert_eq!(
            ctx.app.call(request).await.unwrap().status(),
            StatusCode::NOT_FOUND
        );

        let request = get(&format!(
            "languages/{}/grammar-tables/{id}",
            ctx.language_code
        ))
        .await;
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            crate::tests::response_to_value(response.into_body()).await["name"],
            "declension"
        );
    }

    #[tokio::test]
    async fn create_rejects_duplicate_or_foreign_scope_ids() {
        let mut ctx = create_test_context().await;
        let class_id = default_word_class_id(&mut ctx.app, &ctx.language_code).await;
        let mut duplicate = table_request("duplicate scope", &class_id);
        duplicate["word_class_ids"] = json!([class_id.clone(), class_id]);
        let request = post(
            &ctx.owner_token,
            &format!("languages/{}/grammar-tables", ctx.language_code),
            duplicate,
        )
        .await;
        assert_eq!(
            ctx.app.call(request).await.unwrap().status(),
            StatusCode::BAD_REQUEST
        );

        let foreign_class = default_word_class_id(&mut ctx.app, &ctx.other_language_code).await;
        let request = post(
            &ctx.owner_token,
            &format!("languages/{}/grammar-tables", ctx.language_code),
            table_request("foreign scope", &foreign_class),
        )
        .await;
        assert_eq!(
            ctx.app.call(request).await.unwrap().status(),
            StatusCode::BAD_REQUEST
        );
    }

    #[tokio::test]
    async fn matching_tables_are_exposed_for_matching_words() {
        let mut ctx = create_test_context().await;
        let table = create_test_table(
            &mut ctx.app,
            &ctx.owner_token,
            &ctx.language_code,
            "declension",
        )
        .await;
        let word = create_test_word(&ctx.owner_token, &mut ctx.app, &ctx.language_code).await;
        let request = get(&format!(
            "languages/{}/words/{}/{}/grammar-tables",
            ctx.language_code,
            word["slug"].as_str().unwrap(),
            word["lemma"].as_i64().unwrap(),
        ))
        .await;
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let response = crate::tests::response_to_value(response.into_body()).await;
        assert!(
            response["results"]
                .as_array()
                .unwrap()
                .iter()
                .any(|result| { result["table_id"] == table["id"] })
        );
    }

    #[tokio::test]
    async fn swap_requires_auth_and_changes_the_order() {
        let mut ctx = create_test_context().await;
        let first =
            create_test_table(&mut ctx.app, &ctx.owner_token, &ctx.language_code, "first").await;
        let second =
            create_test_table(&mut ctx.app, &ctx.owner_token, &ctx.language_code, "second").await;
        let swap = json!({"id1": first["id"], "id2": second["id"]});

        let request = post_without_auth(
            &format!("languages/{}/grammar-tables/swap", ctx.language_code),
            swap.clone(),
        )
        .await;
        assert_eq!(
            ctx.app.call(request).await.unwrap().status(),
            StatusCode::UNAUTHORIZED
        );
        let request = post(
            &ctx.owner_token,
            &format!("languages/{}/grammar-tables/swap", ctx.language_code),
            swap,
        )
        .await;
        assert_eq!(
            ctx.app.call(request).await.unwrap().status(),
            StatusCode::NO_CONTENT
        );

        let request = get(&format!("languages/{}/grammar-tables", ctx.language_code)).await;
        let response = ctx.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let tables = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(tables[0]["id"], second["id"]);
        assert_eq!(tables[1]["id"], first["id"]);
    }
}
