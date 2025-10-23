use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, QueryBuilder};
use uuid::Uuid;
use validator::Validate;

use crate::{
    err::{AppResult, bad_request, forbidden, not_found},
    model::{language_invite::PermissionLevel, user::User},
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

        crate::model::language_permission::LanguagePermissionRepository::new(self.state.clone())
            .create_by_tx(
                &mut tx,
                crate::model::language_permission::CreateLanguagePermission {
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

    pub async fn find_by_id(&self, id: Uuid) -> AppResult<Language> {
        let result = sqlx::query_as!(Language, "SELECT * FROM languages WHERE id = $1", id)
            .fetch_optional(&self.state.pool)
            .await?;

        result.ok_or_else(|| not_found(format!("language with id '{id}'")))
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

        let permissions = crate::model::language_permission::LanguagePermissionRepository::new(
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
        ensure_verified(requestor)?;

        let permissions = crate::model::language_permission::LanguagePermissionRepository::new(
            self.state.clone(),
        );
        let user_perm = permissions
            .find_by_user_and_language(requestor.id, id)
            .await?;
        let Some(perm) = user_perm else {
            return Err(bad_request(
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

    pub async fn list(&self, limit: i64, offset: i64) -> AppResult<Vec<Language>> {
        let result = sqlx::query_as!(
            Language,
            "SELECT * FROM languages ORDER BY created_at DESC LIMIT $1 OFFSET $2",
            limit,
            offset
        )
        .fetch_all(&self.state.pool)
        .await?;

        Ok(result)
    }

    pub async fn count(&self) -> AppResult<i64> {
        let result = sqlx::query!("SELECT COUNT(*) as count FROM languages")
            .fetch_one(&self.state.pool)
            .await?;

        Ok(result.count.unwrap_or(0))
    }

    pub async fn code_exists(&self, code: &str) -> AppResult<bool> {
        let result = sqlx::query!("SELECT 1 as exists FROM languages WHERE code = $1", code)
            .fetch_optional(&self.state.pool)
            .await?;

        Ok(result.is_some())
    }

    pub async fn list_by_user(
        &self,
        user_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> AppResult<Vec<Language>> {
        let result = sqlx::query_as!(
            Language,
            "SELECT * FROM languages WHERE created_by = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3",
            user_id,
            limit,
            offset
        )
        .fetch_all(&self.state.pool)
        .await?;

        Ok(result)
    }

    pub async fn search(&self, search: LanguageSearch) -> AppResult<PaginatedResponse<Language>> {
        use sqlx::QueryBuilder;

        let limit = search.pagination.limit + 1;
        let mut query_builder: QueryBuilder<sqlx::Postgres> =
            QueryBuilder::new("SELECT * FROM languages WHERE 1=1");

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
            .fetch_all(&self.state.pool)
            .await?;

        let has_more = items.len() > usize::try_from(search.pagination.limit).unwrap_or(0);
        if has_more {
            items.pop();
        }

        let next_cursor = if has_more {
            items.last().map(|l| l.id)
        } else {
            None
        };

        let previous_cursor = search.pagination.cursor;
        let pages_left = i32::from(has_more);

        Ok(PaginatedResponse {
            items,
            pages_left,
            next_cursor,
            previous_cursor,
        })
    }

    pub async fn editors_of_language(
        &self,
        language_id: Uuid,
        pagination: PaginatedRequest,
    ) -> AppResult<PaginatedResponse<User>> {
        let limit = pagination.limit + 1;
        let mut query_builder: QueryBuilder<sqlx::Postgres> = QueryBuilder::new(
            "SELECT u.* FROM users u \
            JOIN language_permissions lp ON u.id = lp.\"user\" \
            WHERE lp.language = ",
        );
        query_builder.push_bind(language_id);

        if let Some(cursor) = pagination.cursor {
            query_builder.push(" AND u.id > ");
            query_builder.push_bind(cursor);
        }

        query_builder.push(" ORDER BY u.created_at DESC LIMIT ");
        query_builder.push_bind(limit);

        let mut items = query_builder
            .build_query_as::<User>()
            .fetch_all(&self.state.pool)
            .await?;

        let has_more = items.len() > usize::try_from(pagination.limit).unwrap_or(0);
        if has_more {
            items.pop();
        }

        let next_cursor = if has_more {
            items.last().map(|u| u.id)
        } else {
            None
        };

        let previous_cursor = pagination.cursor;
        let pages_left = i32::from(has_more);

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
