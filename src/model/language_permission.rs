use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use validator::Validate;

use crate::err::{AppResult, bad_request, not_found};
use crate::model::user::User;
use crate::util::{AppState, ensure_verified};

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
    state: AppState,
}

impl LanguagePermissionRepository {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    pub async fn create(
        &self,
        permission: CreateLanguagePermission,
        invited_by: Uuid,
    ) -> AppResult<LanguagePermission> {
        permission.validate()?;

        let tx = &mut self.state.pool.begin().await?;

        self.create_by_tx(tx, permission, invited_by).await
    }

    pub async fn create_by_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        permission: CreateLanguagePermission,
        invited_by: Uuid,
    ) -> AppResult<LanguagePermission> {
        permission.validate()?;

        let result = sqlx::query_as!(
            LanguagePermission,
            r#"
                INSERT INTO language_permissions (language, "user", permission, via, invited_by, invited_at)
                VALUES ($1, $2, $3, $4, $5, CURRENT_TIMESTAMP)
                RETURNING id, language, "user", permission as "permission: PermissionLevel", via, invited_by, invited_at, accepted_at
            "#,
            permission.language,
            permission.user,
            permission.permission as PermissionLevel,
            permission.via,
            invited_by
        )
        .fetch_one(&mut **tx)
        .await?;

        Ok(result)
    }

    pub async fn find_by_id(&self, id: Uuid) -> AppResult<LanguagePermission> {
        let result = sqlx::query_as!(
            LanguagePermission,
            r#"SELECT id, language, "user", permission as "permission: PermissionLevel", via, invited_by, invited_at, accepted_at FROM language_permissions WHERE id = $1"#,
            id
        )
        .fetch_optional(&self.state.pool)
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
        .fetch_optional(&self.state.pool)
        .await?;

        Ok(result)
    }

    pub async fn list_by_user(&self, user: Uuid) -> AppResult<Vec<LanguagePermission>> {
        let result = sqlx::query_as!(
            LanguagePermission,
            r#"SELECT id, language, "user", permission as "permission: PermissionLevel", via, invited_by, invited_at, accepted_at FROM language_permissions WHERE "user" = $1 ORDER BY invited_at DESC"#,
            user
        )
        .fetch_all(&self.state.pool)
        .await?;

        Ok(result)
    }

    pub async fn list_by_language(&self, language: Uuid) -> AppResult<Vec<LanguagePermission>> {
        let result = sqlx::query_as!(
            LanguagePermission,
            r#"SELECT id, language, "user", permission as "permission: PermissionLevel", via, invited_by, invited_at, accepted_at FROM language_permissions WHERE language = $1 ORDER BY invited_at DESC"#,
            language
        )
        .fetch_all(&self.state.pool)
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
        .fetch_optional(&self.state.pool)
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
        .fetch_optional(&self.state.pool)
        .await?;

        result.ok_or_else(|| not_found(format!("pending language permission with id '{id}'")))
    }

    pub async fn delete(&self, id: Uuid) -> AppResult<bool> {
        let result = sqlx::query!("DELETE FROM language_permissions WHERE id = $1", id)
            .execute(&self.state.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    // permission-checked methods for use by controllers

    pub async fn list_by_language_checked(
        &self,
        requestor: &User,
        language: Uuid,
    ) -> AppResult<Vec<LanguagePermission>> {
        ensure_verified(requestor)?;

        let user_perm = self
            .find_by_user_and_language(requestor.id, language)
            .await?;

        let Some(perm) = user_perm else {
            return Err(bad_request("you don't have permission to view permissions"));
        };

        if perm.permission != PermissionLevel::Owner && perm.permission != PermissionLevel::Admin {
            return Err(bad_request(
                "only owners and admins can view all permissions",
            ));
        }

        self.list_by_language(language).await
    }

    pub async fn find_by_user_and_language_checked(
        &self,
        requestor: &User,
        language: Uuid,
        target_user: Uuid,
    ) -> AppResult<LanguagePermission> {
        ensure_verified(requestor)?;

        let user_perm = self
            .find_by_user_and_language(requestor.id, language)
            .await?;

        let Some(_) = user_perm else {
            return Err(bad_request("you don't have permission to view permissions"));
        };

        let target_perm = self
            .find_by_user_and_language(target_user, language)
            .await?;

        target_perm.ok_or_else(|| not_found("permission for user on language"))
    }

    #[allow(clippy::match_same_arms)] // it's more readable this way
    pub async fn update_permission_checked(
        &self,
        requestor: &User,
        target_perm_id: Uuid,
        new_permission: PermissionLevel,
    ) -> AppResult<LanguagePermission> {
        ensure_verified(requestor)?;

        let target = self.find_by_id(target_perm_id).await?;
        let requestor_perm = self
            .find_by_user_and_language(requestor.id, target.language)
            .await?;

        let Some(requestor) = requestor_perm else {
            return Err(bad_request("you don't have permission to edit permissions"));
        };

        // check permission table from api.md
        let can_edit = match (requestor.permission, target.permission) {
            (PermissionLevel::Owner, PermissionLevel::Owner) => false,
            (PermissionLevel::Owner, _) => true,
            (PermissionLevel::Admin, PermissionLevel::Editor) => true,
            _ => false,
        };

        if !can_edit {
            return Err(bad_request(
                "you don't have permission to edit this user's permissions",
            ));
        }

        self.update_permission(target_perm_id, new_permission).await
    }

    #[allow(clippy::match_same_arms)] // it's more readable this way
    pub async fn delete_checked(&self, requestor: &User, target_perm_id: Uuid) -> AppResult<bool> {
        ensure_verified(requestor)?;

        let target = self.find_by_id(target_perm_id).await?;
        let requestor_perm = self
            .find_by_user_and_language(requestor.id, target.language)
            .await?;

        let Some(requestor_perm) = requestor_perm else {
            return Err(bad_request(
                "you don't have permission to delete permissions",
            ));
        };

        // check if removing own permissions (always allowed except owner)
        if requestor.id == target.user {
            if requestor_perm.permission == PermissionLevel::Owner {
                return Err(bad_request("owner cannot remove their own permissions"));
            }
            return self.delete(target_perm_id).await;
        }

        // check permission table from api.md
        let can_delete = match (requestor_perm.permission, target.permission) {
            (PermissionLevel::Owner, PermissionLevel::Owner) => false,
            (PermissionLevel::Owner, _) => true,
            (PermissionLevel::Admin, PermissionLevel::Editor) => true,
            _ => false,
        };

        if !can_delete {
            return Err(bad_request(
                "you don't have permission to delete this user's permissions",
            ));
        }

        self.delete(target_perm_id).await
    }
}

crate::util::repo_from_parts!(LanguagePermissionRepository);
