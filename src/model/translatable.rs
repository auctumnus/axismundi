use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use validator::Validate;

use crate::{
    err::{AppResult, not_found},
    model::users::User,
    pagination::{PaginatedRequest, PaginatedResponse},
    util::{AppState, ensure_verified},
};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Translatable {
    pub id: Uuid,
    pub slug: String,
    pub title: String,
    pub english: String,
    pub source_name: Option<String>,
    pub source_url: Option<String>,
    pub source_content: Option<String>,
    pub source_language: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing)]
    pub created_by: Uuid,
    #[serde(skip_serializing)]
    pub updated_by: Uuid,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CreateTranslatable {
    #[validate(length(min = 1, max = 200))]
    pub slug: String,

    #[validate(length(min = 1, max = 500))]
    pub title: String,

    #[validate(length(min = 1, max = 100000))]
    pub english: String,

    #[validate(length(max = 1000))]
    pub source_name: Option<String>,

    #[validate(url)]
    #[validate(length(max = 2000))]
    pub source_url: Option<String>,

    #[validate(length(max = 100000))]
    pub source_content: Option<String>,

    #[validate(length(max = 100))]
    pub source_language: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct UpdateTranslatable {
    #[validate(length(min = 1, max = 200))]
    pub slug: Option<String>,

    #[validate(length(min = 1, max = 500))]
    pub title: Option<String>,

    #[validate(length(min = 1, max = 100000))]
    pub english: Option<String>,

    #[validate(length(max = 1000))]
    pub source_name: Option<String>,

    #[validate(url)]
    #[validate(length(max = 2000))]
    pub source_url: Option<String>,

    #[validate(length(max = 100000))]
    pub source_content: Option<String>,

    #[validate(length(max = 100))]
    pub source_language: Option<String>,
}

pub struct TranslatableRepository {
    state: AppState,
}

impl TranslatableRepository {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    pub async fn find_by_id(&self, id: Uuid) -> AppResult<Translatable> {
        let result = sqlx::query_as!(
            Translatable,
            r#"
                SELECT
                    id,
                    slug,
                    title,
                    english,
                    source_name,
                    source_url,
                    source_content,
                    source_language,
                    created_at,
                    updated_at,
                    created_by,
                    updated_by
                FROM translatable
                WHERE id = $1
            "#,
            id
        )
        .fetch_optional(&self.state.pool)
        .await?;

        result.ok_or_else(|| not_found(format!("translatable with id '{id}'")))
    }

    pub async fn create(
        &self,
        requestor: &User,
        translatable: CreateTranslatable,
    ) -> AppResult<Translatable> {
        translatable.validate()?;

        ensure_verified(requestor)?;

        let result = sqlx::query_as!(
            Translatable,
            r#"
                INSERT INTO translatable (slug, title, english, source_name, source_url, source_content, source_language, created_by, updated_by)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $8)
                RETURNING id, slug, title, english, source_name, source_url, source_content, source_language, created_at, updated_at, created_by, updated_by
            "#,
            translatable.slug,
            translatable.title,
            translatable.english,
            translatable.source_name,
            translatable.source_url,
            translatable.source_content,
            translatable.source_language,
            requestor.id
        )
        .fetch_one(&self.state.pool)
        .await?;

        // Create activity (translatables are always public)
        let activity_repo = crate::model::user_activities::UserActivityRepository::new(self.state.clone());
        let _activity = activity_repo.create(
            requestor.id,
            crate::model::user_activities::ActivityType::CreateTranslatable,
            result.id,
            "translatable",
            None,
            None,
        ).await?;

        Ok(result)
    }

    pub async fn update(
        &self,
        requestor: &User,
        id: Uuid,
        updates: UpdateTranslatable,
    ) -> AppResult<Translatable> {
        updates.validate()?;

        ensure_verified(requestor)?;

        let result = sqlx::query_as!(
            Translatable,
            r#"
                UPDATE translatable
                SET slug = COALESCE($2, slug),
                    title = COALESCE($3, title),
                    english = COALESCE($4, english),
                    source_name = COALESCE($5, source_name),
                    source_url = COALESCE($6, source_url),
                    source_content = COALESCE($7, source_content),
                    source_language = COALESCE($8, source_language),
                    updated_by = $9,
                    updated_at = CURRENT_TIMESTAMP
                WHERE id = $1
                RETURNING id, slug, title, english, source_name, source_url, source_content, source_language, created_at, updated_at, created_by, updated_by
            "#,
            id,
            updates.slug,
            updates.title,
            updates.english,
            updates.source_name,
            updates.source_url,
            updates.source_content,
            updates.source_language,
            requestor.id
        )
        .fetch_optional(&self.state.pool)
        .await?;

        let updated_translatable = result.ok_or_else(|| not_found(format!("translatable with id '{id}'")))?;

        // Create activity (translatables are always public)
        let activity_repo = crate::model::user_activities::UserActivityRepository::new(self.state.clone());
        let _activity = activity_repo.create(
            requestor.id,
            crate::model::user_activities::ActivityType::UpdateTranslatable,
            updated_translatable.id,
            "translatable",
            None,
            None,
        ).await?;

        Ok(updated_translatable)
    }

    pub async fn delete(&self, requestor: &User, id: Uuid) -> AppResult<bool> {
        ensure_verified(requestor)?;

        // Only allow creator to delete
        let existing = self.find_by_id(id).await?;
        if existing.created_by != requestor.id {
            return Err(crate::err::forbidden("only the creator can delete this translatable"));
        }

        let result = sqlx::query!("DELETE FROM translatable WHERE id = $1", id)
            .execute(&self.state.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn search(
        &self,
        pagination: PaginatedRequest,
        search: TranslatableSearch,
    ) -> AppResult<PaginatedResponse<Translatable>> {
        let items_future = sqlx::query_as!(
            Translatable,
            r#"
                SELECT
                    id,
                    slug,
                    title,
                    english,
                    source_name,
                    source_url,
                    source_content,
                    source_language,
                    created_at,
                    updated_at,
                    created_by,
                    updated_by
                FROM translatable
                WHERE
                ($1::TIMESTAMPTZ IS NULL OR created_at < $1)
                AND ($2::TIMESTAMPTZ IS NULL OR created_at > $2)
                AND ($3::TEXT IS NULL OR source_language = $3)
                ORDER BY (
                    CASE
                        WHEN $4::TEXT IS NOT NULL AND english ILIKE '%' || $4 || '%' THEN 100.0
                        WHEN $4::TEXT IS NOT NULL AND source_content ILIKE '%' || $4 || '%' THEN 90.0
                        WHEN $4::TEXT IS NOT NULL AND source_name ILIKE '%' || $4 || '%' THEN 80.0
                        ELSE 0.0
                    END +
                    CASE WHEN $4::TEXT IS NOT NULL THEN
                        similarity(english, $4) * 3.0 +
                        COALESCE(similarity(source_content, $4), 0.0) * 2.0 +
                        COALESCE(similarity(source_name, $4), 0.0) * 1.0
                    ELSE 0.0
                    END
                ) DESC, id
                LIMIT $5
                OFFSET $6
            "#,
            search.created_before,
            search.created_after,
            search.source_language,
            search.q,
            i64::from(pagination.limit),
            i64::from(pagination.offset)
        )
        .fetch_all(&self.state.pool);

        let count_future = sqlx::query_scalar!(
            r#"
                SELECT COUNT(*)
                FROM translatable
                WHERE
                ($1::TIMESTAMPTZ IS NULL OR created_at < $1)
                AND ($2::TIMESTAMPTZ IS NULL OR created_at > $2)
                AND ($3::TEXT IS NULL OR source_language = $3)
            "#,
            search.created_before,
            search.created_after,
            search.source_language
        )
        .fetch_one(&self.state.pool);

        let (items, total_count) = tokio::try_join!(items_future, count_future)?;

        let total = total_count.unwrap_or(0);
        let has_more = (i64::from(pagination.offset) + i64::try_from(items.len()).unwrap_or(i64::MAX)) < total;

        Ok(PaginatedResponse {
            items,
            total,
            offset: pagination.offset,
            limit: pagination.limit,
            has_more,
        })
    }
}

#[derive(Default, Debug, Deserialize)]
pub struct TranslatableSearch {
    pub q: Option<String>,
    pub source_language: Option<String>,
    pub created_before: Option<DateTime<Utc>>,
    pub created_after: Option<DateTime<Utc>>,
}

crate::util::repo_from_parts!(TranslatableRepository);
