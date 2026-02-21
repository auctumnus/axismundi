use axum::{
    Json,
    extract::{Path, Query},
    routing::{delete, get, post},
};
use reqwest::StatusCode;
use uuid::Uuid;

use crate::{
    err::{AppResult, not_found, unauthorized_no_session},
    model::{
        language_families::LanguageFamilyRepository,
        language_family_members::{
            CreateLanguageFamilyMember, LanguageFamilyMember, LanguageFamilyMemberRepository,
            MemberWithLanguages, SearchLanguageFamilyMembers,
        },
        languages::LanguageRepository,
    },
    pagination::{PaginatedRequest, PaginatedResponse},
    util::{AppState, extract_session::Session},
};

pub fn create_router() -> axum::Router<AppState> {
    axum::Router::new()
        .route(
            "/language-family/{code}/members",
            post(create_language_family_member),
        )
        .route(
            "/language-family/{code}/members/by-id/{id}/children",
            post(create_language_family_member_with_parent_id),
        )
        .route(
            "/language-family/{code}/members/by-id/{id}/children",
            get(search_language_family_members_with_parent_id),
        )
        .route(
            "/language-family/{code}/members/by-id/{id}",
            get(get_language_family_member),
        )
        .route(
            "/language-family/{code}/members/by-id/{id}",
            delete(delete_language_family_member),
        )
        .route("/language-family/{code}/root", get(find_root))
        .route(
            "/language-family/{code}/members/by-code/{code}",
            get(get_language_family_member_by_code),
        )
        .route(
            "/language-family/{code}/members/by-code/{code}",
            delete(delete_language_family_member_by_code),
        )
        .route(
            "/language-family/{code}/members/by-code/{code}/children",
            post(create_language_family_member_with_parent_code),
        )
        .route(
            "/language-family/{code}/members/by-code/{code}/children",
            get(search_language_family_members_with_parent_code),
        )
        .route(
            "/language-family/{code}/members",
            get(search_language_family_members_by_family),
        )
        .route(
            "/language-family-members",
            get(search_language_family_members),
        )
}

type ApiResponse<T> = AppResult<T>;
type PaginatedApiResponse<T> = AppResult<Json<PaginatedResponse<T>>>;

pub async fn create_language_family_member(
    s: Session,
    language_family_members: LanguageFamilyMemberRepository,
    language_families: LanguageFamilyRepository,
    Path(code): Path<String>,
    Json(create): Json<CreateLanguageFamilyMember>,
) -> ApiResponse<Json<LanguageFamilyMember>> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    let family = language_families.find_by_code(&code).await?;

    let member = language_family_members
        .create(requestor.clone(), family, None, create)
        .await?;

    Ok(Json(member))
}

pub async fn create_language_family_member_with_parent_id(
    s: Session,
    language_family_members: LanguageFamilyMemberRepository,
    language_families: LanguageFamilyRepository,
    Path((code, parent_id)): Path<(String, Uuid)>,
    Json(create): Json<CreateLanguageFamilyMember>,
) -> ApiResponse<Json<MemberWithLanguages>> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    let family = language_families.find_by_code(&code).await?;

    let member = language_family_members
        .create(requestor.clone(), family, Some(parent_id), create)
        .await?;

    let materialized = language_family_members.materialize(member).await?;

    Ok(Json(materialized))
}

pub async fn create_language_family_member_with_parent_code(
    s: Session,
    language_family_members: LanguageFamilyMemberRepository,
    language_families: LanguageFamilyRepository,
    languages: LanguageRepository,
    Path((code, parent_code)): Path<(String, String)>,
    Json(create): Json<CreateLanguageFamilyMember>,
) -> ApiResponse<Json<MemberWithLanguages>> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    let family = language_families.find_by_code(&code).await?;

    let parent_language = languages.find_by_code(&parent_code).await?;

    let Some(parent) = language_family_members
        .find_by_family_and_language(family.id, parent_language.id)
        .await?
    else {
        return Err(not_found("Parent language family member not found"));
    };

    let member = language_family_members
        .create(requestor.clone(), family, Some(parent.id), create)
        .await?;

    let materialized = language_family_members.materialize(member).await?;
    Ok(Json(materialized))
}

