use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::prelude::FromRow;
use uuid::Uuid;

use crate::err::{AppResult, bad_request, not_found};
use crate::model::audit_log::{AuditActionType, AuditableResource, PermissionCheck};
use crate::model::language_family_invites::LanguageFamilyInvite;
use crate::model::language_invites::PermissionLevel;
use crate::model::users::User;
use crate::pagination::{PaginatedRequest, PaginatedResponse};
use crate::util::{AppState, ensure_verified, repo_from_parts};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct LanguageFamilyPermission {
    pub id: Uuid,
    pub family: Uuid,
    pub user: Uuid,
    pub permission: PermissionLevel,
    pub via: Option<Uuid>,
    pub invited_by: Uuid,
    pub invited_at: DateTime<Utc>,
    pub accepted_at: Option<DateTime<Utc>>,
}

pub struct CreateLanguageFamilyPermission {
    pub family: Uuid,
    pub user: Uuid,
    pub permission: PermissionLevel,
    pub via: Option<Uuid>,
}

pub struct SearchLanguageFamilyPermission {
    pub permission: PermissionLevel,
    pub family: String,
}

pub struct LanguageFamilyPermissionRepository {
    state: AppState,
}

pub struct CheckPermissionReq {
    pub user: Uuid,
    pub family: Uuid,
    pub required_level: PermissionLevel,
    pub action_type: AuditActionType,
    pub resource_type: AuditableResource,
    pub resource_id: Uuid,
    pub context: Option<Value>,
}

