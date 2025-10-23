use crate::{
    err::{bad_request, forbidden, not_found, unauthorized_no_session, AppResult},
    model::{
        language::LanguageRepository,
        language_invite::PermissionLevel,
        language_permission::LanguagePermissionRepository,
        word_class::{CreateWordClass, UpdateWordClass, WordClass, WordClassRepository, WordClassSearch},
    },
    pagination::{PaginatedRequest, PaginatedResponse},
    util::extract_session::Session,
};
use axum::{
    extract::Path,
    http::StatusCode,
    Json,
};
use serde::Deserialize;

type ApiResponse<T> = AppResult<T>;
type PaginatedApiResponse<T> = AppResult<PaginatedResponse<T>>;

#[derive(Deserialize)]
pub struct SearchQuery {
    pub q: Option<String>,
}

pub async fn create_word_class(
    s: Session,
    languages: LanguageRepository,
    permissions: LanguagePermissionRepository,
    word_classes: WordClassRepository,
    Path(code): Path<String>,
    Json(mut create): Json<CreateWordClass>,
) -> ApiResponse<Json<WordClass>> {
    let Some(session) = s.session() else {
        return Err(unauthorized_no_session());
    };

    let language = languages.find_by_code(&code).await?;
    let user_perm = permissions.find_by_user_and_language(session.user_id, language.id).await?;

    let Some(perm) = user_perm else {
        return Err(forbidden("you don't have permission to create word classes"));
    };

    if perm.permission == PermissionLevel::Viewer {
        return Err(forbidden("viewers cannot create word classes"));
    }

    if create.abbreviation == "search" {
        return Err(bad_request("cannot use 'search' as abbreviation"));
    }

    create.language = language.id;
    word_classes.create(create, session.user_id).await.map(Json)
}

pub async fn list_word_classes(
    languages: LanguageRepository,
    word_classes: WordClassRepository,
    Path(code): Path<String>,
    pagination: PaginatedRequest,
    axum::extract::Query(query): axum::extract::Query<SearchQuery>,
) -> PaginatedApiResponse<WordClass> {
    let language = languages.find_by_code(&code).await?;

    let search = WordClassSearch {
        pagination,
        text_query: query.q,
    };

    word_classes.search(language.id, search).await
}

pub async fn get_word_class(
    languages: LanguageRepository,
    word_classes: WordClassRepository,
    Path((code, abbreviation)): Path<(String, String)>,
) -> ApiResponse<Json<WordClass>> {
    let language = languages.find_by_code(&code).await?;
    let classes = word_classes.list_by_language(language.id).await?;

    classes.iter()
        .find(|c| c.abbreviation == abbreviation)
        .cloned()
        .ok_or_else(|| not_found(format!("word class '{abbreviation}'")))
        .map(Json)
}

pub async fn edit_word_class(
    s: Session,
    languages: LanguageRepository,
    permissions: LanguagePermissionRepository,
    word_classes: WordClassRepository,
    Path((code, abbreviation)): Path<(String, String)>,
    Json(updates): Json<UpdateWordClass>,
) -> ApiResponse<Json<WordClass>> {
    let Some(session) = s.session() else {
        return Err(unauthorized_no_session());
    };

    let language = languages.find_by_code(&code).await?;
    let user_perm = permissions.find_by_user_and_language(session.user_id, language.id).await?;

    let Some(perm) = user_perm else {
        return Err(forbidden("you don't have permission to edit word classes"));
    };

    if perm.permission == PermissionLevel::Viewer {
        return Err(forbidden("viewers cannot edit word classes"));
    }

    let classes = word_classes.list_by_language(language.id).await?;
    let class = classes.iter().find(|c| c.abbreviation == abbreviation)
        .ok_or_else(|| not_found(format!("word class '{abbreviation}'")))?;

    word_classes.update(class.id, updates, session.user_id).await.map(Json)
}

pub async fn delete_word_class(
    s: Session,
    languages: LanguageRepository,
    permissions: LanguagePermissionRepository,
    word_classes: WordClassRepository,
    Path((code, abbreviation)): Path<(String, String)>,
) -> ApiResponse<StatusCode> {
    let Some(session) = s.session() else {
        return Err(unauthorized_no_session());
    };

    let language = languages.find_by_code(&code).await?;
    let user_perm = permissions.find_by_user_and_language(session.user_id, language.id).await?;

    let Some(perm) = user_perm else {
        return Err(forbidden("you don't have permission to delete word classes"));
    };

    if perm.permission == PermissionLevel::Viewer {
        return Err(forbidden("viewers cannot delete word classes"));
    }

    let classes = word_classes.list_by_language(language.id).await?;
    let class = classes.iter().find(|c| c.abbreviation == abbreviation)
        .ok_or_else(|| not_found(format!("word class '{abbreviation}'")))?;

    word_classes.delete(class.id).await?;
    Ok(StatusCode::NO_CONTENT)
}
