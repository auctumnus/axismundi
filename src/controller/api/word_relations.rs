use crate::{
    err::{AppResult, not_found, unauthorized_no_session},
    model::{
        languages::LanguageRepository,
        word_relations::{
            CognacyFull, CreateWordRelation, SearchWordRelations, WordRelation,
            WordRelationRepository, WordRelationSearchResult, WordRelationType,
        },
        words::WordRepository,
    },
    pagination::{PaginatedRequest, PaginatedResponse},
    util::extract_session::Session,
};
use axum::{
    Json,
    extract::Path,
    http::StatusCode,
    routing::{delete, get, post},
};
use serde::Deserialize;

pub fn create_router() -> axum::Router<crate::util::AppState> {
    axum::Router::new()
        .route("/languages/{code}/words/{slug}/{lemma}/relations", post(add_relation))
        .route("/languages/{code}/words/{slug}/{lemma}/relations", get(search_relations))
        .route("/languages/{code}/words/{slug}/{lemma}/relations/{related_code}/{related_slug}/{related_lemma}", delete(delete_relation))
        .route("/languages/{code}/words/{slug}/{lemma}/etymology", get(get_etymology))
}
type ApiResponse<T> = AppResult<T>;
type PaginatedApiResponse<T> = AppResult<PaginatedResponse<T>>;
#[derive(Deserialize)]
pub struct AddRelationRequest {
    kind: WordRelationType,
    language: String,
    slug: String,
    lemma: i32,
}

pub async fn add_relation(
    s: Session,
    languages: LanguageRepository,
    words: WordRepository,
    word_relations: WordRelationRepository,
    Path((code, slug, lemma)): Path<(String, String, i32)>,
    Json(req): Json<AddRelationRequest>,
) -> ApiResponse<Json<WordRelation>> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    let language = languages.find_by_code(&code).await?;
    let antecedent = words
        .find_by_slug_and_lemma(Some(requestor), language.id, &slug, lemma)
        .await?;
    let consequent_language = languages.find_by_code(&req.language).await?;
    let consequent = words
        .find_by_slug_and_lemma(
            Some(requestor),
            consequent_language.id,
            &req.slug,
            req.lemma,
        )
        .await?;

    let word_relation = word_relations
        .create(
            requestor,
            CreateWordRelation {
                antecedent,
                consequent,
                kind: req.kind,
            },
        )
        .await?;

    Ok(Json(word_relation))
}

pub async fn delete_relation(
    s: Session,
    languages: LanguageRepository,
    words: WordRepository,
    word_relations: WordRelationRepository,
    Path((code, slug, lemma, related_code, related_slug, related_lemma)): Path<(
        String,
        String,
        i32,
        String,
        String,
        i32,
    )>,
) -> ApiResponse<StatusCode> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    let language = languages.find_by_code(&code).await?;
    let antecedent = words
        .find_by_slug_and_lemma(Some(requestor), language.id, &slug, lemma)
        .await?;
    let consequent_language = languages.find_by_code(&related_code).await?;
    let consequent = words
        .find_by_slug_and_lemma(
            Some(requestor),
            consequent_language.id,
            &related_slug,
            related_lemma,
        )
        .await?;

    word_relations
        .delete(requestor, &antecedent, &consequent)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn search_relations(
    s: Session,
    languages: LanguageRepository,
    words: WordRepository,
    word_relations: WordRelationRepository,
    Path((code, slug, lemma)): Path<(String, String, i32)>,
    pagination: PaginatedRequest,
    axum::extract::Query(search): axum::extract::Query<SearchWordRelations>,
) -> PaginatedApiResponse<WordRelationSearchResult> {
    let language = languages.find_by_code(&code).await?;
    let word = words
        .find_by_slug_and_lemma(s.user(), language.id, &slug, lemma)
        .await?;

    word_relations.search(pagination, search, &word).await
}

