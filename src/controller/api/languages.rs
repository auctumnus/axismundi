use crate::{
    err::{bad_request, forbidden, unauthorized_no_session, AppResult},
    model::{
        language::{CreateLanguage, Language, LanguageRepository, LanguageSearch, UpdateLanguage},
        language_invite::PermissionLevel,
        language_permission::{CreateLanguagePermission, LanguagePermission, LanguagePermissionRepository},
        user::UserRepository,
    },
    pagination::{PaginatedRequest, PaginatedResponse},
    util::extract_session::Session,
};
use axum::{
    extract::Path,
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;

type ApiResponse<T> = AppResult<T>;
type PaginatedApiResponse<T> = AppResult<PaginatedResponse<T>>;

pub async fn create_language(
    s: Session,
    languages: LanguageRepository,
    permissions: LanguagePermissionRepository,
    Json(create): Json<CreateLanguage>,
) -> ApiResponse<Json<Language>> {
    let Some(session) = s.session() else {
        return Err(unauthorized_no_session());
    };

    if create.code == "search" {
        return Err(bad_request("cannot use 'search' as language code"));
    }

    let language = languages.create(create, session.user_id).await?;

    // grant owner permissions
    permissions.create(
        CreateLanguagePermission {
            language: language.id,
            user: session.user_id,
            permission: PermissionLevel::Owner,
            via: None,
        },
        session.user_id,
    ).await?;

    Ok(Json(language))
}

pub async fn get_language(
    languages: LanguageRepository,
    Path(code): Path<String>,
) -> ApiResponse<Json<Language>> {
    languages.find_by_code(&code).await.map(Json)
}

#[derive(Deserialize)]
pub struct LanguageSearchQuery {
    pub owned_by: Option<String>,
    pub edited_by: Option<String>,
    pub q: Option<String>,
    pub created_before: Option<DateTime<Utc>>,
    pub created_after: Option<DateTime<Utc>>,
}

pub async fn list_languages(
    languages: LanguageRepository,
    _users: UserRepository,
    _permissions: LanguagePermissionRepository,
    pagination: PaginatedRequest,
    axum::extract::Query(query): axum::extract::Query<LanguageSearchQuery>,
) -> PaginatedApiResponse<Language> {
    let edited_by = query.edited_by.map(|s| s.split(',').map(String::from).collect());

    let search = LanguageSearch {
        pagination,
        text_query: query.q,
        owned_by: query.owned_by,
        edited_by,
        created_before: query.created_before,
        created_after: query.created_after,
    };

    languages.search(search).await
}

pub async fn edit_language(
    s: Session,
    languages: LanguageRepository,
    permissions: LanguagePermissionRepository,
    Path(code): Path<String>,
    Json(updates): Json<UpdateLanguage>,
) -> ApiResponse<Json<Language>> {
    let Some(session) = s.session() else {
        return Err(unauthorized_no_session());
    };

    let language = languages.find_by_code(&code).await?;
    let user_perm = permissions.find_by_user_and_language(session.user_id, language.id).await?;

    let Some(perm) = user_perm else {
        return Err(forbidden("you don't have permission to edit this language"));
    };

    if perm.permission == PermissionLevel::Viewer {
        return Err(forbidden("viewers cannot edit language"));
    }

    languages.update(language.id, updates, session.user_id).await.map(Json)
}

pub async fn delete_language(
    s: Session,
    languages: LanguageRepository,
    permissions: LanguagePermissionRepository,
    Path(code): Path<String>,
) -> ApiResponse<StatusCode> {
    let Some(session) = s.session() else {
        return Err(unauthorized_no_session());
    };

    let language = languages.find_by_code(&code).await?;
    let user_perm = permissions.find_by_user_and_language(session.user_id, language.id).await?;

    let Some(perm) = user_perm else {
        return Err(forbidden("you don't have permission to delete this language"));
    };

    if perm.permission != PermissionLevel::Owner {
        return Err(forbidden("only the owner can delete a language"));
    }

    languages.delete(language.id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn get_language_owner(
    languages: LanguageRepository,
    users: UserRepository,
    Path(code): Path<String>,
) -> ApiResponse<axum::response::Redirect> {
    let language = languages.find_by_code(&code).await?;
    let owner = users.find_by_id(language.created_by).await?;
    Ok(axum::response::Redirect::to(&format!("/users/{}", owner.username)))
}

pub async fn get_language_editors(
    languages: LanguageRepository,
    _permissions: LanguagePermissionRepository,
    Path(code): Path<String>,
    _pagination: PaginatedRequest,
) -> PaginatedApiResponse<LanguagePermission> {
    let _language = languages.find_by_code(&code).await?;
    // TODO: implement pagination
    Ok(PaginatedResponse {
        items: vec![],
        pages_left: 0,
        next_cursor: None,
        previous_cursor: None,
    })
}
