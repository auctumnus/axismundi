use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;
use validator::Validate;

use crate::{err::{AppResult, bad_request, not_found}, pagination::{PaginatedRequest, PaginatedResponse}};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct WordClass {
    pub id: Uuid,
    pub language: Uuid,
    pub name: String,
    pub abbreviation: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub created_by: Uuid,
    pub updated_by: Uuid,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CreateWordClass {
    pub language: Uuid,

    #[validate(length(min = 1, max = 50))]
    pub name: String,

    #[validate(length(min = 1, max = 10))]
    pub abbreviation: String,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct UpdateWordClass {
    #[validate(length(min = 1, max = 50))]
    pub name: Option<String>,

    #[validate(length(min = 1, max = 10))]
    pub abbreviation: Option<String>,
}

pub struct WordClassRepository {
    pool: PgPool,
}

impl WordClassRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, word_class: CreateWordClass, user_id: Uuid) -> AppResult<WordClass> {
        word_class.validate()?;

        if self.name_exists_in_language(word_class.language, &word_class.name).await? {
            return Err(bad_request("word class name already exists in this language"));
        }

        if self.abbreviation_exists_in_language(word_class.language, &word_class.abbreviation).await? {
            return Err(bad_request("word class abbreviation already exists in this language"));
        }

        let result = sqlx::query_as!(
            WordClass,
            r#"
                INSERT INTO word_classes (language, name, abbreviation, created_by, updated_by)
                VALUES ($1, $2, $3, $4, $4)
                RETURNING *
            "#,
            word_class.language,
            word_class.name,
            word_class.abbreviation,
            user_id
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(result)
    }

    pub async fn find_by_id(&self, id: Uuid) -> AppResult<WordClass> {
        let result = sqlx::query_as!(
            WordClass,
            "SELECT * FROM word_classes WHERE id = $1",
            id
        )
        .fetch_optional(&self.pool)
        .await?;

        result.ok_or_else(|| not_found(format!("word class with id '{id}'")))
    }

    pub async fn list_by_language(&self, language: Uuid) -> AppResult<Vec<WordClass>> {
        let result = sqlx::query_as!(
            WordClass,
            "SELECT * FROM word_classes WHERE language = $1 ORDER BY name",
            language
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(result)
    }

    pub async fn update(&self, id: Uuid, updates: UpdateWordClass, user_id: Uuid) -> AppResult<WordClass> {
        updates.validate()?;

        // Get the current word class to check the language
        let current = self.find_by_id(id).await?;

        if let Some(name) = &updates.name {
            if self.name_exists_in_language(current.language, name).await? {
                return Err(bad_request("word class name already exists in this language"));
            }
        }

        if let Some(abbreviation) = &updates.abbreviation {
            if self.abbreviation_exists_in_language(current.language, abbreviation).await? {
                return Err(bad_request("word class abbreviation already exists in this language"));
            }
        }

        let result = sqlx::query_as!(
            WordClass,
            r#"
                UPDATE word_classes
                SET name = COALESCE($2, name),
                    abbreviation = COALESCE($3, abbreviation),
                    updated_by = $4,
                    updated_at = CURRENT_TIMESTAMP
                WHERE id = $1
                RETURNING *
            "#,
            id,
            updates.name,
            updates.abbreviation,
            user_id
        )
        .fetch_optional(&self.pool)
        .await?;

        result.ok_or_else(|| not_found(format!("word class with id '{id}'")))
    }

    pub async fn delete(&self, id: Uuid) -> AppResult<bool> {
        let result = sqlx::query!("DELETE FROM word_classes WHERE id = $1", id)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn name_exists_in_language(&self, language: Uuid, name: &str) -> AppResult<bool> {
        let result = sqlx::query!(
            "SELECT 1 as exists FROM word_classes WHERE language = $1 AND name = $2",
            language,
            name
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(result.is_some())
    }

    async fn abbreviation_exists_in_language(&self, language: Uuid, abbreviation: &str) -> AppResult<bool> {
        let result = sqlx::query!(
            "SELECT 1 as exists FROM word_classes WHERE language = $1 AND abbreviation = $2",
            language,
            abbreviation
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(result.is_some())
    }

    pub async fn search(&self, language: Uuid, search: WordClassSearch) -> AppResult<PaginatedResponse<WordClass>> {
        use sqlx::QueryBuilder;

        let limit = search.pagination.limit + 1;
        let mut query_builder: QueryBuilder<sqlx::Postgres> = QueryBuilder::new("SELECT * FROM word_classes WHERE language = ");
        query_builder.push_bind(language);

        if let Some(ref q) = search.text_query {
            query_builder.push(" AND name % ");
            query_builder.push_bind(q);
        }

        if let Some(cursor) = search.pagination.cursor {
            query_builder.push(" AND id > ");
            query_builder.push_bind(cursor);
        }

        query_builder.push(" ORDER BY name LIMIT ");
        query_builder.push_bind(limit);

        let mut items = query_builder
            .build_query_as::<WordClass>()
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
pub struct WordClassSearch {
    pub pagination: PaginatedRequest,
    pub text_query: Option<String>,
}

crate::util::repo_from_parts!(WordClassRepository);
