use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
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
pub struct Quotation {
    pub id: Uuid,
    #[serde(skip_serializing)]
    pub translation: Uuid,
    #[serde(skip_serializing)]
    pub definition: Uuid,
    pub span_start: i32,
    pub span_end: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing)]
    pub created_by: Uuid,
    #[serde(skip_serializing)]
    pub updated_by: Uuid,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CreateQuotation {
    pub definition: Uuid,
    
    #[validate(range(min = 0))]
    pub span_start: i32,
    #[validate(range(min = 0))]
    pub span_end: i32,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct UpdateQuotation {
    #[validate(range(min = 0))]
    pub span_start: Option<i32>,
    #[validate(range(min = 0))]
    pub span_end: Option<i32>,
}

pub struct QuotationRepository {
    state: AppState,
}

impl QuotationRepository {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    pub async fn find_by_id(&self, id: Uuid) -> AppResult<Quotation> {
        let result = sqlx::query_as!(
            Quotation,
            r#"
                SELECT
                    id,
                    translation,
                    definition,
                    span_start,
                    span_end,
                    created_at,
                    updated_at,
                    created_by,
                    updated_by
                FROM quotation
                WHERE id = $1
            "#,
            id
        )
        .fetch_optional(&self.state.pool)
        .await?;

        result.ok_or_else(|| not_found(format!("quotation with id '{id}'")))
    }

    pub async fn create(
        &self,
        requestor: &User,
        translation_id: Uuid,
        definition_id: Uuid,
        quotation: CreateQuotation,
    ) -> AppResult<Quotation> {
        quotation.validate()?;

        ensure_verified(requestor)?;

        // Get the translation to find its language
        let translation = crate::model::translations::TranslationRepository::new(self.state.clone())
            .find_by_id(translation_id)
            .await?;

        // Get the definition to find its word and language
        let definition = crate::model::definitions::DefinitionRepository::new(self.state.clone())
            .find_by_id(definition_id)
            .await?;
        let word = crate::model::words::WordRepository::new(self.state.clone())
            .find_by_id(definition.word)
            .await?;

        // Both the translation and definition must be for the same language
        if translation.language != word.language {
            return Err(bad_request("translation and definition must be for the same language"));
        }

        // Check permissions
        let permissions = crate::model::language_permissions::LanguagePermissionRepository::new(
            self.state.clone(),
        );
        let user_perm = permissions
            .find_by_user_and_language(requestor.id, translation.language)
            .await?;

        let Some(perm) = user_perm else {
            return Err(bad_request("you don't have permission to create quotations"));
        };

        if perm.permission == PermissionLevel::Viewer {
            return Err(bad_request("viewers cannot create quotations"));
        }

        // check for overlapping quotations
        let overlapping = sqlx::query!(
            r#"
                SELECT EXISTS (
                    SELECT 1 FROM quotation
                    WHERE span_start < $1 AND span_end > $2
                    AND translation = $3
                )
            "#,
            quotation.span_end,
            quotation.span_start,
            translation_id
        )
        .fetch_one(&self.state.pool)
        .await?;

        if overlapping.exists.unwrap_or(false) {
            return Err(bad_request("quotation overlaps with an existing quotation"));
        }

        let result = sqlx::query_as!(
            Quotation,
            r#"
                INSERT INTO quotation (translation, definition, span_start, span_end, created_by, updated_by)
                VALUES ($1, $2, $3, $4, $5, $5)
                RETURNING id, translation, definition, span_start, span_end, created_at, updated_at, created_by, updated_by
            "#,
            translation_id,
            definition_id,
            quotation.span_start,
            quotation.span_end,
            requestor.id
        )
        .fetch_one(&self.state.pool)
        .await?;

        Ok(result)
    }

    pub async fn update(
        &self,
        requestor: &User,
        id: Uuid,
        updates: UpdateQuotation,
    ) -> AppResult<Quotation> {
        updates.validate()?;

        ensure_verified(requestor)?;

        // Get the quotation and check language permissions
        let existing = self.find_by_id(id).await?;
        let translation = crate::model::translations::TranslationRepository::new(self.state.clone())
            .find_by_id(existing.translation)
            .await?;

        // Check permissions
        let permissions = crate::model::language_permissions::LanguagePermissionRepository::new(
            self.state.clone(),
        );
        let user_perm = permissions
            .find_by_user_and_language(requestor.id, translation.language)
            .await?;

        let Some(perm) = user_perm else {
            return Err(bad_request("you don't have permission to edit quotations"));
        };

        if perm.permission == PermissionLevel::Viewer {
            return Err(bad_request("viewers cannot edit quotations"));
        }

        let span_end = updates.span_end.unwrap_or(existing.span_end);
        let span_start = updates.span_start.unwrap_or(existing.span_start);

        if span_start >= span_end {
            return Err(bad_request("span_start must be less than span_end"));
        }

        if span_start < 0 || span_end < 0 {
            return Err(bad_request("span_start and span_end must be non-negative"));
        }

        if span_end > translation.translated_text.chars().count() as i32 {
            return Err(bad_request("span_end exceeds length of translated text"));
        }


        // check for overlapping quotations
        let overlapping = sqlx::query!(
            r#"
                SELECT EXISTS (
                    SELECT 1 FROM quotation
                    WHERE span_start < $1 AND span_end > $2
                    AND translation = $3
                    AND id <> $4
                )
            "#,
            span_end,
            span_start,
            existing.translation,
            id
        )
        .fetch_one(&self.state.pool)
        .await?;

        if overlapping.exists.unwrap_or(false) {
            return Err(bad_request("quotation overlaps with an existing quotation"));
        }

        let result = sqlx::query_as!(
            Quotation,
            r#"
                UPDATE quotation
                SET span_start = COALESCE($2, span_start),
                    span_end = COALESCE($3, span_end),
                    updated_by = $4,
                    updated_at = CURRENT_TIMESTAMP
                WHERE id = $1
                RETURNING id, translation, definition, span_start, span_end, created_at, updated_at, created_by, updated_by
            "#,
            id,
            updates.span_start,
            updates.span_end,
            requestor.id
        )
        .fetch_optional(&self.state.pool)
        .await?;

        result.ok_or_else(|| not_found(format!("quotation with id '{id}'")))
    }

    pub async fn delete(&self, requestor: &User, id: Uuid) -> AppResult<bool> {
        ensure_verified(requestor)?;

        // Get the quotation and check language permissions
        let existing = self.find_by_id(id).await?;
        let translation = crate::model::translations::TranslationRepository::new(self.state.clone())
            .find_by_id(existing.translation)
            .await?;

        // Check permissions
        let permissions = crate::model::language_permissions::LanguagePermissionRepository::new(
            self.state.clone(),
        );
        let user_perm = permissions
            .find_by_user_and_language(requestor.id, translation.language)
            .await?;

        let Some(perm) = user_perm else {
            return Err(bad_request("you don't have permission to delete quotations"));
        };

        if perm.permission == PermissionLevel::Viewer {
            return Err(bad_request("viewers cannot delete quotations"));
        }

        let result = sqlx::query!("DELETE FROM quotation WHERE id = $1", id)
            .execute(&self.state.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn list_by_translation(
        &self,
        translation_id: Uuid,
        pagination: PaginatedRequest,
    ) -> AppResult<PaginatedResponse<Quotation>> {
        let items_future = sqlx::query_as!(
            Quotation,
            r#"
                SELECT
                    id,
                    translation,
                    definition,
                    span_start,
                    span_end,
                    created_at,
                    updated_at,
                    created_by,
                    updated_by
                FROM quotation
                WHERE translation = $1
                ORDER BY span_start ASC
                LIMIT $2
                OFFSET $3
            "#,
            translation_id,
            i64::from(pagination.limit),
            i64::from(pagination.offset)
        )
        .fetch_all(&self.state.pool);

        let count_future = sqlx::query_scalar!(
            r#"
                SELECT COUNT(*)
                FROM quotation
                WHERE translation = $1
            "#,
            translation_id
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

    pub async fn list_by_definition(
        &self,
        definition_id: Uuid,
        pagination: PaginatedRequest,
    ) -> AppResult<PaginatedResponse<Quotation>> {
        let items_future = sqlx::query_as!(
            Quotation,
            r#"
                SELECT
                    id,
                    translation,
                    definition,
                    span_start,
                    span_end,
                    created_at,
                    updated_at,
                    created_by,
                    updated_by
                FROM quotation
                WHERE definition = $1
                ORDER BY created_at DESC
                LIMIT $2
                OFFSET $3
            "#,
            definition_id,
            i64::from(pagination.limit),
            i64::from(pagination.offset)
        )
        .fetch_all(&self.state.pool);

        let count_future = sqlx::query_scalar!(
            r#"
                SELECT COUNT(*)
                FROM quotation
                WHERE definition = $1
            "#,
            definition_id
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

crate::util::repo_from_parts!(QuotationRepository);
