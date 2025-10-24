use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sqlx::FromRow;
use uuid::Uuid;
use validator::Validate;

use crate::{
    err::{AppResult, bad_request, not_found},
    model::{language_invites::PermissionLevel, users::User},
    pagination::{PaginatedRequest, PaginatedResponse},
    util::{AppState, ensure_verified},
};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[allow(clippy::struct_field_names)]
pub struct Word {
    pub id: Uuid,
    pub language: Uuid,
    pub word_class: Option<Uuid>,
    pub word: String,
    pub slug: String,
    pub definition: String,
    pub ipa: Option<String>,
    pub notes: Option<String>,
    pub extra: Option<JsonValue>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub created_by: Uuid,
    pub updated_by: Uuid,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CreateWord {
    #[validate(length(min = 1, max = 200))]
    pub word: String,

    #[validate(length(min = 1, max = 10))]
    pub word_class: String,

    // TODO: implement slug generation
    #[validate(length(min = 1, max = 200))]
    pub slug: String,

    #[validate(length(min = 1, max = 5000))]
    pub definition: String,

    #[validate(length(max = 200))]
    pub ipa: Option<String>,

    #[validate(length(max = 10000))]
    pub notes: Option<String>,

    pub extra: Option<JsonValue>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct UpdateWord {
    #[validate(length(min = 1, max = 200))]
    pub word: Option<String>,

    #[validate(length(min = 1, max = 10))]
    pub word_class: Option<String>,

    // TODO: implement slug generation on word update
    #[validate(length(min = 1, max = 200))]
    pub slug: Option<String>,

    #[validate(length(min = 1, max = 5000))]
    pub definition: Option<String>,

    #[validate(length(max = 200))]
    pub ipa: Option<String>,

    #[validate(length(max = 10000))]
    pub notes: Option<String>,

    pub extra: Option<JsonValue>,
}

pub struct WordRepository {
    state: AppState,
}

impl WordRepository {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    pub async fn create(
        &self,
        requestor: &User,
        language: Uuid,
        word: CreateWord,
    ) -> AppResult<Word> {
        word.validate()?;

        ensure_verified(requestor)?;

        let permissions = crate::model::language_permissions::LanguagePermissionRepository::new(
            self.state.clone(),
        );
        let user_perm = permissions
            .find_by_user_and_language(requestor.id, language)
            .await?;

        let Some(perm) = user_perm else {
            return Err(bad_request("you don't have permission to create words"));
        };

        if perm.permission == PermissionLevel::Viewer {
            return Err(bad_request("viewers cannot create words"));
        }

        let word_classes = crate::model::word_classes::WordClassRepository::new(self.state.clone());
        let word_class = word_classes
            .find_by_abbreviation(language, &word.word_class)
            .await?;

        let result = sqlx::query_as!(
            Word,
            r#"
                INSERT INTO words (language, word_class, word, slug, definition, ipa, notes, extra, created_by, updated_by)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $9)
                RETURNING *
            "#,
            language,
            word_class.id,
            word.word,
            word.slug,
            word.definition,
            word.ipa,
            word.notes,
            word.extra,
            requestor.id
        )
        .fetch_one(&self.state.pool)
        .await?;

        Ok(result)
    }

    pub async fn find_by_id(&self, id: Uuid) -> AppResult<Word> {
        let result = sqlx::query_as!(Word, "SELECT * FROM words WHERE id = $1", id)
            .fetch_optional(&self.state.pool)
            .await?;

        result.ok_or_else(|| not_found(format!("word with id '{id}'")))
    }

    pub async fn find_by_slug(&self, language: Uuid, slug: &str) -> AppResult<Word> {
        let result = sqlx::query_as!(
            Word,
            "SELECT * FROM words WHERE language = $1 AND slug = $2",
            language,
            slug
        )
        .fetch_optional(&self.state.pool)
        .await?;

        result.ok_or_else(|| not_found(format!("word with slug '{slug}'")))
    }

    pub async fn list_by_language(
        &self,
        language: Uuid,
        limit: i64,
        offset: i64,
    ) -> AppResult<Vec<Word>> {
        let result = sqlx::query_as!(
            Word,
            "SELECT * FROM words WHERE language = $1 ORDER BY word LIMIT $2 OFFSET $3",
            language,
            limit,
            offset
        )
        .fetch_all(&self.state.pool)
        .await?;

        Ok(result)
    }

    pub async fn search_by_word(
        &self,
        language: Uuid,
        query: &str,
        limit: i64,
    ) -> AppResult<Vec<Word>> {
        let search_pattern = format!("%{query}%");
        let result = sqlx::query_as!(
            Word,
            "SELECT * FROM words WHERE language = $1 AND word ILIKE $2 ORDER BY word LIMIT $3",
            language,
            search_pattern,
            limit
        )
        .fetch_all(&self.state.pool)
        .await?;

        Ok(result)
    }

    pub async fn search_by_definition(
        &self,
        language: Uuid,
        query: &str,
        limit: i64,
    ) -> AppResult<Vec<Word>> {
        let search_pattern = format!("%{query}%");
        let result = sqlx::query_as!(
            Word,
            "SELECT * FROM words WHERE language = $1 AND definition ILIKE $2 ORDER BY word LIMIT $3",
            language,
            search_pattern,
            limit
        )
        .fetch_all(&self.state.pool)
        .await?;

        Ok(result)
    }

    pub async fn list_by_word_class(
        &self,
        word_class: Uuid,
        limit: i64,
        offset: i64,
    ) -> AppResult<Vec<Word>> {
        let result = sqlx::query_as!(
            Word,
            "SELECT * FROM words WHERE word_class = $1 ORDER BY word LIMIT $2 OFFSET $3",
            word_class,
            limit,
            offset
        )
        .fetch_all(&self.state.pool)
        .await?;

        Ok(result)
    }

    pub async fn update(&self, requestor: &User, id: Uuid, updates: UpdateWord) -> AppResult<Word> {
        updates.validate()?;

        ensure_verified(requestor)?;

        // get the word to find its language
        let word = self.find_by_id(id).await?;

        let permissions = crate::model::language_permissions::LanguagePermissionRepository::new(
            self.state.clone(),
        );
        let user_perm = permissions
            .find_by_user_and_language(requestor.id, word.language)
            .await?;

        let Some(perm) = user_perm else {
            return Err(bad_request("you don't have permission to edit words"));
        };

        if perm.permission == PermissionLevel::Viewer {
            return Err(bad_request("viewers cannot edit words"));
        }

        let word_classes = crate::model::word_classes::WordClassRepository::new(self.state.clone());

        let word_class = if let Some(ref abbreviation) = updates.word_class {
            Some(
                word_classes
                    .find_by_abbreviation(word.language, abbreviation)
                    .await?
                    .id,
            )
        } else {
            None
        };

        let result = sqlx::query_as!(
            Word,
            r#"
                UPDATE words
                SET word_class = COALESCE($2, word_class),
                    word = COALESCE($3, word),
                    slug = COALESCE($4, slug),
                    definition = COALESCE($5, definition),
                    ipa = COALESCE($6, ipa),
                    notes = COALESCE($7, notes),
                    extra = COALESCE($8, extra),
                    updated_by = $9,
                    updated_at = CURRENT_TIMESTAMP
                WHERE id = $1
                RETURNING *
            "#,
            id,
            word_class,
            updates.word,
            updates.slug,
            updates.definition,
            updates.ipa,
            updates.notes,
            updates.extra,
            requestor.id
        )
        .fetch_optional(&self.state.pool)
        .await?;

        result.ok_or_else(|| not_found(format!("word with id '{id}'")))
    }

    pub async fn delete(&self, requestor: &User, id: Uuid) -> AppResult<bool> {
        ensure_verified(requestor)?;

        // get the word to find its language
        let word = self.find_by_id(id).await?;

        let permissions = crate::model::language_permissions::LanguagePermissionRepository::new(
            self.state.clone(),
        );
        let user_perm = permissions
            .find_by_user_and_language(requestor.id, word.language)
            .await?;

        let Some(perm) = user_perm else {
            return Err(bad_request("you don't have permission to delete words"));
        };

        if perm.permission == PermissionLevel::Viewer {
            return Err(bad_request("viewers cannot delete words"));
        }

        let result = sqlx::query!("DELETE FROM words WHERE id = $1", id)
            .execute(&self.state.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn count_by_language(&self, language: Uuid) -> AppResult<i64> {
        let result = sqlx::query!(
            "SELECT COUNT(*) as count FROM words WHERE language = $1",
            language
        )
        .fetch_one(&self.state.pool)
        .await?;

        Ok(result.count.unwrap_or(0))
    }

    pub async fn search(
        &self,
        language: Uuid,
        pagination: PaginatedRequest,
        search: WordSearch,
    ) -> AppResult<PaginatedResponse<Word>> {
        // search strategy:
        // - exact matches in word, then definition, then notes
        // - trigram similarity

        let word_class = if let Some(ref abbreviation) = search.word_class {
            Some(
                crate::model::word_classes::WordClassRepository::new(self.state.clone())
                    .find_by_abbreviation(language, abbreviation)
                    .await?
                    .id,
            )
        } else {
            None
        };

        let items_future = sqlx::query_as!(
            Word,
            r#"
                SELECT *
                FROM words
                WHERE
                language = $1
                AND ($2::UUID IS NULL OR word_class = $2)
                AND ($3::TIMESTAMPTZ IS NULL OR created_at < $3)
                AND ($4::TIMESTAMPTZ IS NULL OR created_at > $4)
                ORDER BY (
                    CASE
                        WHEN $5::TEXT IS NOT NULL AND word ILIKE '%' || $5 || '%' THEN 100.0
                        WHEN $5::TEXT IS NOT NULL AND definition ILIKE '%' || $5 || '%' THEN 90.0
                        WHEN $5::TEXT IS NOT NULL AND notes ILIKE '%' || $5 || '%' THEN 80.0
                        ELSE 0.0
                    END +
                    CASE WHEN $4::TEXT IS NOT NULL THEN
                        similarity(word, $5) * 3.0 +
                        COALESCE(similarity(definition, $5), 0.0) * 2.0 +
                        COALESCE(similarity(notes, $5), 0.0) * 1.0
                    ELSE 0.0
                    END
                ) DESC, id
                LIMIT $6
                OFFSET $7
            "#,
            language,
            word_class,
            search.created_before,
            search.created_after,
            search.text_query,
            i64::from(pagination.limit),
            i64::from(pagination.offset)
        )
        .fetch_all(&self.state.pool);

        let count_future = sqlx::query_scalar!(
            r#"
                SELECT COUNT(*)
                FROM words
                WHERE
                language = $1
                AND ($2::UUID IS NULL OR word_class = $2)
                AND ($3::TIMESTAMPTZ IS NULL OR created_at < $3)
                AND ($4::TIMESTAMPTZ IS NULL OR created_at > $4)
            "#,
            language,
            word_class,
            search.created_before,
            search.created_after
        )
        .fetch_one(&self.state.pool);

        let (items, total_count) = tokio::try_join!(items_future, count_future)?;

        let total = total_count.unwrap_or(0);
        let has_more = (i64::from(pagination.offset) + items.len() as i64) < total;

        Ok(PaginatedResponse {
            items,
            total,
            offset: pagination.offset,
            limit: pagination.limit,
            has_more,
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct WordSearch {
    pub text_query: Option<String>,
    pub word_class: Option<String>,
    pub created_before: Option<DateTime<Utc>>,
    pub created_after: Option<DateTime<Utc>>,
}

crate::util::repo_from_parts!(WordRepository);
