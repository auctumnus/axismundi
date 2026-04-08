use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use crate::err::{AppResult, bad_request, not_found};
use crate::model::audit_log::{AuditActionType, PermissionCheck};
use crate::model::language_invites::LanguageInvite;
use crate::model::users::User;
use crate::util::{AppState, ensure_verified, is_admin_or_mod};

use super::language_invites::PermissionLevel;

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

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateLanguagePermission {
    pub language: Uuid,
    pub user: Uuid,
    pub permission: PermissionLevel,
    pub via: Option<Uuid>,
}

pub struct CheckPermissionReq {
    pub user: Uuid,
    pub language: Uuid,
    pub required_level: PermissionLevel,
    pub action_type: AuditActionType,
    pub resource_type: crate::model::audit_log::AuditableResource,
    pub resource_id: Uuid,
    pub context: Option<serde_json::Value>,
}

pub struct LanguagePermissionRepository {
    state: AppState,
}

impl LanguagePermissionRepository {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    pub async fn create_by_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        permission: CreateLanguagePermission,
        invited_by: Uuid,
    ) -> AppResult<LanguagePermission> {
        let result = sqlx::query_as!(
            LanguagePermission,
            r#"
                INSERT INTO language_permissions (language, "user", permission, via, invited_by, invited_at, accepted_at)
                VALUES ($1, $2, $3, $4, $5, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
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

    pub async fn find_owner(&self, language: Uuid) -> AppResult<LanguagePermission> {
        let result = sqlx::query_as!(
            LanguagePermission,
            r#"SELECT id, language, "user", permission as "permission: PermissionLevel", via, invited_by, invited_at, accepted_at FROM language_permissions WHERE language = $1 AND permission = $2"#,
            language,
            PermissionLevel::Owner as PermissionLevel
        )
        .fetch_optional(&self.state.pool)
        .await?;

        result.ok_or_else(|| not_found(format!("owner permission for language '{language}'")))
    }

    #[allow(dead_code)]
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

    pub async fn create_from_invite(
        &self,
        invite: &LanguageInvite,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ) -> AppResult<LanguagePermission> {
        if invite.permissions == PermissionLevel::Owner {
            // transfer ownership
            sqlx::query!(
                r#"
                    UPDATE language_permissions
                    SET permission = $2
                    WHERE language = $1 AND permission = $3
                "#,
                invite.language,
                PermissionLevel::Admin as PermissionLevel,
                PermissionLevel::Owner as PermissionLevel
            )
            .execute(&mut **tx)
            .await?;
        }

        let result = sqlx::query_as!(
            LanguagePermission,
            r#"
                INSERT INTO language_permissions (language, "user", permission, via, invited_by, invited_at, accepted_at)
                VALUES ($1, $2, $3, $4, $5, $6, $7)
                RETURNING id, language, "user", permission as "permission: PermissionLevel", via, invited_by, invited_at, accepted_at
            "#,
            invite.language,
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
                    "language_id": target.language,
                    "target_user_id": target.user,
                    "old_permission": target.permission,
                    "new_permission": new_permission
                }),
            };
            let _ = audit_logs.create_internal(log_req).await;

            return Ok(result);
        }

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
                    "language_id": target.language,
                    "target_user_id": target.user,
                    "permission": target.permission
                }),
            };
            let _ = audit_logs.create_internal(log_req).await;

            return Ok(result);
        }

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

    pub async fn has_permission(
        &self,
        user: Uuid,
        language: Uuid,
        required: PermissionLevel,
    ) -> AppResult<bool> {
        let perm = self.find_by_user_and_language(user, language).await?;

        Ok(match perm {
            Some(p) => p.permission >= required,
            None => false,
        })
    }

    pub async fn check_permission_with_audit(
        &self,
        req: CheckPermissionReq,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ) -> AppResult<PermissionCheck> {
        let has_perm = self
            .has_permission(req.user, req.language, req.required_level)
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
                    details: req.context.unwrap_or_else(|| serde_json::json!({})),
                };
                audit_logs.create_internal_tx(tx, log_req).await?;

                Ok(PermissionCheck::Audited)
            } else {
                Ok(PermissionCheck::NoPermission)
            }
        }
    }

    pub async fn can_edit_language(
        &self,
        requestor: Option<&User>,
        language: &Uuid,
    ) -> AppResult<bool> {
        if let Some(user) = requestor {
            ensure_verified(user)?;

            if is_admin_or_mod(&self.state, user.id).await? {
                return Ok(true);
            }

            let perm = self.find_by_user_and_language(user.id, *language).await?;

            Ok(match perm {
                Some(p) => p.permission >= PermissionLevel::Editor,
                None => false,
            })
        } else {
            Ok(false)
        }
    }
}

