use axum::{
    Json,
    response::{IntoResponse, Response},
};
use std::collections::HashMap;

use crate::{
    err::{AppResult, unauthorized_no_session},
    model::{
        contribution_stats::{ContributionStatsRepository, ContributionsSearch},
        language_families::{
            CreateLanguageFamily, LanguageFamily, LanguageFamilyInner, LanguageFamilyRepository,
        },
        language_family_members::LanguageFamilyMemberRepository,
        users::User,
    },
    pagination::PaginatedResponse,
    util::{
        AppState,
        extract_session::Session,
        graph_svg::{self, LanguageFamilyMemberLabel},
    },
};

pub fn create_router() -> axum::Router<AppState> {
    axum::Router::new()
        .route(
            "/language-families",
            axum::routing::post(create_language_family),
        )
        .route(
            "/language-families/{code}",
            axum::routing::get(get_language_family),
        )
        .route(
            "/language-families",
            axum::routing::get(search_language_families),
        )
        .route(
            "/language-families/{code}/like",
            axum::routing::post(like_language_family),
        )
        .route(
            "/language-families/{code}/unlike",
            axum::routing::post(unlike_language_family),
        )
        .route(
            "/language-families/{code}/tree.svg",
            axum::routing::get(get_family_tree_svg),
        )
        .route(
            "/language-families/{code}/contributors",
            axum::routing::get(get_family_contributors),
        )
}

type ApiResponse<T> = AppResult<T>;
type PaginatedApiResponse<T> = AppResult<Json<PaginatedResponse<T>>>;

#[derive(serde::Serialize)]
pub struct LikeLanguageFamilyResponse {
    pub liked: bool,
    pub like_count: i64,
}

pub async fn create_language_family(
    s: Session,
    language_families: LanguageFamilyRepository,
    Json(create): Json<CreateLanguageFamily>,
) -> ApiResponse<Json<LanguageFamily>> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    let family = language_families.create(requestor.clone(), create).await?;

    Ok(Json(family))
}

pub async fn get_language_family(
    language_families: LanguageFamilyRepository,
    axum::extract::Path(code): axum::extract::Path<String>,
) -> ApiResponse<Json<LanguageFamily>> {
    let family = language_families.find_by_code(&code).await?;

    Ok(Json(family))
}

pub async fn search_language_families(
    language_families: LanguageFamilyRepository,
    pagination: crate::pagination::PaginatedRequest,
    axum::extract::Query(query): axum::extract::Query<
        crate::model::language_families::SearchLanguageFamilies,
    >,
) -> PaginatedApiResponse<LanguageFamily> {
    let families = language_families.search(query, pagination).await?;

    Ok(Json(families))
}

pub async fn like_language_family(
    s: Session,
    language_families: LanguageFamilyRepository,
    axum::extract::Path(code): axum::extract::Path<String>,
) -> ApiResponse<Json<LikeLanguageFamilyResponse>> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    let family = language_families.find_by_code(&code).await?;

    let like_count = language_families
        .like_language_family(family.id, requestor.id)
        .await?;
    let response = LikeLanguageFamilyResponse {
        liked: true,
        like_count: like_count.unwrap_or(family.like_count),
    };
    Ok(Json(response))
}

pub async fn unlike_language_family(
    s: Session,
    language_families: LanguageFamilyRepository,
    axum::extract::Path(code): axum::extract::Path<String>,
) -> ApiResponse<Json<LikeLanguageFamilyResponse>> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    let family = language_families.find_by_code(&code).await?;

    let like_count = language_families
        .unlike_language_family(family.id, requestor.id)
        .await?;
    let response = LikeLanguageFamilyResponse {
        liked: false,
        like_count: like_count.unwrap_or(family.like_count),
    };
    Ok(Json(response))
}

pub async fn get_family_contributors(
    contribution_stats: ContributionStatsRepository,
    axum::extract::Path(code): axum::extract::Path<String>,
    axum::extract::Query(query): axum::extract::Query<ContributionsSearch>,
    pagination: crate::pagination::PaginatedRequest,
) -> PaginatedApiResponse<User> {
    let contributors = contribution_stats
        .search_top_contributors_for_family(&code, &query, &pagination)
        .await?;

    Ok(Json(contributors))
}

