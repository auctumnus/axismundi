use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;
use validator::Validate;

use crate::err::{AppResult, not_found};

use super::language_invite::PermissionLevel;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct LanguagePermission {
    pub id: Uuid,
    pub language: Uuid,
    pub user: Uuid,
    pub permission: PermissionLevel,
    pub via: Option<Uuid>,
    pub invited_by: Uuid,
    pub invited_at: DateTime<Utc>,
    pub accepted_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CreateLanguagePermission {
    pub language: Uuid,
    pub user: Uuid,
    pub permission: PermissionLevel,
    pub via: Option<Uuid>,
}

pub struct LanguagePermissionRepository {
    pool: PgPool,
}

impl LanguagePermissionRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn create(
        &self,
        permission: CreateLanguagePermission,
        invited_by: Uuid,
    ) -> AppResult<LanguagePermission> {
        permission.validate()?;

        let result = sqlx::query_as!(
            LanguagePermission,
            r#"
                INSERT INTO language_permissions (language, "user", permission, via, invited_by)
                VALUES ($1, $2, $3, $4, $5)
                RETURNING id, language, "user", permission as "permission: PermissionLevel", via, invited_by, invited_at, accepted_at
            "#,
            permission.language,
            permission.user,
            permission.permission as PermissionLevel,
            permission.via,
            invited_by
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(result)
    }

    pub async fn find_by_id(&self, id: Uuid) -> AppResult<LanguagePermission> {
        let result = sqlx::query_as!(
            LanguagePermission,
            r#"SELECT id, language, "user", permission as "permission: PermissionLevel", via, invited_by, invited_at, accepted_at FROM language_permissions WHERE id = $1"#,
            id
        )
        .fetch_optional(&self.pool)
        .await?;

        result.ok_or_else(|| not_found(format!("language permission with id '{id}'")))
    }

    pub async fn find_by_user_and_language(
        &self,
        user: Uuid,
        language: Uuid,
    ) -> AppResult<Option<LanguagePermission>> {
        let result = sqlx::query_as!(
            LanguagePermission,
            r#"SELECT id, language, "user", permission as "permission: PermissionLevel", via, invited_by, invited_at, accepted_at FROM language_permissions WHERE "user" = $1 AND language = $2"#,
            user,
            language
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(result)
    }

    pub async fn list_by_user(&self, user: Uuid) -> AppResult<Vec<LanguagePermission>> {
        let result = sqlx::query_as!(
            LanguagePermission,
            r#"SELECT id, language, "user", permission as "permission: PermissionLevel", via, invited_by, invited_at, accepted_at FROM language_permissions WHERE "user" = $1 ORDER BY invited_at DESC"#,
            user
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(result)
    }

    pub async fn list_by_language(&self, language: Uuid) -> AppResult<Vec<LanguagePermission>> {
        let result = sqlx::query_as!(
            LanguagePermission,
            r#"SELECT id, language, "user", permission as "permission: PermissionLevel", via, invited_by, invited_at, accepted_at FROM language_permissions WHERE language = $1 ORDER BY invited_at DESC"#,
            language
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(result)
    }

    pub async fn update_permission(
        &self,
        id: Uuid,
        new_permission: PermissionLevel,
    ) -> AppResult<LanguagePermission> {
        let result = sqlx::query_as!(
            LanguagePermission,
            r#"
                UPDATE language_permissions
                SET permission = $2
                WHERE id = $1
                RETURNING id, language, "user", permission as "permission: PermissionLevel", via, invited_by, invited_at, accepted_at
            "#,
            id,
            new_permission as PermissionLevel
        )
        .fetch_optional(&self.pool)
        .await?;

        result.ok_or_else(|| not_found(format!("language permission with id '{id}'")))
    }

    pub async fn accept(&self, id: Uuid) -> AppResult<LanguagePermission> {
        let result = sqlx::query_as!(
            LanguagePermission,
            r#"
                UPDATE language_permissions
                SET accepted_at = CURRENT_TIMESTAMP
                WHERE id = $1 AND accepted_at IS NULL
                RETURNING id, language, "user", permission as "permission: PermissionLevel", via, invited_by, invited_at, accepted_at
            "#,
            id
        )
        .fetch_optional(&self.pool)
        .await?;

        result.ok_or_else(|| not_found(format!("pending language permission with id '{id}'")))
    }

    pub async fn delete(&self, id: Uuid) -> AppResult<bool> {
        let result = sqlx::query!("DELETE FROM language_permissions WHERE id = $1", id)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }
}

crate::util::repo_from_parts!(LanguagePermissionRepository);