pub async fn get_language_family_member(
    language_family_members: LanguageFamilyMemberRepository,
    language_families: LanguageFamilyRepository,
    Path((code, member_id)): Path<(String, Uuid)>,
) -> ApiResponse<Json<MemberWithLanguages>> {
    let _family = language_families.find_by_code(&code).await?;

    let member = language_family_members.find_by_id(member_id).await?;

    let materialized = language_family_members.materialize(member).await?;
    Ok(Json(materialized))
}

pub async fn get_language_family_member_by_code(
    language_family_members: LanguageFamilyMemberRepository,
    language_families: LanguageFamilyRepository,
    languages: LanguageRepository,
    Path((code, language_code)): Path<(String, String)>,
) -> ApiResponse<Json<MemberWithLanguages>> {
    let family = language_families.find_by_code(&code).await?;

    let language = languages.find_by_code(&language_code).await?;

    let Some(member) = language_family_members
        .find_by_family_and_language(family.id, language.id)
        .await?
    else {
        return Err(not_found("Language family member not found"));
    };

    let materialized = language_family_members.materialize(member).await?;
    Ok(Json(materialized))
}

pub async fn search_language_family_members_with_parent_id(
    language_family_members: LanguageFamilyMemberRepository,
    language_families: LanguageFamilyRepository,
    Path((code, parent_id)): Path<(String, Uuid)>,
    Query(mut query): Query<SearchLanguageFamilyMembers>,
    pagination: PaginatedRequest,
) -> PaginatedApiResponse<MemberWithLanguages> {
    let _family = language_families.find_by_code(&code).await?;

    let parent = language_family_members.find_by_id(parent_id).await?;

    query.parent_member_id = Some(parent.id);

    let members = language_family_members.search(query, pagination).await?;

    let mut materialized_members = vec![];
    for member in members.items {
        let materialized = language_family_members.materialize(member).await?;
        materialized_members.push(materialized);
    }

    Ok(Json(PaginatedResponse {
        items: materialized_members,
        total: members.total,
        offset: members.offset,
        limit: members.limit,
        has_more: members.has_more,
    }))
}

pub async fn search_language_family_members_with_parent_code(
    language_family_members: LanguageFamilyMemberRepository,
    language_families: LanguageFamilyRepository,
    languages: LanguageRepository,
    Path((code, parent_code)): Path<(String, String)>,
    Query(mut query): Query<SearchLanguageFamilyMembers>,
    pagination: PaginatedRequest,
) -> PaginatedApiResponse<MemberWithLanguages> {
    let family = language_families.find_by_code(&code).await?;

    let parent_language = languages.find_by_code(&parent_code).await?;

    let Some(parent) = language_family_members
        .find_by_family_and_language(family.id, parent_language.id)
        .await?
    else {
        return Err(not_found("Parent language family member not found"));
    };

    query.parent_member_id = Some(parent.id);

    let members = language_family_members.search(query, pagination).await?;

    let mut materialized_members = vec![];
    for member in members.items {
        let materialized = language_family_members.materialize(member).await?;
        materialized_members.push(materialized);
    }

    Ok(Json(PaginatedResponse {
        items: materialized_members,
        total: members.total,
        offset: members.offset,
        limit: members.limit,
        has_more: members.has_more,
    }))
}

pub async fn delete_language_family_member(
    s: Session,
    language_family_members: LanguageFamilyMemberRepository,
    language_families: LanguageFamilyRepository,
    Path((code, member_id)): Path<(String, Uuid)>,
) -> ApiResponse<StatusCode> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    let _ = language_families.find_by_code(&code).await?;

    language_family_members.delete(requestor, member_id).await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn delete_language_family_member_by_code(
    s: Session,
    language_family_members: LanguageFamilyMemberRepository,
    language_families: LanguageFamilyRepository,
    languages: LanguageRepository,
    Path((code, language_code)): Path<(String, String)>,
) -> ApiResponse<StatusCode> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    let family = language_families.find_by_code(&code).await?;

    let parent_language = languages.find_by_code(&language_code).await?;

    let Some(member) = language_family_members
        .find_by_family_and_language(family.id, parent_language.id)
        .await?
    else {
        return Err(not_found("Language family member not found"));
    };

    language_family_members.delete(requestor, member.id).await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn find_root(
    language_family_members: LanguageFamilyMemberRepository,
    language_families: LanguageFamilyRepository,
    Path(code): Path<String>,
) -> ApiResponse<Json<MemberWithLanguages>> {
    let family = language_families.find_by_code(&code).await?;

    let Some(root_member) = language_family_members.find_root(family.id).await? else {
        return Err(not_found("Root language family member not found"));
    };

    let materialized = language_family_members.materialize(root_member).await?;
    Ok(Json(materialized))
}

