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
pub struct QuotationSuggestion {
    pub id: Uuid,
    #[serde(skip_serializing)]
    pub language: Uuid,
    #[serde(skip_serializing)]
    pub definition: Uuid,
    pub span_content: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing)]
    pub created_by: Uuid,
    #[serde(skip_serializing)]
    pub updated_by: Uuid,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CreateQuotationSuggestion {
    #[validate(length(min = 1, max = 10000))]
    pub span_content: String,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct UpdateQuotationSuggestion {
    #[validate(length(min = 1, max = 10000))]
    pub span_content: Option<String>,
}

pub struct QuotationSuggestionRepository {
    state: AppState,
}

impl QuotationSuggestionRepository {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    pub async fn find_by_id(&self, id: Uuid) -> AppResult<QuotationSuggestion> {
        let result = sqlx::query_as!(
            QuotationSuggestion,
            r#"
                SELECT
                    id,
                    language,
                    definition,
                    span_content,
                    created_at,
                    updated_at,
                    created_by,
                    updated_by
                FROM quotation_suggestion
                WHERE id = $1
            "#,
            id
        )
        .fetch_optional(&self.state.pool)
        .await?;

        result.ok_or_else(|| not_found(format!("quotation suggestion with id '{id}'")))
    }

    pub async fn find_by_id_with_permission_check(
        &self,
        requestor: Option<&User>,
        id: Uuid,
    ) -> AppResult<QuotationSuggestion> {
        let suggestion = self.find_by_id(id).await?;

        // Check if requestor has permission to view this suggestion
        if let Some(user) = requestor {
            let permissions = crate::model::language_permissions::LanguagePermissionRepository::new(
                self.state.clone(),
            );
            let user_perm = permissions
                .find_by_user_and_language(user.id, suggestion.language)
                .await?;

            if user_perm.is_none() {
                return Err(crate::err::forbidden("you don't have permission to view this quotation suggestion"));
            }
        } else {
            return Err(crate::err::unauthorized_no_session());
        }

        Ok(suggestion)
    }

    pub async fn create(
        &self,
        requestor: &User,
        language_id: Uuid,
        definition_id: Uuid,
        suggestion: CreateQuotationSuggestion,
    ) -> AppResult<QuotationSuggestion> {
        suggestion.validate()?;

        ensure_verified(requestor)?;

        // Verify the definition exists and get its word
        let definition = crate::model::definitions::DefinitionRepository::new(self.state.clone())
            .find_by_id(definition_id)
            .await?;
        let word = crate::model::words::WordRepository::new(self.state.clone())
            .find_by_id(definition.word)
            .await?;

        // Verify the language matches the word's language
        if word.language != language_id {
            return Err(bad_request("language must match the word's language"));
        }

        // Check permissions
        let permissions = crate::model::language_permissions::LanguagePermissionRepository::new(
            self.state.clone(),
        );
        let user_perm = permissions
            .find_by_user_and_language(requestor.id, language_id)
            .await?;

        let Some(perm) = user_perm else {
            return Err(bad_request("you don't have permission to create quotation suggestions"));
        };

        if perm.permission == PermissionLevel::Viewer {
            return Err(bad_request("viewers cannot create quotation suggestions"));
        }

        let result = sqlx::query_as!(
            QuotationSuggestion,
            r#"
                INSERT INTO quotation_suggestion (language, definition, span_content, created_by, updated_by)
                VALUES ($1, $2, $3, $4, $4)
                RETURNING id, language, definition, span_content, created_at, updated_at, created_by, updated_by
            "#,
            language_id,
            definition_id,
            suggestion.span_content,
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
        updates: UpdateQuotationSuggestion,
    ) -> AppResult<QuotationSuggestion> {
        updates.validate()?;

        ensure_verified(requestor)?;

        // Get the suggestion and check language permissions
        let existing = self.find_by_id(id).await?;

        // Check permissions
        let permissions = crate::model::language_permissions::LanguagePermissionRepository::new(
            self.state.clone(),
        );
        let user_perm = permissions
            .find_by_user_and_language(requestor.id, existing.language)
            .await?;

        let Some(perm) = user_perm else {
            return Err(bad_request("you don't have permission to edit quotation suggestions"));
        };

        if perm.permission == PermissionLevel::Viewer {
            return Err(bad_request("viewers cannot edit quotation suggestions"));
        }

        let result = sqlx::query_as!(
            QuotationSuggestion,
            r#"
                UPDATE quotation_suggestion
                SET span_content = COALESCE($2, span_content),
                    updated_by = $3,
                    updated_at = CURRENT_TIMESTAMP
                WHERE id = $1
                RETURNING id, language, definition, span_content, created_at, updated_at, created_by, updated_by
            "#,
            id,
            updates.span_content,
            requestor.id
        )
        .fetch_optional(&self.state.pool)
        .await?;

        result.ok_or_else(|| not_found(format!("quotation suggestion with id '{id}'")))
    }

    pub async fn delete(&self, requestor: &User, id: Uuid) -> AppResult<bool> {
        ensure_verified(requestor)?;

        // Get the suggestion and check language permissions
        let existing = self.find_by_id(id).await?;

        // Check permissions
        let permissions = crate::model::language_permissions::LanguagePermissionRepository::new(
            self.state.clone(),
        );
        let user_perm = permissions
            .find_by_user_and_language(requestor.id, existing.language)
            .await?;

        let Some(perm) = user_perm else {
            return Err(bad_request("you don't have permission to delete quotation suggestions"));
        };

        if perm.permission == PermissionLevel::Viewer {
            return Err(bad_request("viewers cannot delete quotation suggestions"));
        }

        let result = sqlx::query!("DELETE FROM quotation_suggestion WHERE id = $1", id)
            .execute(&self.state.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn list_by_language(
        &self,
        requestor: Option<&User>,
        language_id: Uuid,
        pagination: PaginatedRequest,
    ) -> AppResult<PaginatedResponse<QuotationSuggestion>> {
        // Check if requestor has permission to view suggestions for this language
        if let Some(user) = requestor {
            let permissions = crate::model::language_permissions::LanguagePermissionRepository::new(
                self.state.clone(),
            );
            let user_perm = permissions
                .find_by_user_and_language(user.id, language_id)
                .await?;

            if user_perm.is_none() {
                return Err(crate::err::forbidden("you don't have permission to view quotation suggestions for this language"));
            }
        } else {
            return Err(crate::err::unauthorized_no_session());
        }
        let items_future = sqlx::query_as!(
            QuotationSuggestion,
            r#"
                SELECT
                    id,
                    language,
                    definition,
                    span_content,
                    created_at,
                    updated_at,
                    created_by,
                    updated_by
                FROM quotation_suggestion
                WHERE language = $1
                ORDER BY created_at DESC
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
                FROM quotation_suggestion
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

    pub async fn list_by_definition(
        &self,
        requestor: Option<&User>,
        definition_id: Uuid,
        pagination: PaginatedRequest,
    ) -> AppResult<PaginatedResponse<QuotationSuggestion>> {
        // Get the definition to find its word and language
        let definition = crate::model::definitions::DefinitionRepository::new(self.state.clone())
            .find_by_id(definition_id)
            .await?;
        let word = crate::model::words::WordRepository::new(self.state.clone())
            .find_by_id(definition.word)
            .await?;

        // Check if requestor has permission to view suggestions for this language
        if let Some(user) = requestor {
            let permissions = crate::model::language_permissions::LanguagePermissionRepository::new(
                self.state.clone(),
            );
            let user_perm = permissions
                .find_by_user_and_language(user.id, word.language)
                .await?;

            if user_perm.is_none() {
                return Err(crate::err::forbidden("you don't have permission to view quotation suggestions for this language"));
            }
        } else {
            return Err(crate::err::unauthorized_no_session());
        }
        let items_future = sqlx::query_as!(
            QuotationSuggestion,
            r#"
                SELECT
                    id,
                    language,
                    definition,
                    span_content,
                    created_at,
                    updated_at,
                    created_by,
                    updated_by
                FROM quotation_suggestion
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
                FROM quotation_suggestion
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

crate::util::repo_from_parts!(QuotationSuggestionRepository);
