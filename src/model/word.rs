use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;
use validator::Validate;

use crate::{err::{AppResult, not_found}, pagination::{PaginatedRequest, PaginatedResponse}};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
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
    pub language: Uuid,
    pub word_class: Option<Uuid>,

    #[validate(length(min = 1, max = 200))]
    pub word: String,

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
    pub word_class: Option<Uuid>,

    #[validate(length(min = 1, max = 200))]
    pub word: Option<String>,

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
    pool: PgPool,
}

impl WordRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, word: CreateWord, user_id: Uuid) -> AppResult<Word> {
        word.validate()?;

        let result = sqlx::query_as!(
            Word,
            r#"
                INSERT INTO words (language, word_class, word, slug, definition, ipa, notes, extra, created_by, updated_by)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $9)
                RETURNING *
            "#,
            word.language,
            word.word_class,
            word.word,
            word.slug,
            word.definition,
            word.ipa,
            word.notes,
            word.extra,
            user_id
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(result)
    }

    pub async fn find_by_id(&self, id: Uuid) -> AppResult<Word> {
        let result = sqlx::query_as!(
            Word,
            "SELECT * FROM words WHERE id = $1",
            id
        )
        .fetch_optional(&self.pool)
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
        .fetch_optional(&self.pool)
        .await?;

        result.ok_or_else(|| not_found(format!("word with slug '{slug}'")))
    }

    pub async fn list_by_language(&self, language: Uuid, limit: i64, offset: i64) -> AppResult<Vec<Word>> {
        let result = sqlx::query_as!(
            Word,
            "SELECT * FROM words WHERE language = $1 ORDER BY word LIMIT $2 OFFSET $3",
            language,
            limit,
            offset
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(result)
    }

    pub async fn search_by_word(&self, language: Uuid, query: &str, limit: i64) -> AppResult<Vec<Word>> {
        let search_pattern = format!("%{query}%");
        let result = sqlx::query_as!(
            Word,
            "SELECT * FROM words WHERE language = $1 AND word ILIKE $2 ORDER BY word LIMIT $3",
            language,
            search_pattern,
            limit
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(result)
    }

    pub async fn search_by_definition(&self, language: Uuid, query: &str, limit: i64) -> AppResult<Vec<Word>> {
        let search_pattern = format!("%{query}%");
        let result = sqlx::query_as!(
            Word,
            "SELECT * FROM words WHERE language = $1 AND definition ILIKE $2 ORDER BY word LIMIT $3",
            language,
            search_pattern,
            limit
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(result)
    }

    pub async fn list_by_word_class(&self, word_class: Uuid, limit: i64, offset: i64) -> AppResult<Vec<Word>> {
        let result = sqlx::query_as!(
            Word,
            "SELECT * FROM words WHERE word_class = $1 ORDER BY word LIMIT $2 OFFSET $3",
            word_class,
            limit,
            offset
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(result)
    }

    pub async fn update(&self, id: Uuid, updates: UpdateWord, user_id: Uuid) -> AppResult<Word> {
        updates.validate()?;

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
            updates.word_class,
            updates.word,
            updates.slug,
            updates.definition,
            updates.ipa,
            updates.notes,
            updates.extra,
            user_id
        )
        .fetch_optional(&self.pool)
        .await?;

        result.ok_or_else(|| not_found(format!("word with id '{id}'")))
    }

    pub async fn delete(&self, id: Uuid) -> AppResult<bool> {
        let result = sqlx::query!("DELETE FROM words WHERE id = $1", id)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn count_by_language(&self, language: Uuid) -> AppResult<i64> {
        let result = sqlx::query!(
            "SELECT COUNT(*) as count FROM words WHERE language = $1",
            language
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(result.count.unwrap_or(0))
    }

    pub async fn search(&self, language: Uuid, search: WordSearch) -> AppResult<PaginatedResponse<Word>> {
        use sqlx::QueryBuilder;

        let limit = search.pagination.limit + 1;
        let mut query_builder: QueryBuilder<sqlx::Postgres> = QueryBuilder::new("SELECT * FROM words WHERE language = ");
        query_builder.push_bind(language);

        if let Some(ref q) = search.text_query {
            query_builder.push(" AND (word % ");
            query_builder.push_bind(q);
            query_builder.push(" OR definition % ");
            query_builder.push_bind(q);
            query_builder.push(")");
        }

        if let Some(word_class) = search.word_class {
            query_builder.push(" AND word_class = ");
            query_builder.push_bind(word_class);
        }

        if let Some(cursor) = search.pagination.cursor {
            query_builder.push(" AND id > ");
            query_builder.push_bind(cursor);
        }

        query_builder.push(" ORDER BY word LIMIT ");
        query_builder.push_bind(limit);

        let mut items = query_builder
            .build_query_as::<Word>()
            .fetch_all(&self.pool)
            .await?;

        let has_more = items.len() > search.pagination.limit as usize;
        if has_more {
            items.pop();
        }

        let next_cursor = if has_more {
            items.last().map(|w| w.id)
        } else {
            None
        };

        let previous_cursor = search.pagination.cursor;
        let pages_left = if has_more { 1 } else { 0 };

        Ok(PaginatedResponse {
            items,
            pages_left,
            next_cursor,
            previous_cursor,
        })
    }
}

#[derive(Debug)]
pub struct WordSearch {
    pub pagination: PaginatedRequest,
    pub text_query: Option<String>,
    pub word_class: Option<Uuid>,
}

crate::util::repo_from_parts!(WordRepository);
