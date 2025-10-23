use crate::{
    err::{forbidden, unauthorized_no_session, AppResult},
    model::{
        language::LanguageRepository,
        language_invite::PermissionLevel,
        language_permission::LanguagePermissionRepository,
        word::{CreateWord, UpdateWord, Word, WordRepository, WordSearch},
        word_class::WordClassRepository,
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
pub struct WordSearchQuery {
    pub q: Option<String>,
    pub word_class: Option<String>,
}

pub async fn create_word(
    s: Session,
    languages: LanguageRepository,
    permissions: LanguagePermissionRepository,
    words: WordRepository,
    Path(code): Path<String>,
    Json(mut create): Json<CreateWord>,
) -> ApiResponse<Json<Word>> {
    let Some(session) = s.session() else {
        return Err(unauthorized_no_session());
    };

    let language = languages.find_by_code(&code).await?;
    let user_perm = permissions.find_by_user_and_language(session.user_id, language.id).await?;

    let Some(perm) = user_perm else {
        return Err(forbidden("you don't have permission to create words"));
    };

    if perm.permission == PermissionLevel::Viewer {
        return Err(forbidden("viewers cannot create words"));
    }

    create.language = language.id;
    words.create(create, session.user_id).await.map(Json)
}

pub async fn list_words(
    languages: LanguageRepository,
    words: WordRepository,
    word_classes: WordClassRepository,
    Path(code): Path<String>,
    pagination: PaginatedRequest,
    axum::extract::Query(query): axum::extract::Query<WordSearchQuery>,
) -> PaginatedApiResponse<Word> {
    let language = languages.find_by_code(&code).await?;

    let word_class_uuid = if let Some(ref abbr) = query.word_class {
        let classes = word_classes.list_by_language(language.id).await?;
        classes.iter()
            .find(|c| c.abbreviation == *abbr)
            .map(|c| c.id)
    } else {
        None
    };

    let search = WordSearch {
        pagination,
        text_query: query.q,
        word_class: word_class_uuid,
    };

    words.search(language.id, search).await
}

pub async fn edit_word(
    s: Session,
    languages: LanguageRepository,
    permissions: LanguagePermissionRepository,
    words: WordRepository,
    Path((code, slug)): Path<(String, String)>,
    Json(updates): Json<UpdateWord>,
) -> ApiResponse<Json<Word>> {
    let Some(session) = s.session() else {
        return Err(unauthorized_no_session());
    };

    let language = languages.find_by_code(&code).await?;
    let user_perm = permissions.find_by_user_and_language(session.user_id, language.id).await?;

    let Some(perm) = user_perm else {
        return Err(forbidden("you don't have permission to edit words"));
    };

    if perm.permission == PermissionLevel::Viewer {
        return Err(forbidden("viewers cannot edit words"));
    }

    let word = words.find_by_slug(language.id, &slug).await?;
    words.update(word.id, updates, session.user_id).await.map(Json)
}

pub async fn delete_word(
    s: Session,
    languages: LanguageRepository,
    permissions: LanguagePermissionRepository,
    words: WordRepository,
    Path((code, slug)): Path<(String, String)>,
) -> ApiResponse<StatusCode> {
    let Some(session) = s.session() else {
        return Err(unauthorized_no_session());
    };

    let language = languages.find_by_code(&code).await?;
    let user_perm = permissions.find_by_user_and_language(session.user_id, language.id).await?;

    let Some(perm) = user_perm else {
        return Err(forbidden("you don't have permission to delete words"));
    };

    if perm.permission == PermissionLevel::Viewer {
        return Err(forbidden("viewers cannot delete words"));
    }

    let word = words.find_by_slug(language.id, &slug).await?;
    words.delete(word.id).await?;
    Ok(StatusCode::NO_CONTENT)
}