impl LanguageFamilyPermissionRepository {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    pub async fn create_by_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        permission: CreateLanguageFamilyPermission,
        invited_by: Uuid,
    ) -> AppResult<LanguageFamilyPermission> {
        let result = sqlx::query_as!(
            LanguageFamilyPermission,
            r#"
                INSERT INTO language_family_permissions (family, "user", permission, via, invited_by, invited_at, accepted_at)
                VALUES ($1, $2, $3, $4, $5, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
                RETURNING id, family, "user", permission as "permission: PermissionLevel", via, invited_by, invited_at, accepted_at
            "#,
            permission.family,
            permission.user,
            permission.permission as PermissionLevel,
            permission.via,
            invited_by
        )
        .fetch_one(&mut **tx)
        .await?;

        Ok(result)
    }

    pub async fn count_contributors(&self, family: Uuid) -> AppResult<i64> {
        let result = sqlx::query_scalar!(
            r#"
                SELECT COUNT(*)
                FROM language_family_permissions
                WHERE family = $1
            "#,
            family
        )
        .fetch_one(&self.state.pool)
        .await?
        .unwrap_or(0);

        Ok(result)
    }

    pub async fn create_from_invite(
        &self,
        invite: &LanguageFamilyInvite,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ) -> AppResult<LanguageFamilyPermission> {
        if invite.permissions == PermissionLevel::Owner {
            // transfer ownership
            sqlx::query!(
                r#"
                    UPDATE language_family_permissions
                    SET permission = $2
                    WHERE family = $1 AND permission = $3
                "#,
                invite.family,
                PermissionLevel::Admin as PermissionLevel,
                PermissionLevel::Owner as PermissionLevel
            )
            .execute(&mut **tx)
            .await?;
        }

        let result = sqlx::query_as!(
            LanguageFamilyPermission,
            r#"
                INSERT INTO language_family_permissions (family, "user", permission, via, invited_by, invited_at, accepted_at)
                VALUES ($1, $2, $3, $4, $5, $6, $7)
                RETURNING id, family, "user", permission as "permission: PermissionLevel", via, invited_by, invited_at, accepted_at
            "#,
            invite.family,
            invite.recipient,
            invite.permissions as PermissionLevel,
            invite.id,
            invite.sender,
            invite.sent_at,
            invite.accepted_at
        )
        .fetch_one(&mut **tx)
        .await?;

        Ok(result)
    }

    pub async fn find_by_family_and_user(
        &self,
        family: Uuid,
        user: Uuid,
    ) -> AppResult<Option<LanguageFamilyPermission>> {
        let result = sqlx::query_as!(
            LanguageFamilyPermission,
            r#"
                SELECT id, family, "user", permission as "permission: PermissionLevel", via, invited_by, invited_at, accepted_at
                FROM language_family_permissions
                WHERE family = $1 AND "user" = $2
            "#,
            family,
            user
        )
        .fetch_optional(&self.state.pool)
        .await?;

        Ok(result)
    }

    pub async fn find_owner(&self, family: Uuid) -> AppResult<LanguageFamilyPermission> {
        let result = sqlx::query_as!(
            LanguageFamilyPermission,
            r#"
                SELECT id, family, "user", permission as "permission: PermissionLevel", via, invited_by, invited_at, accepted_at
                FROM language_family_permissions
                WHERE family = $1 AND permission = 'owner'
            "#,
            family
        )
        .fetch_one(&self.state.pool)
        .await?;

        Ok(result)
    }

    pub async fn search(
        &self,
        search: SearchLanguageFamilyPermission,
        pagination: PaginatedRequest,
    ) -> AppResult<PaginatedResponse<LanguageFamilyPermission>> {
        let items = sqlx::query_as!(
            LanguageFamilyPermission,
            r#"
                SELECT id, family, "user", permission as "permission: PermissionLevel", via, invited_by, invited_at, accepted_at
                FROM language_family_permissions
                WHERE permission = $1
                ORDER BY invited_at DESC
                LIMIT $2 OFFSET $3
            "#,
            search.permission as PermissionLevel,
            pagination.limit as i64,
            pagination.offset as i64
        )
        .fetch_all(&self.state.pool)
        .await?;

        let total: i64 = sqlx::query_scalar!(
            r#"
                SELECT COUNT(*)
                FROM language_family_permissions
                WHERE permission = $1
            "#,
            search.permission as PermissionLevel
        )
        .fetch_one(&self.state.pool)
        .await?
        .unwrap_or(0);

        let has_more = ((pagination.offset + pagination.limit) as i64) < total;

        Ok(PaginatedResponse {
            items,
            total,
            limit: pagination.limit,
            offset: pagination.offset,
            has_more,
        })
    }

    pub async fn has_permission(
        &self,
        user: Uuid,
        family: Uuid,
        required_level: PermissionLevel,
    ) -> AppResult<bool> {
        let result = self.find_by_family_and_user(family, user).await?;

        if let Some(permission) = result {
            Ok(permission.permission as i32 >= required_level as i32)
        } else {
            Ok(false)
        }
    }

    pub async fn check_permission_with_audit(
        &self,
        req: CheckPermissionReq,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ) -> AppResult<PermissionCheck> {
        let has_perm = self
            .has_permission(req.user, req.family, req.required_level)
            .await?;

        if has_perm {
            Ok(PermissionCheck::HasPermission)
        } else {
            // Check if user is admin/mod
            let is_admin_or_mod = crate::util::is_admin_or_mod(&self.state, req.user).await?;

            if is_admin_or_mod {
                // Create audit log
                let audit_logs =
                    crate::model::audit_log::AuditLogRepository::new(self.state.clone());
                let log_req = crate::model::audit_log::CreateAuditLog {
                    user_id: Some(req.user),
                    action: req.action_type,
                    resource_type: req.resource_type,
                    resource_id: req.resource_id,
                    details: serde_json::json!({
                        "required_permission": req.required_level,
                        "family_id": req.family,
                        "extra": req.context
                    }),
                };
                audit_logs.create_internal_tx(tx, log_req).await?;

                Ok(PermissionCheck::Audited)
            } else {
                Ok(PermissionCheck::NoPermission)
            }
        }
    }

    pub async fn find_by_id(&self, id: Uuid) -> AppResult<LanguageFamilyPermission> {
        let result = sqlx::query_as!(
            LanguageFamilyPermission,
            r#"
                SELECT id, family, "user", permission as "permission: PermissionLevel", via, invited_by, invited_at, accepted_at
                FROM language_family_permissions
                WHERE id = $1
            "#,
            id
        )
        .fetch_optional(&self.state.pool)
        .await?;

        result.ok_or_else(|| not_found(format!("language family permission with id '{id}'")))
    }

    pub async fn list_by_family(&self, family: Uuid) -> AppResult<Vec<LanguageFamilyPermission>> {
        let result = sqlx::query_as!(
            LanguageFamilyPermission,
            r#"
                SELECT id, family, "user", permission as "permission: PermissionLevel", via, invited_by, invited_at, accepted_at
                FROM language_family_permissions
                WHERE family = $1 AND accepted_at IS NOT NULL
                ORDER BY invited_at DESC
            "#,
            family
        )
        .fetch_all(&self.state.pool)
        .await?;

        Ok(result)
    }

    pub async fn update_permission(
        &self,
        id: Uuid,
        new_permission: PermissionLevel,
    ) -> AppResult<LanguageFamilyPermission> {
        let result = sqlx::query_as!(
            LanguageFamilyPermission,
            r#"
                UPDATE language_family_permissions
                SET permission = $2
                WHERE id = $1
                RETURNING id, family, "user", permission as "permission: PermissionLevel", via, invited_by, invited_at, accepted_at
            "#,
            id,
            new_permission as PermissionLevel
        )
        .fetch_optional(&self.state.pool)
        .await?;

        result.ok_or_else(|| not_found(format!("language family permission with id '{id}'")))
    }

    pub async fn delete(&self, id: Uuid) -> AppResult<bool> {
        let result = sqlx::query!("DELETE FROM language_family_permissions WHERE id = $1", id)
            .execute(&self.state.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    // permission-checked methods for use by controllers

    pub async fn list_by_family_checked(
        &self,
        requestor: &User,
        family: Uuid,
    ) -> AppResult<Vec<LanguageFamilyPermission>> {
        ensure_verified(requestor)?;

        let user_perm = self.find_by_family_and_user(family, requestor.id).await?;

        let Some(perm) = user_perm else {
            return Err(bad_request("you don't have permission to view permissions"));
        };

        if perm.permission != PermissionLevel::Owner && perm.permission != PermissionLevel::Admin {
            return Err(bad_request(
                "only owners and admins can view all permissions",
            ));
        }

        self.list_by_family(family).await
    }

    pub async fn find_by_user_and_family_checked(
        &self,
        requestor: &User,
        family: Uuid,
        target_user: Uuid,
    ) -> AppResult<LanguageFamilyPermission> {
        ensure_verified(requestor)?;

        let user_perm = self.find_by_family_and_user(family, requestor.id).await?;

        let Some(_) = user_perm else {
            return Err(bad_request("you don't have permission to view permissions"));
        };

        let target_perm = self.find_by_family_and_user(family, target_user).await?;

        target_perm.ok_or_else(|| not_found("permission for user on language family"))
    }

    #[allow(clippy::match_same_arms)]
    pub async fn update_permission_checked(
        &self,
        requestor: &User,
        target_perm_id: Uuid,
        new_permission: PermissionLevel,
    ) -> AppResult<LanguageFamilyPermission> {
        ensure_verified(requestor)?;

        crate::model::user_bans::UserBanRepository::new(self.state.clone())
            .ensure_not_banned(requestor.id)
            .await?;

        let target = self.find_by_id(target_perm_id).await?;

        // Check if requestor is admin/mod - if so, allow with audit log
        if crate::util::is_admin_or_mod(&self.state, requestor.id).await? {
            let result = self
                .update_permission(target_perm_id, new_permission)
                .await?;

            // Create audit log
            let audit_logs = crate::model::audit_log::AuditLogRepository::new(self.state.clone());
            let log_req = crate::model::audit_log::CreateAuditLog {
                user_id: Some(requestor.id),
                action: crate::model::audit_log::AuditActionType::Updated,
                resource_type: crate::model::audit_log::AuditableResource::Permission,
                resource_id: target_perm_id,
                details: serde_json::json!({
                    "family_id": target.family,
                    "target_user_id": target.user,
                    "old_permission": target.permission,
                    "new_permission": new_permission
                }),
            };
            let _ = audit_logs.create_internal(log_req).await;

            return Ok(result);
        }

        let requestor_perm = self
            .find_by_family_and_user(target.family, requestor.id)
            .await?;

        let Some(requestor_perm) = requestor_perm else {
            return Err(bad_request("you don't have permission to edit permissions"));
        };

        // check permission table
        let can_edit = match (requestor_perm.permission, target.permission) {
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

    #[allow(clippy::match_same_arms)]
    pub async fn delete_checked(&self, requestor: &User, target_perm_id: Uuid) -> AppResult<bool> {
        ensure_verified(requestor)?;

        crate::model::user_bans::UserBanRepository::new(self.state.clone())
            .ensure_not_banned(requestor.id)
            .await?;

        let target = self.find_by_id(target_perm_id).await?;

        // Check if requestor is admin/mod - if so, allow with audit log
        if crate::util::is_admin_or_mod(&self.state, requestor.id).await? {
            let result = self.delete(target_perm_id).await?;

            // Create audit log
            let audit_logs = crate::model::audit_log::AuditLogRepository::new(self.state.clone());
            let log_req = crate::model::audit_log::CreateAuditLog {
                user_id: Some(requestor.id),
                action: crate::model::audit_log::AuditActionType::Deleted,
                resource_type: crate::model::audit_log::AuditableResource::Permission,
                resource_id: target_perm_id,
                details: serde_json::json!({
                    "family_id": target.family,
                    "target_user_id": target.user,
                    "permission": target.permission
                }),
            };
            let _ = audit_logs.create_internal(log_req).await;

            return Ok(result);
        }

        let requestor_perm = self
            .find_by_family_and_user(target.family, requestor.id)
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

        // check permission table
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

repo_from_parts!(LanguageFamilyPermissionRepository);
