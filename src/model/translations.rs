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
pub struct Translation {
    pub id: Uuid,
    #[serde(skip_serializing)]
    pub translatable: Uuid,
    #[serde(skip_serializing)]
    pub language: Uuid,
    pub translated_text: String,
    pub translator_name: Option<String>,
    pub translator_url: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing)]
    pub created_by: Uuid,
    #[serde(skip_serializing)]
    pub updated_by: Uuid,

    pub translatable_slug: String,
    pub translatable_title: String
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CreateTranslation {
    #[validate(length(min = 1, max = 100000))]
    pub translated_text: String,

    #[validate(length(max = 1000))]
    pub translator_name: Option<String>,

    #[validate(url)]
    #[validate(length(max = 2000))]
    pub translator_url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct UpdateTranslation {
    #[validate(length(min = 1, max = 100000))]
    pub translated_text: Option<String>,

    #[validate(length(max = 1000))]
    pub translator_name: Option<String>,

    #[validate(url)]
    #[validate(length(max = 2000))]
    pub translator_url: Option<String>,
}

pub struct TranslationRepository {
    state: AppState,
}

impl TranslationRepository {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    pub async fn find_by_id(&self, id: Uuid) -> AppResult<Translation> {
        let result = sqlx::query_as!(
            Translation,
            r#"
                SELECT
                    t.id,
                    t.translatable,
                    t.language,
                    t.translated_text,
                    t.translator_name,
                    t.translator_url,
                    t.created_at,
                    t.updated_at,
                    t.created_by,
                    t.updated_by,
                    tr.slug as translatable_slug,
                    tr.title as translatable_title
                FROM translation t
                JOIN translatable tr ON t.translatable = tr.id
                WHERE t.id = $1
            "#,
            id
        )
        .fetch_optional(&self.state.pool)
        .await?;

        result.ok_or_else(|| not_found(format!("translation with id '{id}'")))
    }

    pub async fn find_by_translatable_and_language(
        &self,
        translatable_id: Uuid,
        language_id: Uuid,
    ) -> AppResult<Option<Translation>> {
        let result = sqlx::query_as!(
            Translation,
            r#"
                SELECT
                    t.id,
                    t.translatable,
                    t.language,
                    t.translated_text,
                    t.translator_name,
                    t.translator_url,
                    t.created_at,
                    t.updated_at,
                    t.created_by,
                    t.updated_by,
                    tr.slug as translatable_slug,
                    tr.title as translatable_title
                FROM translation t
                JOIN translatable tr ON t.translatable = tr.id
                WHERE t.translatable = $1 AND t.language = $2
            "#,
            translatable_id,
            language_id
        )
        .fetch_optional(&self.state.pool)
        .await?;

        Ok(result)
    }

    pub async fn create(
        &self,
        requestor: &User,
        translatable_id: Uuid,
        language_id: Uuid,
        translation: CreateTranslation,
    ) -> AppResult<Translation> {
        translation.validate()?;

        ensure_verified(requestor)?;

        // Check permissions for the language
        let permissions = crate::model::language_permissions::LanguagePermissionRepository::new(
            self.state.clone(),
        );
        let user_perm = permissions
            .find_by_user_and_language(requestor.id, language_id)
            .await?;

        let Some(perm) = user_perm else {
            return Err(bad_request("you don't have permission to create translations"));
        };

        if perm.permission == PermissionLevel::Viewer {
            return Err(bad_request("viewers cannot create translations"));
        }

        // Verify the translatable exists
        crate::model::translatable::TranslatableRepository::new(self.state.clone())
            .find_by_id(translatable_id)
            .await?;

        if let Some(_) = self
            .find_by_translatable_and_language(translatable_id, language_id)
            .await?
        {
            return Err(bad_request("a translation for this translatable in this language already exists"));
        }

        let result = sqlx::query_as!(
            Translation,
            r#"
                WITH inserted AS (
                    INSERT INTO translation (translatable, language, translated_text, translator_name, translator_url, created_by, updated_by)
                    VALUES ($1, $2, $3, $4, $5, $6, $6)
                    RETURNING id, translatable, language, translated_text, translator_name, translator_url, created_at, updated_at, created_by, updated_by
                )
                SELECT
                    i.id,
                    i.translatable,
                    i.language,
                    i.translated_text,
                    i.translator_name,
                    i.translator_url,
                    i.created_at,
                    i.updated_at,
                    i.created_by,
                    i.updated_by,
                    tr.slug as translatable_slug,
                    tr.title as translatable_title
                FROM inserted i
                JOIN translatable tr ON i.translatable = tr.id
            "#,
            translatable_id,
            language_id,
            translation.translated_text,
            translation.translator_name,
            translation.translator_url,
            requestor.id
        )
        .fetch_one(&self.state.pool)
        .await?;

        // Create activity if language is public
        let lang_repo = crate::model::languages::LanguageRepository::new(self.state.clone());
        let lang = lang_repo.find_by_id(language_id).await?;
        if !lang.private {
            let activity_repo = crate::model::user_activities::UserActivityRepository::new(self.state.clone());
            let _activity = activity_repo.create(
                requestor.id,
                crate::model::user_activities::ActivityType::CreateTranslation,
                result.id,
                "translation",
                Some(language_id),
                Some("language"),
            ).await?;
        }

        Ok(result)
    }

    pub async fn update(
        &self,
        requestor: &User,
        id: Uuid,
        updates: UpdateTranslation,
    ) -> AppResult<Translation> {
        updates.validate()?;

        ensure_verified(requestor)?;

        // Get the translation to find its language
        let existing = self.find_by_id(id).await?;

        // Check permissions
        let permissions = crate::model::language_permissions::LanguagePermissionRepository::new(
            self.state.clone(),
        );
        let user_perm = permissions
            .find_by_user_and_language(requestor.id, existing.language)
            .await?;

        let Some(perm) = user_perm else {
            return Err(bad_request("you don't have permission to edit translations"));
        };

        if perm.permission == PermissionLevel::Viewer {
            return Err(bad_request("viewers cannot edit translations"));
        }

        let result = sqlx::query_as!(
            Translation,
            r#"
                WITH updated AS (
                    UPDATE translation
                    SET translated_text = COALESCE($2, translated_text),
                        translator_name = COALESCE($3, translator_name),
                        translator_url = COALESCE($4, translator_url),
                        updated_by = $5,
                        updated_at = CURRENT_TIMESTAMP
                    WHERE id = $1
                    RETURNING id, translatable, language, translated_text, translator_name, translator_url, created_at, updated_at, created_by, updated_by
                )
                SELECT
                    u.id,
                    u.translatable,
                    u.language,
                    u.translated_text,
                    u.translator_name,
                    u.translator_url,
                    u.created_at,
                    u.updated_at,
                    u.created_by,
                    u.updated_by,
                    tr.slug as translatable_slug,
                    tr.title as translatable_title
                FROM updated u
                JOIN translatable tr ON u.translatable = tr.id
            "#,
            id,
            updates.translated_text,
            updates.translator_name,
            updates.translator_url,
            requestor.id
        )
        .fetch_optional(&self.state.pool)
        .await?;

        let updated_translation = result.ok_or_else(|| not_found(format!("translation with id '{id}'")))?;

        // Create activity if language is public
        let lang_repo = crate::model::languages::LanguageRepository::new(self.state.clone());
        let lang = lang_repo.find_by_id(existing.language).await?;
        if !lang.private {
            let activity_repo = crate::model::user_activities::UserActivityRepository::new(self.state.clone());
            let _activity = activity_repo.create(
                requestor.id,
                crate::model::user_activities::ActivityType::UpdateTranslation,
                updated_translation.id,
                "translation",
                Some(existing.language),
                Some("language"),
            ).await?;
        }

        Ok(updated_translation)
    }

    pub async fn delete(&self, requestor: &User, id: Uuid) -> AppResult<bool> {
        ensure_verified(requestor)?;

        // Get the translation to find its language
        let existing = self.find_by_id(id).await?;

        // Check permissions
        let permissions = crate::model::language_permissions::LanguagePermissionRepository::new(
            self.state.clone(),
        );
        let user_perm = permissions
            .find_by_user_and_language(requestor.id, existing.language)
            .await?;

        let Some(perm) = user_perm else {
            return Err(bad_request("you don't have permission to delete translations"));
        };

        if perm.permission == PermissionLevel::Viewer {
            return Err(bad_request("viewers cannot delete translations"));
        }

        let result = sqlx::query!("DELETE FROM translation WHERE id = $1", id)
            .execute(&self.state.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn list_by_translatable(
        &self,
        translatable_id: Uuid,
        pagination: PaginatedRequest,
    ) -> AppResult<PaginatedResponse<Translation>> {
        let items_future = sqlx::query_as!(
            Translation,
            r#"
                SELECT
                    t.id,
                    t.translatable,
                    t.language,
                    t.translated_text,
                    t.translator_name,
                    t.translator_url,
                    t.created_at,
                    t.updated_at,
                    t.created_by,
                    t.updated_by,
                    tr.slug as translatable_slug,
                    tr.title as translatable_title
                FROM translation t
                JOIN translatable tr ON t.translatable = tr.id
                WHERE t.translatable = $1
                ORDER BY t.created_at DESC
                LIMIT $2
                OFFSET $3
            "#,
            translatable_id,
            i64::from(pagination.limit),
            i64::from(pagination.offset)
        )
        .fetch_all(&self.state.pool);

        let count_future = sqlx::query_scalar!(
            r#"
                SELECT COUNT(*)
                FROM translation
                WHERE translatable = $1
            "#,
            translatable_id
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

    pub async fn list_by_language(
        &self,
        language_id: Uuid,
        pagination: PaginatedRequest,
    ) -> AppResult<PaginatedResponse<Translation>> {
        let items_future = sqlx::query_as!(
            Translation,
            r#"
                SELECT
                    t.id,
                    t.translatable,
                    t.language,
                    t.translated_text,
                    t.translator_name,
                    t.translator_url,
                    t.created_at,
                    t.updated_at,
                    t.created_by,
                    t.updated_by,
                    tr.slug as translatable_slug,
                    tr.title as translatable_title
                FROM translation t
                JOIN translatable tr ON t.translatable = tr.id
                WHERE t.language = $1
                ORDER BY t.created_at DESC
                LIMIT $2
                OFFSET $3
            "#,
            language_id,
            i64::from(pagination.limit),
            i64::from(pagination.offset)
        )
        .fetch_all(&self.state.pool);

        let count_future = sqlx::query_scalar!(
            r#"
                SELECT COUNT(*)
                FROM translation
                WHERE language = $1
            "#,
            language_id
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

crate::util::repo_from_parts!(TranslationRepository);