crate::util::repo_from_parts!(LanguagePermissionRepository);

#[async_trait::async_trait]
impl crate::model::bookmarks::ResolveBookmark for LanguagePermissionRepository {
    async fn resolve_bookmark(
        &self,
        item: Uuid,
        link_type: crate::model::bookmarks::LinkType,
    ) -> AppResult<String> {
        // api: /api/languages/{code}/permissions/{username}
        // web: /languages/{code}/permissions/{username}
        let permission = self.find_by_id(item).await?;

        let languages = crate::model::languages::LanguageRepository::new(self.state.clone());
        let language = languages.find_by_id(permission.language).await?;

        let users = crate::model::users::UserRepository::new(self.state.clone());
        let user = users.find_by_id(permission.user).await?;

        let slug = match link_type {
            crate::model::bookmarks::LinkType::Web => {
                format!("/languages/{}/permissions/{}", language.code, user.username)
            }
            crate::model::bookmarks::LinkType::Api => format!(
                "/api/languages/{}/permissions/{}",
                language.code, user.username
            ),
        };

        Ok(slug)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::audit_log::{
        AuditActionType, AuditLogFilter, AuditLogRepository, AuditableResource,
    };
    use crate::model::language_invites::PermissionLevel;
    use crate::model::languages::{CreateLanguage, LanguageRepository};
    use crate::model::users::UserRepository;
    use crate::pagination::PaginatedRequest;
    use crate::tests::random_code;
    use crate::{config::CONFIG, create_router, email};
    use sqlx::PgPool;

    #[tokio::test]
    async fn test_update_permission_as_admin_creates_audit_log() {
        let pool = PgPool::connect(&CONFIG.database_url).await.unwrap();
        let email_service = std::sync::Arc::new(email::MockEmailService::new());
        let app_state = crate::util::AppState {
            pool: pool.clone(),
            email_service: email_service.clone(),
        };
        let app = create_router(app_state.clone()).into_service();

        // Create an admin user
        let admin_username = crate::tests::random_name();
        let _admin_token =
            crate::tests::make_authed_user(&admin_username, &app, email_service.clone()).await;
        let admin_id =
            sqlx::query_scalar!("select id from users where username = $1", admin_username)
                .fetch_one(&pool)
                .await
                .unwrap();
        sqlx::query!(
            "insert into user_tags (user_id, tag, hidden) values ($1, 'admin', false)",
            admin_id
        )
        .execute(&pool)
        .await
        .unwrap();

        let user_repo = UserRepository::new(app_state.clone());
        let admin = user_repo.find_by_id(admin_id).await.unwrap();

        // Create a language owner
        let owner_username = crate::tests::random_name();
        let _owner_token =
            crate::tests::make_authed_user(&owner_username, &app, email_service.clone()).await;
        let owner_id =
            sqlx::query_scalar!("select id from users where username = $1", owner_username)
                .fetch_one(&pool)
                .await
                .unwrap();
        let owner = user_repo.find_by_id(owner_id).await.unwrap();

        let lang_repo = LanguageRepository::new(app_state.clone());
        let lang = lang_repo
            .create(
                &owner,
                CreateLanguage {
                    name: "Test Language".to_string(),
                    code: crate::tests::random_code(),
                    description: "A test language".to_string(),
                    private: false,
                },
            )
            .await
            .unwrap();

        // Create an editor to target
        let editor_username = crate::tests::random_name();
        let _editor_token =
            crate::tests::make_authed_user(&editor_username, &app, email_service.clone()).await;
        let editor_id =
            sqlx::query_scalar!("select id from users where username = $1", editor_username)
                .fetch_one(&pool)
                .await
                .unwrap();
        let editor = user_repo.find_by_id(editor_id).await.unwrap();

        let perm_repo = LanguagePermissionRepository::new(app_state.clone());

        let mut tx = pool.begin().await.unwrap();
        let editor_perm = perm_repo
            .create_by_tx(
                &mut tx,
                CreateLanguagePermission {
                    language: lang.id,
                    user: editor.id,
                    permission: PermissionLevel::Editor,
                    via: None,
                },
                owner.id,
            )
            .await
            .unwrap();
        tx.commit().await.unwrap();

        // Admin updates editor's permission
        let updated = perm_repo
            .update_permission_checked(&admin, editor_perm.id, PermissionLevel::Admin)
            .await
            .unwrap();

        assert_eq!(updated.permission, PermissionLevel::Admin);

        // Check audit log was created
        let audit_repo = AuditLogRepository::new(app_state.clone());
        let logs = audit_repo
            .search(
                &admin,
                PaginatedRequest::default(),
                AuditLogFilter {
                    user_id: Some(admin.id),
                    action: Some(AuditActionType::Updated),
                    resource_type: Some(AuditableResource::Permission),
                    resource_id: Some(editor_perm.id),
                },
            )
            .await
            .unwrap();

        assert_eq!(logs.items.len(), 1);
        let log = &logs.items[0];
        assert_eq!(log.user_id, Some(admin.id));
        assert_eq!(log.action, AuditActionType::Updated);
        assert_eq!(log.resource_type, AuditableResource::Permission);
        assert_eq!(log.resource_id, editor_perm.id);

        // Check details contain the right information
        let details = &log.details;
        assert_eq!(details["language_id"], serde_json::json!(lang.id));
        assert_eq!(details["target_user_id"], serde_json::json!(editor.id));
        assert_eq!(details["old_permission"], serde_json::json!("editor"));
        assert_eq!(details["new_permission"], serde_json::json!("admin"));
    }

    #[tokio::test]
    async fn test_update_permission_as_moderator_creates_audit_log() {
        let pool = PgPool::connect(&CONFIG.database_url).await.unwrap();
        let email_service = std::sync::Arc::new(email::MockEmailService::new());
        let app_state = crate::util::AppState {
            pool: pool.clone(),
            email_service: email_service.clone(),
        };
        let app = create_router(app_state.clone()).into_service();

        // Create a moderator user
        let mod_username = crate::tests::random_name();
        let _mod_token =
            crate::tests::make_authed_user(&mod_username, &app, email_service.clone()).await;
        let mod_id = sqlx::query_scalar!("select id from users where username = $1", mod_username)
            .fetch_one(&pool)
            .await
            .unwrap();
        sqlx::query!(
            "insert into user_tags (user_id, tag, hidden) values ($1, 'moderator', false)",
            mod_id
        )
        .execute(&pool)
        .await
        .unwrap();

        let user_repo = UserRepository::new(app_state.clone());
        let moderator = user_repo.find_by_id(mod_id).await.unwrap();

        // Create a language owner
        let owner_username = crate::tests::random_name();
        let _owner_token =
            crate::tests::make_authed_user(&owner_username, &app, email_service.clone()).await;
        let owner_id =
            sqlx::query_scalar!("select id from users where username = $1", owner_username)
                .fetch_one(&pool)
                .await
                .unwrap();
        let owner = user_repo.find_by_id(owner_id).await.unwrap();

        let lang_repo = LanguageRepository::new(app_state.clone());
        let lang = lang_repo
            .create(
                &owner,
                CreateLanguage {
                    name: "Test Language".to_string(),
                    code: random_code(),
                    description: "A test language".to_string(),
                    private: false,
                },
            )
            .await
            .unwrap();

        // Create an admin to target
        let admin_username = crate::tests::random_name();
        let _admin_token =
            crate::tests::make_authed_user(&admin_username, &app, email_service.clone()).await;
        let admin_id =
            sqlx::query_scalar!("select id from users where username = $1", admin_username)
                .fetch_one(&pool)
                .await
                .unwrap();
        let admin_user = user_repo.find_by_id(admin_id).await.unwrap();

        let perm_repo = LanguagePermissionRepository::new(app_state.clone());

        let mut tx = pool.begin().await.unwrap();
        let admin_perm = perm_repo
            .create_by_tx(
                &mut tx,
                CreateLanguagePermission {
                    language: lang.id,
                    user: admin_user.id,
                    permission: PermissionLevel::Admin,
                    via: None,
                },
                owner.id,
            )
            .await
            .unwrap();
        tx.commit().await.unwrap();

        // Moderator updates admin's permission to editor
        let updated = perm_repo
            .update_permission_checked(&moderator, admin_perm.id, PermissionLevel::Editor)
            .await
            .unwrap();

        assert_eq!(updated.permission, PermissionLevel::Editor);

        // Check audit log was created
        let audit_repo = AuditLogRepository::new(app_state.clone());
        let logs = audit_repo
            .search(
                &moderator,
                PaginatedRequest::default(),
                AuditLogFilter {
                    user_id: Some(moderator.id),
                    action: Some(AuditActionType::Updated),
                    resource_type: Some(AuditableResource::Permission),
                    resource_id: Some(admin_perm.id),
                },
            )
            .await
            .unwrap();

        assert_eq!(logs.items.len(), 1);
        let log = &logs.items[0];
        assert_eq!(log.user_id, Some(moderator.id));
        assert_eq!(log.action, AuditActionType::Updated);
        assert_eq!(log.resource_id, admin_perm.id);
        assert_eq!(log.details["old_permission"], serde_json::json!("admin"));
        assert_eq!(log.details["new_permission"], serde_json::json!("editor"));
    }

    #[tokio::test]
    async fn test_delete_permission_as_admin_creates_audit_log() {
        let pool = PgPool::connect(&CONFIG.database_url).await.unwrap();
        let email_service = std::sync::Arc::new(email::MockEmailService::new());
        let app_state = crate::util::AppState {
            pool: pool.clone(),
            email_service: email_service.clone(),
        };
        let app = create_router(app_state.clone()).into_service();

        // Create an admin user
        let admin_username = crate::tests::random_name();
        let _admin_token =
            crate::tests::make_authed_user(&admin_username, &app, email_service.clone()).await;
        let admin_id =
            sqlx::query_scalar!("select id from users where username = $1", admin_username)
                .fetch_one(&pool)
                .await
                .unwrap();
        sqlx::query!(
            "insert into user_tags (user_id, tag, hidden) values ($1, 'admin', false)",
            admin_id
        )
        .execute(&pool)
        .await
        .unwrap();

        let user_repo = UserRepository::new(app_state.clone());
        let admin = user_repo.find_by_id(admin_id).await.unwrap();

        // Create a language owner
        let owner_username = crate::tests::random_name();
        let _owner_token =
            crate::tests::make_authed_user(&owner_username, &app, email_service.clone()).await;
        let owner_id =
            sqlx::query_scalar!("select id from users where username = $1", owner_username)
                .fetch_one(&pool)
                .await
                .unwrap();
        let owner = user_repo.find_by_id(owner_id).await.unwrap();

        let lang_repo = LanguageRepository::new(app_state.clone());
        let lang = lang_repo
            .create(
                &owner,
                CreateLanguage {
                    name: "Test Language".to_string(),
                    code: crate::tests::random_code(),
                    description: "A test language".to_string(),
                    private: false,
                },
            )
            .await
            .unwrap();

        // Create an editor to delete
        let editor_username = crate::tests::random_name();
        let _editor_token =
            crate::tests::make_authed_user(&editor_username, &app, email_service.clone()).await;
        let editor_id =
            sqlx::query_scalar!("select id from users where username = $1", editor_username)
                .fetch_one(&pool)
                .await
                .unwrap();
        let editor = user_repo.find_by_id(editor_id).await.unwrap();

        let perm_repo = LanguagePermissionRepository::new(app_state.clone());

        let mut tx = pool.begin().await.unwrap();
        let editor_perm = perm_repo
            .create_by_tx(
                &mut tx,
                CreateLanguagePermission {
                    language: lang.id,
                    user: editor.id,
                    permission: PermissionLevel::Editor,
                    via: None,
                },
                owner.id,
            )
            .await
            .unwrap();
        tx.commit().await.unwrap();

        let perm_id = editor_perm.id;

        // Admin deletes editor's permission
        let deleted = perm_repo.delete_checked(&admin, perm_id).await.unwrap();

        assert!(deleted);

        // Check audit log was created
        let audit_repo = AuditLogRepository::new(app_state.clone());
        let logs = audit_repo
            .search(
                &admin,
                PaginatedRequest::default(),
                AuditLogFilter {
                    user_id: Some(admin.id),
                    action: Some(AuditActionType::Deleted),
                    resource_type: Some(AuditableResource::Permission),
                    resource_id: Some(perm_id),
                },
            )
            .await
            .unwrap();

        assert_eq!(logs.items.len(), 1);
        let log = &logs.items[0];
        assert_eq!(log.user_id, Some(admin.id));
        assert_eq!(log.action, AuditActionType::Deleted);
        assert_eq!(log.resource_type, AuditableResource::Permission);
        assert_eq!(log.resource_id, perm_id);

        // Check details contain the right information
        let details = &log.details;
        assert_eq!(details["language_id"], serde_json::json!(lang.id));
        assert_eq!(details["target_user_id"], serde_json::json!(editor.id));
        assert_eq!(details["permission"], serde_json::json!("editor"));
    }

    #[tokio::test]
    async fn test_delete_permission_as_moderator_creates_audit_log() {
        let pool = PgPool::connect(&CONFIG.database_url).await.unwrap();
        let email_service = std::sync::Arc::new(email::MockEmailService::new());
        let app_state = crate::util::AppState {
            pool: pool.clone(),
            email_service: email_service.clone(),
        };
        let app = create_router(app_state.clone()).into_service();

        // Create a moderator user
        let mod_username = crate::tests::random_name();
        let _mod_token =
            crate::tests::make_authed_user(&mod_username, &app, email_service.clone()).await;
        let mod_id = sqlx::query_scalar!("select id from users where username = $1", mod_username)
            .fetch_one(&pool)
            .await
            .unwrap();
        sqlx::query!(
            "insert into user_tags (user_id, tag, hidden) values ($1, 'moderator', false)",
            mod_id
        )
        .execute(&pool)
        .await
        .unwrap();

        let user_repo = UserRepository::new(app_state.clone());
        let moderator = user_repo.find_by_id(mod_id).await.unwrap();

        // Create a language owner
        let owner_username = crate::tests::random_name();
        let _owner_token =
            crate::tests::make_authed_user(&owner_username, &app, email_service.clone()).await;
        let owner_id =
            sqlx::query_scalar!("select id from users where username = $1", owner_username)
                .fetch_one(&pool)
                .await
                .unwrap();
        let owner = user_repo.find_by_id(owner_id).await.unwrap();

        let lang_repo = LanguageRepository::new(app_state.clone());
        let lang = lang_repo
            .create(
                &owner,
                CreateLanguage {
                    name: "Test Language".to_string(),
                    code: crate::tests::random_code(),
                    description: "A test language".to_string(),
                    private: false,
                },
            )
            .await
            .unwrap();

        // Create an editor to delete
        let editor_username = crate::tests::random_name();
        let _editor_token =
            crate::tests::make_authed_user(&editor_username, &app, email_service.clone()).await;
        let editor_id =
            sqlx::query_scalar!("select id from users where username = $1", editor_username)
                .fetch_one(&pool)
                .await
                .unwrap();
        let editor = user_repo.find_by_id(editor_id).await.unwrap();

        let perm_repo = LanguagePermissionRepository::new(app_state.clone());

        let mut tx = pool.begin().await.unwrap();
        let editor_perm = perm_repo
            .create_by_tx(
                &mut tx,
                CreateLanguagePermission {
                    language: lang.id,
                    user: editor.id,
                    permission: PermissionLevel::Editor,
                    via: None,
                },
                owner.id,
            )
            .await
            .unwrap();
        tx.commit().await.unwrap();

        let perm_id = editor_perm.id;

        // Moderator deletes editor's permission
        let deleted = perm_repo.delete_checked(&moderator, perm_id).await.unwrap();

        assert!(deleted);

        // Check audit log was created
        let audit_repo = AuditLogRepository::new(app_state.clone());
        let logs = audit_repo
            .search(
                &moderator,
                PaginatedRequest::default(),
                AuditLogFilter {
                    user_id: Some(moderator.id),
                    action: Some(AuditActionType::Deleted),
                    resource_type: Some(AuditableResource::Permission),
                    resource_id: Some(perm_id),
                },
            )
            .await
            .unwrap();

        assert_eq!(logs.items.len(), 1);
        assert_eq!(logs.items[0].user_id, Some(moderator.id));
    }

    #[tokio::test]
    async fn test_normal_user_update_does_not_create_audit_log() {
        let pool = PgPool::connect(&CONFIG.database_url).await.unwrap();
        let email_service = std::sync::Arc::new(email::MockEmailService::new());
        let app_state = crate::util::AppState {
            pool: pool.clone(),
            email_service: email_service.clone(),
        };
        let app = create_router(app_state.clone()).into_service();

        // Create a language owner
        let owner_username = crate::tests::random_name();
        let _owner_token =
            crate::tests::make_authed_user(&owner_username, &app, email_service.clone()).await;
        let owner_id =
            sqlx::query_scalar!("select id from users where username = $1", owner_username)
                .fetch_one(&pool)
                .await
                .unwrap();

        let user_repo = UserRepository::new(app_state.clone());
        let owner = user_repo.find_by_id(owner_id).await.unwrap();

        let lang_repo = LanguageRepository::new(app_state.clone());
        let lang = lang_repo
            .create(
                &owner,
                CreateLanguage {
                    name: "Test Language".to_string(),
                    code: crate::tests::random_code(),
                    description: "A test language".to_string(),
                    private: false,
                },
            )
            .await
            .unwrap();

        // Create an editor to target
        let editor_username = crate::tests::random_name();
        let _editor_token =
            crate::tests::make_authed_user(&editor_username, &app, email_service.clone()).await;
        let editor_id =
            sqlx::query_scalar!("select id from users where username = $1", editor_username)
                .fetch_one(&pool)
                .await
                .unwrap();
        let editor = user_repo.find_by_id(editor_id).await.unwrap();

        let perm_repo = LanguagePermissionRepository::new(app_state.clone());

        let mut tx = pool.begin().await.unwrap();
        let editor_perm = perm_repo
            .create_by_tx(
                &mut tx,
                CreateLanguagePermission {
                    language: lang.id,
                    user: editor.id,
                    permission: PermissionLevel::Editor,
                    via: None,
                },
                owner.id,
            )
            .await
            .unwrap();
        tx.commit().await.unwrap();

        // Owner updates editor's permission (normal permission flow, not override)
        let updated = perm_repo
            .update_permission_checked(&owner, editor_perm.id, PermissionLevel::Admin)
            .await
            .unwrap();

        assert_eq!(updated.permission, PermissionLevel::Admin);

        // Check NO audit log was created (owner is not admin/mod, so this is normal operation)
        let admin_username = crate::tests::random_name();
        let _admin_token =
            crate::tests::make_authed_user(&admin_username, &app, email_service.clone()).await;
        let admin_id =
            sqlx::query_scalar!("select id from users where username = $1", admin_username)
                .fetch_one(&pool)
                .await
                .unwrap();
        sqlx::query!(
            "insert into user_tags (user_id, tag, hidden) values ($1, 'admin', false)",
            admin_id
        )
        .execute(&pool)
        .await
        .unwrap();
        let admin = user_repo.find_by_id(admin_id).await.unwrap();

        let audit_repo = AuditLogRepository::new(app_state.clone());
        let logs = audit_repo
            .search(
                &admin,
                PaginatedRequest::default(),
                AuditLogFilter {
                    user_id: Some(owner.id),
                    action: Some(AuditActionType::Updated),
                    resource_type: Some(AuditableResource::Permission),
                    resource_id: Some(editor_perm.id),
                },
            )
            .await
            .unwrap();

        // Should be 0 logs because owner is using normal permissions, not override
        assert_eq!(logs.items.len(), 0);
    }

    #[tokio::test]
    async fn test_normal_user_delete_does_not_create_audit_log() {
        let pool = PgPool::connect(&CONFIG.database_url).await.unwrap();
        let email_service = std::sync::Arc::new(email::MockEmailService::new());
        let app_state = crate::util::AppState {
            pool: pool.clone(),
            email_service: email_service.clone(),
        };
        let app = create_router(app_state.clone()).into_service();

        // Create a language owner
        let owner_username = crate::tests::random_name();
        let _owner_token =
            crate::tests::make_authed_user(&owner_username, &app, email_service.clone()).await;
        let owner_id =
            sqlx::query_scalar!("select id from users where username = $1", owner_username)
                .fetch_one(&pool)
                .await
                .unwrap();

        let user_repo = UserRepository::new(app_state.clone());
        let owner = user_repo.find_by_id(owner_id).await.unwrap();

        let lang_repo = LanguageRepository::new(app_state.clone());
        let lang = lang_repo
            .create(
                &owner,
                CreateLanguage {
                    name: "Test Language".to_string(),
                    code: crate::tests::random_code(),
                    description: "A test language".to_string(),
                    private: false,
                },
            )
            .await
            .unwrap();

        // Create an editor to delete
        let editor_username = crate::tests::random_name();
        let _editor_token =
            crate::tests::make_authed_user(&editor_username, &app, email_service.clone()).await;
        let editor_id =
            sqlx::query_scalar!("select id from users where username = $1", editor_username)
                .fetch_one(&pool)
                .await
                .unwrap();
        let editor = user_repo.find_by_id(editor_id).await.unwrap();

        let perm_repo = LanguagePermissionRepository::new(app_state.clone());

        let mut tx = pool.begin().await.unwrap();
        let editor_perm = perm_repo
            .create_by_tx(
                &mut tx,
                CreateLanguagePermission {
                    language: lang.id,
                    user: editor.id,
                    permission: PermissionLevel::Editor,
                    via: None,
                },
                owner.id,
            )
            .await
            .unwrap();
        tx.commit().await.unwrap();

        let perm_id = editor_perm.id;

        // Owner deletes editor's permission (normal permission flow)
        let deleted = perm_repo.delete_checked(&owner, perm_id).await.unwrap();

        assert!(deleted);

        // Check NO audit log was created
        let admin_username = crate::tests::random_name();
        let _admin_token =
            crate::tests::make_authed_user(&admin_username, &app, email_service.clone()).await;
        let admin_id =
            sqlx::query_scalar!("select id from users where username = $1", admin_username)
                .fetch_one(&pool)
                .await
                .unwrap();
        sqlx::query!(
            "insert into user_tags (user_id, tag, hidden) values ($1, 'admin', false)",
            admin_id
        )
        .execute(&pool)
        .await
        .unwrap();
        let admin = user_repo.find_by_id(admin_id).await.unwrap();

        let audit_repo = AuditLogRepository::new(app_state.clone());
        let logs = audit_repo
            .search(
                &admin,
                PaginatedRequest::default(),
                AuditLogFilter {
                    user_id: Some(owner.id),
                    action: Some(AuditActionType::Deleted),
                    resource_type: Some(AuditableResource::Permission),
                    resource_id: Some(perm_id),
                },
            )
            .await
            .unwrap();

        // Should be 0 logs because owner is using normal permissions
        assert_eq!(logs.items.len(), 0);
    }
}