pub async fn get_family_tree_svg(
    language_families: LanguageFamilyRepository,
    family_members: LanguageFamilyMemberRepository,
    axum::extract::Path(code): axum::extract::Path<String>,
) -> AppResult<Response> {
    let family = language_families.find_by_code(&code).await?;
    let schema = family.tree_schema()?;

    let LanguageFamilyInner::V1(v1_schema) = schema;

    // Build member labels map by looking up each member's language
    let mut member_labels: HashMap<uuid::Uuid, LanguageFamilyMemberLabel> = HashMap::new();

    // Collect all unique member IDs from edges
    let mut member_ids = std::collections::HashSet::new();
    for edge in &v1_schema.edges {
        if let Some(parent_id) = edge.parent_member_id {
            member_ids.insert(parent_id);
        }
        member_ids.insert(edge.child_member_id);
    }

    // Fetch each member and get its language name
    for member_id in member_ids {
        if let Ok(member) = family_members.find_by_id(member_id).await {
            if let Ok(materialized) = family_members.materialize(member).await {
                use crate::model::language_family_members::LanguageFamilyMember;
                let label = match &materialized.member {
                    LanguageFamilyMember::Language(_) => {
                        if let Some(lang) = materialized.language {
                            LanguageFamilyMemberLabel::Language {
                                name: lang.name,
                                code: lang.code,
                            }
                        } else {
                            LanguageFamilyMemberLabel::Grouping {
                                notes: "(unknown language)".to_string(),
                            }
                        }
                    }
                    LanguageFamilyMember::Grouping(g) => LanguageFamilyMemberLabel::Grouping {
                        notes: g.title.clone(),
                    },
                };
                member_labels.insert(member_id, label);
            }
        }
    }

    let svg = graph_svg::language_family_to_svg(&family.code, &v1_schema, &member_labels)?;

    Ok(([(axum::http::header::CONTENT_TYPE, "image/svg+xml")], svg).into_response())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use reqwest::StatusCode;
    use serde_json::{Value, json};
    use tower::Service;

    use crate::{
        controller::api::tests::{get, post, print_response_body},
        email::MockEmailService,
        model::{
            languages::{CreateLanguage, Language, LanguageRepository},
            user_tags::UserTagRepository,
            users::{User, UserRepository},
        },
        tests::{make_authed_user, random_code, random_name},
        util::AppState,
    };

    #[allow(dead_code, unused_variables)]
    struct TestContext {
        languages: Vec<Language>,
        regular_user_1: User,
        regular_user_1_token: String,
        regular_user_2: User,
        regular_user_2_token: String,
        mod_user: User,
        mod_user_token: String,
        admin_user: User,
        admin_user_token: String,
        app: axum::routing::RouterIntoService<axum::body::Body>,
    }

    async fn create_test_context() -> TestContext {
        async fn make_user_for_context(
            app: &axum::routing::RouterIntoService<axum::body::Body>,
            state: AppState,
            email_service: Arc<MockEmailService>,
            tag: &str,
        ) -> (User, String) {
            let username = random_name();
            let token = make_authed_user(&username, app, email_service.clone()).await;
            let users = UserRepository::new(state.clone());
            let user = users.find_by_username(&username).await.unwrap();
            let tags = UserTagRepository::new(state.clone());
            tags.create_unchecked(user.id, tag.to_string(), true)
                .await
                .unwrap();
            (user, token)
        }

        let email_service = Arc::new(MockEmailService::new());
        let email_service_trait: Arc<dyn crate::email::EmailService> = email_service.clone();
        let (app, app_state) =
            crate::tests::test_app_with_email_service_state(&email_service_trait)
                .await
                .unwrap();

        let (regular_user_1, regular_user_1_token) =
            make_user_for_context(&app, app_state.clone(), email_service.clone(), "regular").await;
        let (regular_user_2, regular_user_2_token) =
            make_user_for_context(&app, app_state.clone(), email_service.clone(), "regular").await;
        let (mod_user, mod_user_token) =
            make_user_for_context(&app, app_state.clone(), email_service.clone(), "moderator")
                .await;
        let (admin_user, admin_user_token) =
            make_user_for_context(&app, app_state.clone(), email_service.clone(), "admin").await;

        let mut languages = Vec::new();
        let languages_repo = LanguageRepository::new(app_state.clone());
        for i in 1..5 {
            let lang_code = random_code();
            let language = languages_repo
                .create(
                    &regular_user_1,
                    CreateLanguage {
                        code: lang_code.clone(),
                        name: format!("Language {}", i),
                        private: false,
                        description: format!("Description for language {}", i),
                    },
                )
                .await
                .unwrap();
            languages.push(language);
        }

        TestContext {
            languages,
            regular_user_1,
            regular_user_1_token,
            regular_user_2,
            regular_user_2_token,
            mod_user,
            mod_user_token,
            admin_user,
            admin_user_token,
            app,
        }
    }

    pub async fn create_test_language_family(
        app: &mut axum::routing::RouterIntoService<axum::body::Body>,
        token: &str,
    ) -> Value {
        let create = json!({
            "code": random_code(),
            "name": "Test Language Family",
            "description": "A description for the test language family."
        });

        let request = post(token, "language-families", create).await;

        let response = app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        crate::tests::response_to_value(response.into_body()).await
    }

    #[tokio::test]
    async fn test_create_language_family() {
        let mut context = create_test_context().await;
        let family = create_test_language_family(&mut context.app, &context.admin_user_token).await;

        assert!(!family["code"].as_str().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_get_language_family() {
        let mut context = create_test_context().await;
        let family = create_test_language_family(&mut context.app, &context.admin_user_token).await;
        let code = family["code"].as_str().unwrap();

        let request = get(&format!("language-families/{}", code)).await;

        let response = context.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let fetched_family = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(fetched_family["code"], code);
    }

    #[tokio::test]
    async fn test_search_language_families() {
        let mut context = create_test_context().await;
        let family = create_test_language_family(&mut context.app, &context.admin_user_token).await;
        let code = family["code"].as_str().unwrap();

        let request = get(&format!("language-families?q={code}&limit=10&offset=0")).await;

        let response = context.app.call(request).await.unwrap();
        if response.status() != StatusCode::OK {
            print_response_body(response).await;
            panic!("Failed to search language families");
        }
        assert_eq!(response.status(), StatusCode::OK);

        let search_result = crate::tests::response_to_value(response.into_body()).await;
        let items = search_result["items"].as_array().unwrap();
        assert!(items.iter().any(|item| item["code"] == code));
    }

    #[tokio::test]
    async fn test_get_family_contributors_returns_owner() {
        let mut context = create_test_context().await;
        let family = create_test_language_family(&mut context.app, &context.admin_user_token).await;
        let code = family["code"].as_str().unwrap();

        // the owner should appear as a contributor (has permissions on the family)
        let request = get(&format!(
            "language-families/{code}/contributors?limit=10&offset=0"
        ))
        .await;

        let response = context.app.call(request).await.unwrap();
        if response.status() != StatusCode::OK {
            print_response_body(response).await;
            panic!("Failed to get family contributors");
        }
        assert_eq!(response.status(), StatusCode::OK);

        let result = crate::tests::response_to_value(response.into_body()).await;
        let items = result["items"].as_array().unwrap();

        // owner should be in the list
        assert!(
            items
                .iter()
                .any(|item| item["username"] == context.admin_user.username)
        );
    }

    #[tokio::test]
    async fn test_get_family_contributors_filters_by_search() {
        let mut context = create_test_context().await;
        let family = create_test_language_family(&mut context.app, &context.admin_user_token).await;
        let code = family["code"].as_str().unwrap();

        // search for the admin user by name
        let admin_name = &context.admin_user.username;
        let request = get(&format!(
            "language-families/{code}/contributors?q={admin_name}&limit=10&offset=0"
        ))
        .await;

        let response = context.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let result = crate::tests::response_to_value(response.into_body()).await;
        let items = result["items"].as_array().unwrap();

        // should find the admin
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["username"], admin_name.as_str());
    }

    #[tokio::test]
    async fn test_get_family_contributors_empty_search_returns_empty() {
        let mut context = create_test_context().await;
        let family = create_test_language_family(&mut context.app, &context.admin_user_token).await;
        let code = family["code"].as_str().unwrap();

        // search for a name that doesn't exist
        let request = get(&format!(
            "language-families/{code}/contributors?q=nonexistentuser12345&limit=10&offset=0"
        ))
        .await;

        let response = context.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let result = crate::tests::response_to_value(response.into_body()).await;
        let items = result["items"].as_array().unwrap();

        assert!(items.is_empty());
    }
}