pub async fn get_etymology(
    s: Session,
    languages: LanguageRepository,
    words: WordRepository,
    word_relations: WordRelationRepository,
    Path((code, slug, lemma)): Path<(String, String, i32)>,
) -> ApiResponse<Json<CognacyFull>> {
    let language = languages.find_by_code(&code).await?;
    let word = words
        .find_by_slug_and_lemma(s.user(), language.id, &slug, lemma)
        .await?;

    let cognacy = word_relations.get_cognacy(&word).await?;

    match cognacy {
        Some(cognacy) => Ok(Json(cognacy)),
        None => Err(not_found("cognacy graph")),
    }
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::StatusCode;
    use axum::routing::RouterIntoService;
    use serde_json::json;
    use std::sync::Arc;
    use tower::Service;

    use crate::controller::api::tests::{
        assert_response_status, get, make_authed_user, post, post_without_auth,
    };
    use crate::email::MockEmailService;

    async fn setup_language_with_words(
        token: &str,
        app: &mut RouterIntoService<Body>,
    ) -> (String, String, String) {
        let lang_code = crate::tests::random_code();
        let body = json!({
            "code": lang_code,
            "name": "Test Language",
        });

        let request = post(token, "languages", body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // Create first word
        let body = json!({
            "slug": "test1",
            "word_class": "n",
            "word": "test1",
        });

        let request = post(token, &format!("languages/{lang_code}/words"), body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // Create second word
        let body = json!({
            "slug": "test2",
            "word_class": "n",
            "word": "test2",
        });

        let request = post(token, &format!("languages/{lang_code}/words"), body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        (lang_code, "test1".to_string(), "test2".to_string())
    }

    #[tokio::test]
    async fn test_add_relation() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let username = crate::tests::random_name();
        let token = make_authed_user(&username, &app, email_service.clone()).await;

        let (lang_code, slug1, slug2) = setup_language_with_words(&token, &mut app).await;

        let body = json!({
            "kind": "borrowed",
            "language": lang_code,
            "slug": slug2,
            "lemma": 1,
        });

        let request = post(
            &token,
            &format!("languages/{lang_code}/words/{slug1}/1/relations"),
            body,
        )
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(body["kind"], "borrowed");
    }

    #[tokio::test]
    async fn test_add_relation_unauthorized() {
        let mut app = crate::tests::test_app().await.unwrap();

        let body = json!({
            "kind": "borrowed",
            "language": "test",
            "slug": "test2",
            "lemma": 1,
        });

        let request = post_without_auth("languages/test/words/test1/1/relations", body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_search_relations() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let username = crate::tests::random_name();
        let token = make_authed_user(&username, &app, email_service.clone()).await;

        let (lang_code, slug1, slug2) = setup_language_with_words(&token, &mut app).await;

        // Add a relation
        let body = json!({
            "kind": "borrowed",
            "language": lang_code,
            "slug": slug2,
            "lemma": 1,
        });

        let request = post(
            &token,
            &format!("languages/{lang_code}/words/{slug1}/1/relations"),
            body,
        )
        .await;
        let response = app.call(request).await.unwrap();
        assert_response_status(response, StatusCode::OK).await;

        // Search for relations
        let request = get(&format!("languages/{lang_code}/words/{slug1}/1/relations")).await;
        let response = app.call(request).await.unwrap();
        let response = assert_response_status(response, StatusCode::OK).await;

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert!(body["items"].is_array());
        assert_eq!(body["items"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_search_relations_with_kind_filter() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let username = crate::tests::random_name();
        let token = make_authed_user(&username, &app, email_service.clone()).await;

        let (lang_code, slug1, slug2) = setup_language_with_words(&token, &mut app).await;

        // Add a borrowed relation
        let body = json!({
            "kind": "borrowed",
            "language": lang_code,
            "slug": slug2,
            "lemma": 1,
        });

        let request = post(
            &token,
            &format!("languages/{lang_code}/words/{slug1}/1/relations"),
            body,
        )
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // Search for borrowed relations
        let request = get(&format!(
            "languages/{lang_code}/words/{slug1}/1/relations?kind=borrowed"
        ))
        .await;
        let response = app.call(request).await.unwrap();
        let response = assert_response_status(response, StatusCode::OK).await;

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert!(body["items"].is_array());
        let items = body["items"].as_array().unwrap();
        println!("Items: {:#?}", items);
        assert!(
            items
                .iter()
                .all(|item| item["relation"]["kind"] == "borrowed")
        );
    }

    #[tokio::test]
    async fn test_get_etymology() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let username = crate::tests::random_name();
        let token = make_authed_user(&username, &app, email_service.clone()).await;

        let (lang_code, slug1, slug2) = setup_language_with_words(&token, &mut app).await;

        // Add a borrowed relation to create a cognacy graph
        let body = json!({
            "kind": "borrowed",
            "language": lang_code,
            "slug": slug2,
            "lemma": 1,
        });

        let request = post(
            &token,
            &format!("languages/{lang_code}/words/{slug1}/1/relations"),
            body,
        )
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // Get the etymology/cognacy graph
        let request = get(&format!("languages/{lang_code}/words/{slug1}/1/etymology")).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;

        // Check that we have a cognacy object with edges and words
        assert!(body["cognacy"].is_object());
        assert!(body["cognacy"]["inner"]["V1"]["edges"].is_array());
        assert_eq!(
            body["cognacy"]["inner"]["V1"]["edges"]
                .as_array()
                .unwrap()
                .len(),
            1
        );

        // Check that we have both words in the words map
        assert!(body["words"].is_object());
        let words_map = body["words"].as_object().unwrap();
        assert_eq!(words_map.len(), 2);
    }

    #[tokio::test]
    async fn test_get_etymology_no_relations() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let username = crate::tests::random_name();
        let token = make_authed_user(&username, &app, email_service.clone()).await;

        let (lang_code, slug1, _slug2) = setup_language_with_words(&token, &mut app).await;

        // Try to get etymology for a word with no relations
        let request = get(&format!("languages/{lang_code}/words/{slug1}/1/etymology")).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_delete_relation() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let username = crate::tests::random_name();
        let token = make_authed_user(&username, &app, email_service.clone()).await;

        let (lang_code, slug1, slug2) = setup_language_with_words(&token, &mut app).await;

        // Add a relation
        let body = json!({
            "kind": "borrowed",
            "language": lang_code,
            "slug": slug2,
            "lemma": 1,
        });

        let request = post(
            &token,
            &format!("languages/{lang_code}/words/{slug1}/1/relations"),
            body,
        )
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // Delete the relation
        let request = crate::controller::api::tests::delete(
            &token,
            &format!("languages/{lang_code}/words/{slug1}/1/relations/{lang_code}/{slug2}/1"),
        );
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        // Verify the relation is gone by searching
        let request = get(&format!("languages/{lang_code}/words/{slug1}/1/relations")).await;
        let response = app.call(request).await.unwrap();
        let body = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(body["items"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn test_delete_relation_splits_cognacy() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let username = crate::tests::random_name();
        let token = make_authed_user(&username, &app, email_service.clone()).await;

        let (lang_code, slug1, slug2) = setup_language_with_words(&token, &mut app).await;

        // Create a third word
        let body = json!({
            "slug": "test3",
            "word_class": "n",
            "word": "test3",
            "definition": "test definition 3",
        });

        let request = post(&token, &format!("languages/{lang_code}/words"), body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // Create a chain: test1 -> test2 -> test3
        let body = json!({
            "kind": "borrowed",
            "language": lang_code,
            "slug": slug2,
            "lemma": 1,
        });

        let request = post(
            &token,
            &format!("languages/{lang_code}/words/{slug1}/1/relations"),
            body,
        )
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = json!({
            "kind": "borrowed",
            "language": lang_code,
            "slug": "test3",
            "lemma": 1,
        });

        let request = post(
            &token,
            &format!("languages/{lang_code}/words/{slug2}/1/relations"),
            body,
        )
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // Delete the middle edge (test2 -> test3), splitting the graph
        let request = crate::controller::api::tests::delete(
            &token,
            &format!("languages/{lang_code}/words/{slug2}/1/relations/{lang_code}/test3/1"),
        );
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        // Verify test1 still has etymology (just with test2)
        let request = get(&format!("languages/{lang_code}/words/{slug1}/1/etymology")).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(body["words"].as_object().unwrap().len(), 2); // test1 and test2

        // Verify test3 no longer has etymology
        let request = get(&format!("languages/{lang_code}/words/test3/1/etymology")).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND); // no cognacy graph
    }

    #[tokio::test]
    async fn test_cyclic_relation_fails() {
        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let mut app = crate::tests::test_app_with_email_service(&email_service_trait)
            .await
            .unwrap();

        let username = crate::tests::random_name();
        let token = make_authed_user(&username, &app, email_service.clone()).await;

        let (lang_code, slug1, slug2) = setup_language_with_words(&token, &mut app).await;

        // Create a third word
        let body = json!({
            "slug": "test3",
            "word_class": "n",
            "word": "test3",
            "definition": "test definition 3",
        });

        let request = post(&token, &format!("languages/{lang_code}/words"), body).await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // Create a chain: test1 -> test2 -> test3
        let body = json!({
            "kind": "borrowed",
            "language": lang_code,
            "slug": slug2,
            "lemma": 1,
        });

        let request = post(
            &token,
            &format!("languages/{lang_code}/words/{slug1}/1/relations"),
            body,
        )
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = json!({
            "kind": "borrowed",
            "language": lang_code,
            "slug": "test3",
            "lemma": 1,
        });

        let request = post(
            &token,
            &format!("languages/{lang_code}/words/{slug2}/1/relations"),
            body,
        )
        .await;
        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // Try to create a cycle by adding test3 -> test1
        let body = json!({
            "kind": "borrowed",
            "language": lang_code,
            "slug": slug1,
            "lemma": 1,
        });

        let request = post(
            &token,
            &format!("languages/{lang_code}/words/test3/1/relations"),
            body,
        )
        .await;
        let response = app.call(request).await.unwrap();

        // Should fail with bad request because it would create a cycle
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}