pub async fn search_language_family_members_by_family(
    language_family_members: LanguageFamilyMemberRepository,
    language_families: LanguageFamilyRepository,
    Path(code): Path<String>,
    Query(query): Query<SearchLanguageFamilyMembers>,
    pagination: PaginatedRequest,
) -> PaginatedApiResponse<MemberWithLanguages> {
    let _ = language_families.find_by_code(&code).await?;

    let mut modified_query = query;
    modified_query.family_code = Some(code);

    let members = language_family_members
        .search(modified_query, pagination)
        .await?;

    let mut materialized_members = vec![];
    for member in members.items {
        let materialized = language_family_members.materialize(member).await?;
        materialized_members.push(materialized);
    }

    Ok(Json(PaginatedResponse {
        items: materialized_members,
        total: members.total,
        offset: members.offset,
        limit: members.limit,
        has_more: members.has_more,
    }))
}

pub async fn search_language_family_members(
    language_family_members: LanguageFamilyMemberRepository,
    Query(query): Query<SearchLanguageFamilyMembers>,
    pagination: PaginatedRequest,
) -> PaginatedApiResponse<MemberWithLanguages> {
    let members = language_family_members.search(query, pagination).await?;

    let mut materialized_members = vec![];
    for member in members.items {
        let materialized = language_family_members.materialize(member).await?;
        materialized_members.push(materialized);
    }

    Ok(Json(PaginatedResponse {
        items: materialized_members,
        total: members.total,
        offset: members.offset,
        limit: members.limit,
        has_more: members.has_more,
    }))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use reqwest::StatusCode;
    use serde_json::{Value, json};
    use tower::Service;

    use crate::{
        controller::api::tests::{delete, get, post, print_response_body},
        email::MockEmailService,
        model::{
            language_families::{CreateLanguageFamily, LanguageFamilyRepository},
            languages::{CreateLanguage, Language, LanguageRepository},
            user_tags::UserTagRepository,
            users::{User, UserRepository},
        },
        tests::{make_authed_user, random_code, random_name},
        util::AppState,
    };

    #[allow(dead_code)]
    struct TestContext {
        languages: Vec<Language>,
        regular_user_1: User,
        regular_user_1_token: String,
        #[allow(dead_code)]
        regular_user_2: User,
        #[allow(dead_code)]
        regular_user_2_token: String,
        #[allow(dead_code)]
        mod_user: User,
        #[allow(dead_code)]
        mod_user_token: String,
        admin_user: User,
        admin_user_token: String,
        app: axum::routing::RouterIntoService<axum::body::Body>,
        family_code: String,
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

        // create a language family for testing (admin owns the family)
        let family_code = random_code();
        let families_repo = LanguageFamilyRepository::new(app_state.clone());
        families_repo
            .create(
                admin_user.clone(),
                CreateLanguageFamily {
                    code: family_code.clone(),
                    name: "Test Family".to_string(),
                    description: "A test language family".to_string(),
                },
            )
            .await
            .unwrap();

        // admin creates languages so they have permission on both family and languages
        let mut languages = Vec::new();
        let languages_repo = LanguageRepository::new(app_state.clone());
        for i in 1..=5 {
            let lang_code = random_code();
            let language = languages_repo
                .create(
                    &admin_user,
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
            family_code,
        }
    }

    async fn create_test_member(
        app: &mut axum::routing::RouterIntoService<axum::body::Body>,
        token: &str,
        family_code: &str,
        language_code: &str,
    ) -> Value {
        let create = json!({
            "language_code": language_code,
            "relation_type": "descendant",
            "notes": "Test notes"
        });

        let request = post(
            token,
            &format!("language-family/{family_code}/members"),
            create,
        )
        .await;
        let response = app.call(request).await.unwrap();

        if response.status() != StatusCode::OK {
            print_response_body(response).await;
            panic!("Failed to create language family member");
        }

        crate::tests::response_to_value(response.into_body()).await
    }

    async fn create_test_member_with_parent_id(
        app: &mut axum::routing::RouterIntoService<axum::body::Body>,
        token: &str,
        family_code: &str,
        parent_id: &str,
        language_code: &str,
    ) -> Value {
        let create = json!({
            "language_code": language_code,
            "relation_type": "descendant",
            "notes": "Child notes"
        });

        let request = post(
            token,
            &format!("language-family/{family_code}/members/by-id/{parent_id}/children"),
            create,
        )
        .await;
        let response = app.call(request).await.unwrap();

        if response.status() != StatusCode::OK {
            print_response_body(response).await;
            panic!("Failed to create language family member with parent id");
        }

        crate::tests::response_to_value(response.into_body()).await
    }

    async fn create_test_member_with_parent_code(
        app: &mut axum::routing::RouterIntoService<axum::body::Body>,
        token: &str,
        family_code: &str,
        parent_language_code: &str,
        language_code: &str,
    ) -> Value {
        let create = json!({
            "language_code": language_code,
            "relation_type": "descendant",
            "notes": "Child notes by code"
        });

        let request = post(
            token,
            &format!(
                "language-family/{family_code}/members/by-code/{parent_language_code}/children"
            ),
            create,
        )
        .await;
        let response = app.call(request).await.unwrap();

        if response.status() != StatusCode::OK {
            print_response_body(response).await;
            panic!("Failed to create language family member with parent code");
        }

        crate::tests::response_to_value(response.into_body()).await
    }

    #[tokio::test]
    async fn test_create_language_family_member_root() {
        let mut context = create_test_context().await;
        let lang = &context.languages[0];

        let member = create_test_member(
            &mut context.app,
            &context.admin_user_token,
            &context.family_code,
            &lang.code,
        )
        .await;

        assert!(member["id"].as_str().is_some());
        assert_eq!(member["relation_type"], "descendant");
    }

    #[tokio::test]
    async fn test_create_language_family_member_with_parent_id() {
        let mut context = create_test_context().await;
        let root_lang = &context.languages[0];
        let child_lang = &context.languages[1];

        let root_member = create_test_member(
            &mut context.app,
            &context.admin_user_token,
            &context.family_code,
            &root_lang.code,
        )
        .await;
        let root_id = root_member["id"].as_str().unwrap();

        let child_member = create_test_member_with_parent_id(
            &mut context.app,
            &context.admin_user_token,
            &context.family_code,
            root_id,
            &child_lang.code,
        )
        .await;

        assert!(child_member["member"]["id"].as_str().is_some());
        assert_eq!(child_member["member"]["relation_type"], "descendant");
    }

    #[tokio::test]
    async fn test_create_language_family_member_with_parent_code() {
        let mut context = create_test_context().await;
        let root_lang = &context.languages[0];
        let child_lang = &context.languages[1];

        create_test_member(
            &mut context.app,
            &context.admin_user_token,
            &context.family_code,
            &root_lang.code,
        )
        .await;

        let child_member = create_test_member_with_parent_code(
            &mut context.app,
            &context.admin_user_token,
            &context.family_code,
            &root_lang.code,
            &child_lang.code,
        )
        .await;

        assert!(child_member["member"]["id"].as_str().is_some());
        assert_eq!(child_member["member"]["relation_type"], "descendant");
    }

    #[tokio::test]
    async fn test_get_language_family_member_by_id() {
        let mut context = create_test_context().await;
        let lang = &context.languages[0];

        let member = create_test_member(
            &mut context.app,
            &context.admin_user_token,
            &context.family_code,
            &lang.code,
        )
        .await;
        let member_id = member["id"].as_str().unwrap();

        let request = get(&format!(
            "language-family/{}/members/by-id/{}",
            context.family_code, member_id
        ))
        .await;

        let response = context.app.call(request).await.unwrap();

        if response.status() != StatusCode::OK {
            print_response_body(response).await;
            panic!("Failed to get language family member by id");
        }

        let fetched = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(fetched["member"]["id"], member_id);
    }

    #[tokio::test]
    async fn test_get_language_family_member_by_code() {
        let mut context = create_test_context().await;
        let lang = &context.languages[0];

        create_test_member(
            &mut context.app,
            &context.admin_user_token,
            &context.family_code,
            &lang.code,
        )
        .await;

        let request = get(&format!(
            "language-family/{}/members/by-code/{}",
            context.family_code, lang.code
        ))
        .await;

        let response = context.app.call(request).await.unwrap();

        if response.status() != StatusCode::OK {
            print_response_body(response).await;
            panic!("Failed to get language family member by code");
        }

        let fetched = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(fetched["language"]["code"], lang.code);
    }

    #[tokio::test]
    async fn test_find_root() {
        let mut context = create_test_context().await;
        let lang = &context.languages[0];

        let root_member = create_test_member(
            &mut context.app,
            &context.admin_user_token,
            &context.family_code,
            &lang.code,
        )
        .await;
        let root_id = root_member["id"].as_str().unwrap();

        let request = get(&format!("language-family/{}/root", context.family_code)).await;

        let response = context.app.call(request).await.unwrap();

        if response.status() != StatusCode::OK {
            print_response_body(response).await;
            panic!("Failed to find root");
        }

        let fetched = crate::tests::response_to_value(response.into_body()).await;
        assert_eq!(fetched["member"]["id"], root_id);
    }

    #[tokio::test]
    async fn test_search_children_by_parent_id() {
        let mut context = create_test_context().await;
        let root_lang = &context.languages[0];
        let child_lang = &context.languages[1];

        let root_member = create_test_member(
            &mut context.app,
            &context.admin_user_token,
            &context.family_code,
            &root_lang.code,
        )
        .await;
        let root_id = root_member["id"].as_str().unwrap();

        create_test_member_with_parent_id(
            &mut context.app,
            &context.admin_user_token,
            &context.family_code,
            root_id,
            &child_lang.code,
        )
        .await;

        let request = get(&format!(
            "language-family/{}/members/by-id/{}/children?limit=10&offset=0",
            context.family_code, root_id
        ))
        .await;

        let response = context.app.call(request).await.unwrap();

        if response.status() != StatusCode::OK {
            print_response_body(response).await;
            panic!("Failed to search children by parent id");
        }

        let result = crate::tests::response_to_value(response.into_body()).await;
        let items = result["items"].as_array().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["language"]["code"], child_lang.code);
    }

    #[tokio::test]
    async fn test_search_children_by_parent_code() {
        let mut context = create_test_context().await;
        let root_lang = &context.languages[0];
        let child_lang = &context.languages[1];

        create_test_member(
            &mut context.app,
            &context.admin_user_token,
            &context.family_code,
            &root_lang.code,
        )
        .await;

        create_test_member_with_parent_code(
            &mut context.app,
            &context.admin_user_token,
            &context.family_code,
            &root_lang.code,
            &child_lang.code,
        )
        .await;

        let request = get(&format!(
            "language-family/{}/members/by-code/{}/children?limit=10&offset=0",
            context.family_code, root_lang.code
        ))
        .await;

        let response = context.app.call(request).await.unwrap();

        if response.status() != StatusCode::OK {
            print_response_body(response).await;
            panic!("Failed to search children by parent code");
        }

        let result = crate::tests::response_to_value(response.into_body()).await;
        let items = result["items"].as_array().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["language"]["code"], child_lang.code);
    }

    #[tokio::test]
    async fn test_search_members_by_family() {
        let mut context = create_test_context().await;
        let root_lang = &context.languages[0];
        let child_lang = &context.languages[1];

        let root_member = create_test_member(
            &mut context.app,
            &context.admin_user_token,
            &context.family_code,
            &root_lang.code,
        )
        .await;
        let root_id = root_member["id"].as_str().unwrap();

        create_test_member_with_parent_id(
            &mut context.app,
            &context.admin_user_token,
            &context.family_code,
            root_id,
            &child_lang.code,
        )
        .await;

        let request = get(&format!(
            "language-family/{}/members?limit=10&offset=0",
            context.family_code
        ))
        .await;

        let response = context.app.call(request).await.unwrap();

        if response.status() != StatusCode::OK {
            print_response_body(response).await;
            panic!("Failed to search members by family");
        }

        let result = crate::tests::response_to_value(response.into_body()).await;
        let items = result["items"].as_array().unwrap();
        assert_eq!(items.len(), 2);
    }

    #[tokio::test]
    async fn test_search_language_family_members_global() {
        let mut context = create_test_context().await;
        let lang = &context.languages[0];

        create_test_member(
            &mut context.app,
            &context.admin_user_token,
            &context.family_code,
            &lang.code,
        )
        .await;

        let request = get(&format!(
            "language-family-members?q={}&limit=10&offset=0",
            lang.code
        ))
        .await;

        let response = context.app.call(request).await.unwrap();

        if response.status() != StatusCode::OK {
            print_response_body(response).await;
            panic!("Failed to search language family members globally");
        }

        let result = crate::tests::response_to_value(response.into_body()).await;
        let items = result["items"].as_array().unwrap();
        assert!(
            items
                .iter()
                .any(|item| item["language"]["code"] == lang.code)
        );
    }

    #[tokio::test]
    async fn test_delete_language_family_member_by_id() {
        let mut context = create_test_context().await;
        let lang = &context.languages[0];

        let member = create_test_member(
            &mut context.app,
            &context.admin_user_token,
            &context.family_code,
            &lang.code,
        )
        .await;
        let member_id = member["id"].as_str().unwrap();

        let request = delete(
            &context.admin_user_token,
            &format!(
                "language-family/{}/members/by-id/{}",
                context.family_code, member_id
            ),
        );

        let response = context.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        // verify it's deleted
        let get_request = get(&format!(
            "language-family/{}/members/by-id/{}",
            context.family_code, member_id
        ))
        .await;

        let get_response = context.app.call(get_request).await.unwrap();
        assert_eq!(get_response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_delete_language_family_member_by_code() {
        let mut context = create_test_context().await;
        let lang = &context.languages[0];

        create_test_member(
            &mut context.app,
            &context.admin_user_token,
            &context.family_code,
            &lang.code,
        )
        .await;

        let request = delete(
            &context.admin_user_token,
            &format!(
                "language-family/{}/members/by-code/{}",
                context.family_code, lang.code
            ),
        );

        let response = context.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        // verify it's deleted
        let get_request = get(&format!(
            "language-family/{}/members/by-code/{}",
            context.family_code, lang.code
        ))
        .await;

        let get_response = context.app.call(get_request).await.unwrap();
        assert_eq!(get_response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_cannot_create_duplicate_root() {
        let mut context = create_test_context().await;
        let lang1 = &context.languages[0];
        let lang2 = &context.languages[1];

        create_test_member(
            &mut context.app,
            &context.admin_user_token,
            &context.family_code,
            &lang1.code,
        )
        .await;

        // try to create another root
        let create = json!({
            "language_code": lang2.code,
            "relation_type": "descendant",
            "notes": "Another root"
        });

        let request = post(
            &context.admin_user_token,
            &format!("language-family/{}/members", context.family_code),
            create,
        )
        .await;

        let response = context.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_unauthorized_create_without_session() {
        let mut context = create_test_context().await;
        let lang = &context.languages[0];

        let create = json!({
            "language_code": lang.code,
            "relation_type": "descendant",
            "notes": "Test notes"
        });

        let request = axum::http::Request::builder()
            .uri(format!(
                "/api/language-family/{}/members",
                context.family_code
            ))
            .method("POST")
            .header("content-type", "application/json")
            .body(axum::body::Body::from(serde_json::to_vec(&create).unwrap()))
            .unwrap();

        let response = context.app.call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
