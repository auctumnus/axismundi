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

#[allow(clippy::struct_field_names)]
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
    #[allow(dead_code)]
    pub created_by: Uuid,
    #[serde(skip_serializing)]
    #[allow(dead_code)]
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

        crate::model::user_bans::UserBanRepository::new(self.state.clone())
            .ensure_not_banned(requestor.id)
            .await?;

        // Get the word to find its language
        let word = crate::model::words::WordRepository::new(self.state.clone())
            .find_by_id(word_id)
            .await?;

        let is_admin_or_mod = crate::util::is_admin_or_mod(&self.state, requestor.id).await?;
        let mut needs_audit_log = false;

        let permissions = crate::model::language_permissions::LanguagePermissionRepository::new(
            self.state.clone(),
        );
        let user_perm = permissions
            .find_by_user_and_language(requestor.id, word.language)
            .await?;

        match (user_perm, is_admin_or_mod) {
            (Some(perm), _) if perm.permission != PermissionLevel::Viewer => {
                // Has proper permission, no audit log needed
            }
            (_, true) => {
                // Is admin/mod but doesn't have proper permission, allow with audit log
                needs_audit_log = true;
            }
            _ => {
                return Err(bad_request(
                    "you don't have permission to create definitions",
                ));
            }
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

        // Create audit log if admin/mod override
        if needs_audit_log {
            let audit_logs = crate::model::audit_log::AuditLogRepository::new(self.state.clone());
            let log_req = crate::model::audit_log::CreateAuditLog {
                user_id: Some(requestor.id),
                action: crate::model::audit_log::AuditActionType::Created,
                resource_type: crate::model::audit_log::AuditableResource::Definition,
                resource_id: result.id,
                details: serde_json::json!({
                    "word_id": word_id,
                    "language_id": word.language,
                    "definition": definition.definition
                }),
            };
            let _ = audit_logs.create_internal(log_req).await;
        }

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

        crate::model::user_bans::UserBanRepository::new(self.state.clone())
            .ensure_not_banned(requestor.id)
            .await?;

        // Get the definition to find its word and language
        let existing = self.find_by_id(id).await?;
        let word = crate::model::words::WordRepository::new(self.state.clone())
            .find_by_id(existing.word)
            .await?;

        let is_admin_or_mod = crate::util::is_admin_or_mod(&self.state, requestor.id).await?;
        let mut needs_audit_log = false;

        let permissions = crate::model::language_permissions::LanguagePermissionRepository::new(
            self.state.clone(),
        );
        let user_perm = permissions
            .find_by_user_and_language(requestor.id, word.language)
            .await?;

        match (user_perm, is_admin_or_mod) {
            (Some(perm), _) if perm.permission != PermissionLevel::Viewer => {
                // Has proper permission, no audit log needed
            }
            (_, true) => {
                // Is admin/mod but doesn't have proper permission, allow with audit log
                needs_audit_log = true;
            }
            _ => {
                return Err(bad_request("you don't have permission to edit definitions"));
            }
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

        let definition_result = result.ok_or_else(|| not_found(format!("definition with id '{id}'")))?;

        // Create audit log if admin/mod override
        if needs_audit_log {
            let audit_logs = crate::model::audit_log::AuditLogRepository::new(self.state.clone());
            let log_req = crate::model::audit_log::CreateAuditLog {
                user_id: Some(requestor.id),
                action: crate::model::audit_log::AuditActionType::Updated,
                resource_type: crate::model::audit_log::AuditableResource::Definition,
                resource_id: id,
                details: serde_json::json!({
                    "word_id": existing.word,
                    "language_id": word.language,
                    "updates": updates
                }),
            };
            let _ = audit_logs.create_internal(log_req).await;
        }

        Ok(definition_result)
    }

    pub async fn delete(&self, requestor: &User, id: Uuid) -> AppResult<bool> {
        ensure_verified(requestor)?;

        crate::model::user_bans::UserBanRepository::new(self.state.clone())
            .ensure_not_banned(requestor.id)
            .await?;

        // Get the definition to find its word and language
        let existing = self.find_by_id(id).await?;
        let word = crate::model::words::WordRepository::new(self.state.clone())
            .find_by_id(existing.word)
            .await?;

        let is_admin_or_mod = crate::util::is_admin_or_mod(&self.state, requestor.id).await?;
        let mut needs_audit_log = false;

        let permissions = crate::model::language_permissions::LanguagePermissionRepository::new(
            self.state.clone(),
        );
        let user_perm = permissions
            .find_by_user_and_language(requestor.id, word.language)
            .await?;

        match (user_perm, is_admin_or_mod) {
            (Some(perm), _) if perm.permission != PermissionLevel::Viewer => {
                // Has proper permission, no audit log needed
            }
            (_, true) => {
                // Is admin/mod but doesn't have proper permission, allow with audit log
                needs_audit_log = true;
            }
            _ => {
                return Err(bad_request(
                    "you don't have permission to delete definitions",
                ));
            }
        }

        let result = sqlx::query!("DELETE FROM definitions WHERE id = $1", id)
            .execute(&self.state.pool)
            .await?;

        // Create audit log if admin/mod override
        if needs_audit_log && result.rows_affected() > 0 {
            let audit_logs = crate::model::audit_log::AuditLogRepository::new(self.state.clone());
            let log_req = crate::model::audit_log::CreateAuditLog {
                user_id: Some(requestor.id),
                action: crate::model::audit_log::AuditActionType::Deleted,
                resource_type: crate::model::audit_log::AuditableResource::Definition,
                resource_id: id,
                details: serde_json::json!({
                    "word_id": existing.word,
                    "language_id": word.language,
                    "definition": existing.definition
                }),
            };
            let _ = audit_logs.create_internal(log_req).await;
        }

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
        let has_more =
            (i64::from(pagination.offset) + i64::try_from(items.len()).unwrap_or(i64::MAX)) < total;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::audit_log::{
        AuditActionType, AuditLogFilter, AuditLogRepository, AuditableResource,
    };
    use crate::model::languages::{CreateLanguage, LanguageRepository};
    use crate::model::users::UserRepository;
    use crate::model::words::{CreateWord, WordRepository};
    use crate::pagination::PaginatedRequest;
    use crate::{config::CONFIG, create_router, email};
    use sqlx::PgPool;

    #[tokio::test]
    async fn test_create_definition_as_admin_creates_audit_log() {
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

        // Create a language and word by another user
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

        

        let word_repo = WordRepository::new(app_state.clone());
        let word = word_repo
            .create(
                &owner,
                lang.id,
                CreateWord {
                    word: "testword".to_string(),
                    word_class: "n".to_string(),
                    ipa: None,
                    notes: Some("test notes".to_string()),
                    extra: None,
                },
            )
            .await
            .unwrap();

        // Admin creates a definition (without permission)
        let def_repo = DefinitionRepository::new(app_state.clone());
        let definition = def_repo
            .create(
                &admin,
                word.id,
                CreateDefinition {
                    definition: "test definition".to_string(),
                    context: None,
                },
            )
            .await
            .unwrap();

        // Check audit log was created
        let audit_repo = AuditLogRepository::new(app_state.clone());
        let logs = audit_repo
            .search(
                &admin,
                PaginatedRequest::default(),
                AuditLogFilter {
                    user_id: Some(admin.id),
                    action: Some(AuditActionType::Created),
                    resource_type: Some(AuditableResource::Definition),
                    resource_id: Some(definition.id),
                },
            )
            .await
            .unwrap();

        assert_eq!(logs.items.len(), 1);
        let log = &logs.items[0];
        assert_eq!(log.user_id, Some(admin.id));
        assert_eq!(log.action, AuditActionType::Created);
        assert_eq!(log.resource_type, AuditableResource::Definition);
        assert_eq!(log.resource_id, definition.id);
        assert_eq!(log.details["word_id"], serde_json::json!(word.id));
        assert_eq!(log.details["language_id"], serde_json::json!(lang.id));
        assert_eq!(
            log.details["definition"],
            serde_json::json!("test definition")
        );
    }

    #[tokio::test]
    async fn test_update_definition_as_moderator_creates_audit_log() {
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

        // Create a language and word by another user
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

        

        let word_repo = WordRepository::new(app_state.clone());
        let word = word_repo
            .create(
                &owner,
                lang.id,
                CreateWord {
                    word: "testword".to_string(),
                    word_class: "n".to_string(),
                    ipa: None,
                    notes: Some("test notes".to_string()),
                    extra: None,
                },
            )
            .await
            .unwrap();

        let def_repo = DefinitionRepository::new(app_state.clone());
        let definition = def_repo
            .create(
                &owner,
                word.id,
                CreateDefinition {
                    definition: "test definition".to_string(),
                    context: None,
                },
            )
            .await
            .unwrap();

        // Moderator updates the definition (without permission)
        let updated = def_repo
            .update(
                &moderator,
                definition.id,
                UpdateDefinition {
                    definition: Some("updated definition".to_string()),
                    context: None,
                },
            )
            .await
            .unwrap();

        assert_eq!(updated.definition, "updated definition");

        // Check audit log was created
        let audit_repo = AuditLogRepository::new(app_state.clone());
        let logs = audit_repo
            .search(
                &moderator,
                PaginatedRequest::default(),
                AuditLogFilter {
                    user_id: Some(moderator.id),
                    action: Some(AuditActionType::Updated),
                    resource_type: Some(AuditableResource::Definition),
                    resource_id: Some(definition.id),
                },
            )
            .await
            .unwrap();

        assert_eq!(logs.items.len(), 1);
        let log = &logs.items[0];
        assert_eq!(log.user_id, Some(moderator.id));
        assert_eq!(log.action, AuditActionType::Updated);
        assert_eq!(log.resource_type, AuditableResource::Definition);
        assert_eq!(log.resource_id, definition.id);
        assert_eq!(log.details["word_id"], serde_json::json!(word.id));
        assert_eq!(log.details["language_id"], serde_json::json!(lang.id));
    }

    #[tokio::test]
    async fn test_delete_definition_as_admin_creates_audit_log() {
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

        // Create a language and word by another user
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

        

        let word_repo = WordRepository::new(app_state.clone());
        let word = word_repo
            .create(
                &owner,
                lang.id,
                CreateWord {
                    word: "testword".to_string(),
                    word_class: "n".to_string(),
                    ipa: None,
                    notes: Some("test notes".to_string()),
                    extra: None,
                },
            )
            .await
            .unwrap();

        let def_repo = DefinitionRepository::new(app_state.clone());
        let definition = def_repo
            .create(
                &owner,
                word.id,
                CreateDefinition {
                    definition: "test definition".to_string(),
                    context: None,
                },
            )
            .await
            .unwrap();

        let def_id = definition.id;

        // Admin deletes the definition (without permission)
        let deleted = def_repo.delete(&admin, def_id).await.unwrap();
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
                    resource_type: Some(AuditableResource::Definition),
                    resource_id: Some(def_id),
                },
            )
            .await
            .unwrap();

        assert_eq!(logs.items.len(), 1);
        let log = &logs.items[0];
        assert_eq!(log.user_id, Some(admin.id));
        assert_eq!(log.action, AuditActionType::Deleted);
        assert_eq!(log.resource_type, AuditableResource::Definition);
        assert_eq!(log.resource_id, def_id);
        assert_eq!(log.details["word_id"], serde_json::json!(word.id));
        assert_eq!(log.details["language_id"], serde_json::json!(lang.id));
        assert_eq!(
            log.details["definition"],
            serde_json::json!("test definition")
        );
    }
}
