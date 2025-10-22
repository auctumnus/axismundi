use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;
use validator::Validate;

use crate::{err::{AppResult, bad_request, not_found}, pagination::{PaginatedRequest, PaginatedResponse}};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Language {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    pub description: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub created_by: Uuid,
    pub updated_by: Uuid,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CreateLanguage {
    #[validate(length(min = 2, max = 10))]
    pub code: String,

    #[validate(length(min = 2, max = 100))]
    pub name: String,

    #[validate(length(max = 5000))]
    pub description: String,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct UpdateLanguage {
    #[validate(length(min = 2, max = 10))]
    pub code: Option<String>,

    #[validate(length(min = 2, max = 100))]
    pub name: Option<String>,

    #[validate(length(max = 5000))]
    pub description: Option<String>,
}

pub struct LanguageRepository {
    pool: PgPool,
}

impl LanguageRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, language: CreateLanguage, user_id: Uuid) -> AppResult<Language> {
        language.validate()?;

        if self.code_exists(&language.code).await? {
            return Err(bad_request("language code is already in use"));
        }

        let result = sqlx::query_as!(
            Language,
            r#"
                INSERT INTO languages (code, name, description, created_by, updated_by)
                VALUES ($1, $2, $3, $4, $4)
                RETURNING *
            "#,
            language.code,
            language.name,
            language.description,
            user_id
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(result)
    }

    pub async fn find_by_id(&self, id: Uuid) -> AppResult<Language> {
        let result = sqlx::query_as!(
            Language,
            "SELECT * FROM languages WHERE id = $1",
            id
        )
        .fetch_optional(&self.pool)
        .await?;

        result.ok_or_else(|| not_found(format!("language with id '{id}'")))
    }

    pub async fn find_by_code(&self, code: &str) -> AppResult<Language> {
        let result = sqlx::query_as!(
            Language,
            "SELECT * FROM languages WHERE code = $1",
            code
        )
        .fetch_optional(&self.pool)
        .await?;

        result.ok_or_else(|| not_found(format!("language with code '{code}'")))
    }

    pub async fn update(&self, id: Uuid, updates: UpdateLanguage, user_id: Uuid) -> AppResult<Language> {
        updates.validate()?;

        if let Some(code) = &updates.code {
            if self.code_exists(code).await? {
                return Err(bad_request("language code is already in use"));
            }
        }

        let result = sqlx::query_as!(
            Language,
            r#"
                UPDATE languages
                SET code = COALESCE($2, code),
                    name = COALESCE($3, name),
                    description = COALESCE($4, description),
                    updated_by = $5,
                    updated_at = CURRENT_TIMESTAMP
                WHERE id = $1
                RETURNING *
            "#,
            id,
            updates.code,
            updates.name,
            updates.description,
            user_id
        )
        .fetch_optional(&self.pool)
        .await?;

        result.ok_or_else(|| not_found(format!("language with id '{id}'")))
    }

    pub async fn delete(&self, id: Uuid) -> AppResult<bool> {
        let result = sqlx::query!("DELETE FROM languages WHERE id = $1", id)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn list(&self, limit: i64, offset: i64) -> AppResult<Vec<Language>> {
        let result = sqlx::query_as!(
            Language,
            "SELECT * FROM languages ORDER BY created_at DESC LIMIT $1 OFFSET $2",
            limit,
            offset
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(result)
    }

    pub async fn count(&self) -> AppResult<i64> {
        let result = sqlx::query!("SELECT COUNT(*) as count FROM languages")
            .fetch_one(&self.pool)
            .await?;

        Ok(result.count.unwrap_or(0))
    }

    pub async fn code_exists(&self, code: &str) -> AppResult<bool> {
        let result = sqlx::query!(
            "SELECT 1 as exists FROM languages WHERE code = $1",
            code
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(result.is_some())
    }

    pub async fn list_by_user(&self, user_id: Uuid, limit: i64, offset: i64) -> AppResult<Vec<Language>> {
        let result = sqlx::query_as!(
            Language,
            "SELECT * FROM languages WHERE created_by = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3",
            user_id,
            limit,
            offset
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(result)
    }

    pub async fn search(&self, search: LanguageSearch) -> AppResult<PaginatedResponse<Language>> {
        use sqlx::QueryBuilder;

        let limit = search.pagination.limit + 1;
        let mut query_builder: QueryBuilder<sqlx::Postgres> = QueryBuilder::new("SELECT * FROM languages WHERE 1=1");

        if let Some(ref q) = search.text_query {
            query_builder.push(" AND (name % ");
            query_builder.push_bind(q);
            query_builder.push(" OR description % ");
            query_builder.push_bind(q);
            query_builder.push(")");
        }

        if let Some(ref owner_username) = search.owned_by {
            query_builder.push(" AND created_by = (SELECT id FROM users WHERE username = ");
            query_builder.push_bind(owner_username);
            query_builder.push(")");
        }

        if let Some(ref edited_by_users) = search.edited_by {
            if !edited_by_users.is_empty() {
                query_builder.push(" AND id IN (SELECT language FROM language_permissions WHERE \"user\" IN (SELECT id FROM users WHERE username IN (");
                let mut separated = query_builder.separated(", ");
                for username in edited_by_users {
                    separated.push_bind(username);
                }
                separated.push_unseparated(")))");
            }
        }

        if let Some(created_before) = search.created_before {
            query_builder.push(" AND created_at < ");
            query_builder.push_bind(created_before);
        }

        if let Some(created_after) = search.created_after {
            query_builder.push(" AND created_at > ");
            query_builder.push_bind(created_after);
        }

        if let Some(cursor) = search.pagination.cursor {
            query_builder.push(" AND id > ");
            query_builder.push_bind(cursor);
        }

        query_builder.push(" ORDER BY created_at DESC LIMIT ");
        query_builder.push_bind(limit);

        let mut items = query_builder
            .build_query_as::<Language>()
            .fetch_all(&self.pool)
            .await?;

        let has_more = items.len() > search.pagination.limit as usize;
        if has_more {
            items.pop();
        }

        let next_cursor = if has_more {
            items.last().map(|l| l.id)
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
pub struct LanguageSearch {
    pub pagination: PaginatedRequest,
    pub text_query: Option<String>,
    pub owned_by: Option<String>,
    pub edited_by: Option<Vec<String>>,
    pub created_before: Option<DateTime<Utc>>,
    pub created_after: Option<DateTime<Utc>>,
}

crate::util::repo_from_parts!(LanguageRepository);
