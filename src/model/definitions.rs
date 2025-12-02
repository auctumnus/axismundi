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
pub struct Definition {
    pub id: Uuid,
    #[serde(skip_serializing)]
    pub word: Uuid,
    pub definition: String,
    pub context: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing)]
    pub created_by: Uuid,
    #[serde(skip_serializing)]
    pub updated_by: Uuid,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CreateDefinition {
    #[validate(length(min = 1, max = 10000))]
    pub definition: String,

    #[validate(length(max = 10000))]
    pub context: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct UpdateDefinition {
    #[validate(length(min = 1, max = 10000))]
    pub definition: Option<String>,

    #[validate(length(max = 10000))]
    pub context: Option<String>,
}

pub struct DefinitionRepository {
    state: AppState,
}

impl DefinitionRepository {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    pub async fn find_by_id(&self, id: Uuid) -> AppResult<Definition> {
        let result = sqlx::query_as!(
            Definition,
            r#"
                SELECT
                    id,
                    word,
                    definition,
                    context,
                    created_at,
                    updated_at,
                    created_by,
                    updated_by
                FROM definitions
                WHERE id = $1
            "#,
            id
        )
        .fetch_optional(&self.state.pool)
        .await?;

        result.ok_or_else(|| not_found(format!("definition with id '{id}'")))
    }

    pub async fn create(
        &self,
        requestor: &User,
        word_id: Uuid,
        definition: CreateDefinition,
    ) -> AppResult<Definition> {
        definition.validate()?;

        ensure_verified(requestor)?;

        // Get the word to find its language
        let word = crate::model::words::WordRepository::new(self.state.clone())
            .find_by_id(word_id)
            .await?;

        // Check permissions
        let permissions = crate::model::language_permissions::LanguagePermissionRepository::new(
            self.state.clone(),
        );
        let user_perm = permissions
            .find_by_user_and_language(requestor.id, word.language)
            .await?;

        let Some(perm) = user_perm else {
            return Err(bad_request("you don't have permission to create definitions"));
        };

        if perm.permission == PermissionLevel::Viewer {
            return Err(bad_request("viewers cannot create definitions"));
        }

        let result = sqlx::query_as!(
            Definition,
            r#"
                INSERT INTO definitions (word, definition, context, created_by, updated_by)
                VALUES ($1, $2, $3, $4, $4)
                RETURNING id, word, definition, context, created_at, updated_at, created_by, updated_by
            "#,
            word_id,
            definition.definition,
            definition.context,
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
        updates: UpdateDefinition,
    ) -> AppResult<Definition> {
        updates.validate()?;

        ensure_verified(requestor)?;

        // Get the definition to find its word and language
        let existing = self.find_by_id(id).await?;
        let word = crate::model::words::WordRepository::new(self.state.clone())
            .find_by_id(existing.word)
            .await?;

        // Check permissions
        let permissions = crate::model::language_permissions::LanguagePermissionRepository::new(
            self.state.clone(),
        );
        let user_perm = permissions
            .find_by_user_and_language(requestor.id, word.language)
            .await?;

        let Some(perm) = user_perm else {
            return Err(bad_request("you don't have permission to edit definitions"));
        };

        if perm.permission == PermissionLevel::Viewer {
            return Err(bad_request("viewers cannot edit definitions"));
        }

        let result = sqlx::query_as!(
            Definition,
            r#"
                UPDATE definitions
                SET definition = COALESCE($2, definition),
                    context = COALESCE($3, context),
                    updated_by = $4,
                    updated_at = CURRENT_TIMESTAMP
                WHERE id = $1
                RETURNING id, word, definition, context, created_at, updated_at, created_by, updated_by
            "#,
            id,
            updates.definition,
            updates.context,
            requestor.id
        )
        .fetch_optional(&self.state.pool)
        .await?;

        result.ok_or_else(|| not_found(format!("definition with id '{id}'")))
    }

    pub async fn delete(&self, requestor: &User, id: Uuid) -> AppResult<bool> {
        ensure_verified(requestor)?;

        // Get the definition to find its word and language
        let existing = self.find_by_id(id).await?;
        let word = crate::model::words::WordRepository::new(self.state.clone())
            .find_by_id(existing.word)
            .await?;

        // Check permissions
        let permissions = crate::model::language_permissions::LanguagePermissionRepository::new(
            self.state.clone(),
        );
        let user_perm = permissions
            .find_by_user_and_language(requestor.id, word.language)
            .await?;

        let Some(perm) = user_perm else {
            return Err(bad_request("you don't have permission to delete definitions"));
        };

        if perm.permission == PermissionLevel::Viewer {
            return Err(bad_request("viewers cannot delete definitions"));
        }

        let result = sqlx::query!("DELETE FROM definitions WHERE id = $1", id)
            .execute(&self.state.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn list_by_word(
        &self,
        word_id: Uuid,
        pagination: PaginatedRequest,
    ) -> AppResult<PaginatedResponse<Definition>> {
        let items_future = sqlx::query_as!(
            Definition,
            r#"
                SELECT
                    id,
                    word,
                    definition,
                    context,
                    created_at,
                    updated_at,
                    created_by,
                    updated_by
                FROM definitions
                WHERE word = $1
                ORDER BY created_at DESC
                LIMIT $2
                OFFSET $3
            "#,
            word_id,
            i64::from(pagination.limit),
            i64::from(pagination.offset)
        )
        .fetch_all(&self.state.pool);

        let count_future = sqlx::query_scalar!(
            r#"
                SELECT COUNT(*)
                FROM definitions
                WHERE word = $1
            "#,
            word_id
        )
        .fetch_one(&self.state.pool);

        let (items, total_count) = tokio::try_join!(items_future, count_future)?;

        let total = total_count.unwrap_or(0);
        let has_more = (i64::from(pagination.offset) + i64::try_from(items.len()).unwrap_or(i64::MAX)) < total;

        println!("items: {:?}", items);

        Ok(PaginatedResponse {
            items,
            total,
            offset: pagination.offset,
            limit: pagination.limit,
            has_more,
        })
    }

    pub async fn get_first_by_word(&self, word_id: &Uuid) -> AppResult<Option<Definition>> {
        let result = sqlx::query_as!(
            Definition,
            r#"
                SELECT
                    id,
                    word,
                    definition,
                    context,
                    created_at,
                    updated_at,
                    created_by,
                    updated_by
                FROM definitions
                WHERE word = $1
                ORDER BY created_at ASC
                LIMIT 1
            "#,
            word_id
        )
        .fetch_optional(&self.state.pool)
        .await?;

        Ok(result)
    }
}

crate::util::repo_from_parts!(DefinitionRepository);
