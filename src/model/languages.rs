use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use validator::Validate;

use crate::{
    err::{AppResult, bad_request, forbidden, not_found},
    model::{language_invites::PermissionLevel, users::{User, UserSearch}},
    pagination::{PaginatedRequest, PaginatedResponse},
    util::{AppState, ensure_verified},
};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Language {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    pub description: String,
    pub private: bool,
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

    #[serde(default)] // i.e. false
    pub private: bool,

    #[serde(default)]
    #[validate(length(max = 5000))]
    pub description: String,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct UpdateLanguage {
    #[validate(length(min = 2, max = 10))]
    pub code: Option<String>,

    #[serde(default)]
    pub private: Option<bool>,

    #[validate(length(min = 2, max = 100))]
    pub name: Option<String>,

    #[validate(length(max = 5000))]
    pub description: Option<String>,
}

pub struct LanguageRepository {
    state: AppState,
}

impl LanguageRepository {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    pub async fn create(&self, requestor: &User, language: CreateLanguage) -> AppResult<Language> {
        language.validate()?;

        ensure_verified(requestor)?;

        if self.code_exists(&language.code).await? {
            return Err(bad_request("language code is already in use"));
        }

        if &language.code == "search" {
            return Err(bad_request("cannot use 'search' as language code"));
        }

        let mut tx = self.state.pool.begin().await?;

        let result = sqlx::query_as!(
            Language,
            r#"
                INSERT INTO languages (code, name, private, description, created_by, updated_by)
                VALUES ($1, $2, $3, $4, $5, $5)
                RETURNING *
            "#,
            language.code,
            language.name,
            language.private,
            language.description,
            requestor.id
        )
        .fetch_one(&mut *tx)
        .await?;

        crate::model::language_permissions::LanguagePermissionRepository::new(self.state.clone())
            .create_by_tx(
                &mut tx,
                crate::model::language_permissions::CreateLanguagePermission {
                    language: result.id,
                    user: requestor.id,
                    permission: PermissionLevel::Owner,
                    via: None,
                },
                requestor.id,
            )
            .await?;

        tx.commit().await?;

        Ok(result)
    }

    pub async fn find_by_code(&self, code: &str) -> AppResult<Language> {
        let result = sqlx::query_as!(Language, "SELECT * FROM languages WHERE code = $1", code)
            .fetch_optional(&self.state.pool)
            .await?;

        result.ok_or_else(|| not_found(format!("language with code '{code}'")))
    }

    pub async fn update(
        &self,
        requestor: &User,
        id: Uuid,
        updates: UpdateLanguage,
    ) -> AppResult<Language> {
        updates.validate()?;

        ensure_verified(requestor)?;

        let permissions = crate::model::language_permissions::LanguagePermissionRepository::new(
            self.state.clone(),
        );
        let user_perm = permissions
            .find_by_user_and_language(requestor.id, id)
            .await?;
        let Some(perm) = user_perm else {
            return Err(forbidden("you don't have permission to edit this language"));
        };

        if perm.permission == PermissionLevel::Viewer {
            return Err(forbidden("viewers cannot edit language"));
        }

        if let Some(code) = &updates.code {
            if code == "search" {
                return Err(bad_request("cannot use 'search' as language code"));
            }
        }

        if updates.private.is_some() && perm.permission != PermissionLevel::Owner {
            return Err(forbidden("only owners can change language privacy"));
        }

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
            requestor.id
        )
        .fetch_optional(&self.state.pool)
        .await?;

        result.ok_or_else(|| not_found(format!("language with id '{id}'")))
    }

    pub async fn delete(&self, requestor: &User, id: Uuid) -> AppResult<bool> {
        tracing::debug!("Deleting language {id}");
        ensure_verified(requestor)?;

        let permissions = crate::model::language_permissions::LanguagePermissionRepository::new(
            self.state.clone(),
        );
        let user_perm = permissions
            .find_by_user_and_language(requestor.id, id)
            .await?;
        let Some(perm) = user_perm else {
            return Err(forbidden(
                "you don't have permission to delete this language",
            ));
        };
        if perm.permission != PermissionLevel::Owner {
            return Err(forbidden("only owners can delete languages"));
        }

        let result = sqlx::query!("DELETE FROM languages WHERE id = $1", id)
            .execute(&self.state.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn code_exists(&self, code: &str) -> AppResult<bool> {
        let result = sqlx::query!("SELECT 1 as exists FROM languages WHERE code = $1", code)
            .fetch_optional(&self.state.pool)
            .await?;

        Ok(result.is_some())
    }

    pub async fn search(&self, pagination: PaginatedRequest, search: LanguageSearch) -> AppResult<PaginatedResponse<Language>> {
        // search strategy:
        // - exact matches in name, code, owner are weighted highly
        // - otherwise, we use similarity on code, name, description
        // - TODO: deal with private languages and edited_by filter

        let items_future = sqlx::query_as!(
            Language,
            r#"
                SELECT languages.*
                FROM languages
                JOIN users ON users.id = languages.created_by
                WHERE
                ($1::TEXT IS NULL OR users.username = $1)
                AND ($2::TIMESTAMPTZ IS NULL OR languages.created_at < $2)
                AND ($3::TIMESTAMPTZ IS NULL OR languages.created_at > $3)
                ORDER BY (
                    CASE
                        WHEN $4::TEXT IS NOT NULL AND languages.name ILIKE '%' || $4 || '%' THEN 100.0
                        WHEN $4::TEXT IS NOT NULL AND languages.code ILIKE '%' || $4 || '%' THEN 90.0
                        WHEN $4::TEXT IS NOT NULL AND languages.description ILIKE '%' || $4 || '%' THEN 80.0
                        WHEN $4::TEXT IS NOT NULL AND users.username ILIKE '%' || $4 || '%' THEN 70.0
                        ELSE 0.0
                    END +
                    CASE WHEN $4::TEXT IS NOT NULL THEN
                        similarity(languages.name, $4) * 3.0 +
                        similarity(languages.code, $4) * 2.0 +
                        similarity(languages.description, $4) * 1.0
                    ELSE 0.0
                    END
                ) DESC, languages.id
                LIMIT $5
                OFFSET $6
            "#,
            search.owned_by,
            search.created_before,
            search.created_after,
            search.text_query,
            pagination.limit as i64,
            pagination.offset as i64
        )
        .fetch_all(&self.state.pool);

        let count_future = sqlx::query_scalar!(
            r#"
                SELECT COUNT(*)
                FROM languages
                JOIN users ON users.id = languages.created_by
                WHERE
                ($1::TEXT IS NULL OR users.username = $1)
                AND ($2::TIMESTAMPTZ IS NULL OR languages.created_at < $2)
                AND ($3::TIMESTAMPTZ IS NULL OR languages.created_at > $3)
            "#,
            search.owned_by,
            search.created_before,
            search.created_after
        )
        .fetch_one(&self.state.pool);

        let (items, total_count) = tokio::try_join!(items_future, count_future)?;

        let total = total_count.unwrap_or(0);
        let has_more = (pagination.offset as i64 + items.len() as i64) < total;

        Ok(PaginatedResponse {
            items,
            total,
            offset: pagination.offset,
            limit: pagination.limit,
            has_more,
        })
    }

    pub async fn search_editors_of_language(
        &self,
        language_id: Uuid,
        pagination: PaginatedRequest,
        search: UserSearch,
    ) -> AppResult<PaginatedResponse<User>> {
        // search strategy:
        // - exact matches in username, display_name are weighted highly
        // - otherwise, we use similarity on username, display_name, description
        // - filter by verified status if specified
        // - must have at least editor permissions (not viewer)

        let items_future = sqlx::query_as!(
            User,
            r#"
                SELECT users.*
                FROM users
                JOIN language_permissions ON language_permissions.user = users.id
                WHERE
                language_permissions.language = $1
                AND language_permissions.permission IN ('editor', 'admin', 'owner')
                AND ($2::BOOL IS NULL OR (users.verified_at IS NOT NULL) = $2)
                AND ($3::TIMESTAMPTZ IS NULL OR users.created_at < $3)
                AND ($4::TIMESTAMPTZ IS NULL OR users.created_at > $4)
                ORDER BY (
                    CASE
                        WHEN $5::TEXT IS NOT NULL AND users.username ILIKE '%' || $5 || '%' THEN 100.0
                        WHEN $5::TEXT IS NOT NULL AND users.display_name ILIKE '%' || $5 || '%' THEN 90.0
                        WHEN $5::TEXT IS NOT NULL AND users.description ILIKE '%' || $5 || '%' THEN 80.0
                        ELSE 0.0
                    END +
                    CASE WHEN $5::TEXT IS NOT NULL THEN
                        similarity(users.username, $5) * 3.0 +
                        COALESCE(similarity(users.display_name, $5), 0.0) * 2.0 +
                        COALESCE(similarity(users.description, $5), 0.0) * 1.0
                    ELSE 0.0
                    END
                ) DESC, users.id
                LIMIT $6
                OFFSET $7
            "#,
            language_id,
            search.verified,
            search.created_before,
            search.created_after,
            search.text_query,
            pagination.limit as i64,
            pagination.offset as i64
        )
        .fetch_all(&self.state.pool);

        let count_future = sqlx::query_scalar!(
            r#"
                SELECT COUNT(*)
                FROM users
                JOIN language_permissions ON language_permissions.user = users.id
                WHERE
                language_permissions.language = $1
                AND language_permissions.permission IN ('editor', 'owner')
                AND ($2::BOOL IS NULL OR (users.verified_at IS NOT NULL) = $2)
                AND ($3::TIMESTAMPTZ IS NULL OR users.created_at < $3)
                AND ($4::TIMESTAMPTZ IS NULL OR users.created_at > $4)
            "#,
            language_id,
            search.verified,
            search.created_before,
            search.created_after
        )
        .fetch_one(&self.state.pool);

        let (items, total_count) = tokio::try_join!(items_future, count_future)?;

        let total = total_count.unwrap_or(0);
        let has_more = (pagination.offset as i64 + items.len() as i64) < total;

        Ok(PaginatedResponse {
            items,
            total,
            offset: pagination.offset,
            limit: pagination.limit,
            has_more,
        })
    }
}

#[derive(Debug)]
pub struct LanguageSearch {
    pub text_query: Option<String>,
    pub owned_by: Option<String>,
    pub edited_by: Option<Vec<String>>,
    pub created_before: Option<DateTime<Utc>>,
    pub created_after: Option<DateTime<Utc>>,
}

crate::util::repo_from_parts!(LanguageRepository);
