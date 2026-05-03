use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use uuid::Uuid;
use validator::Validate;

use crate::AppState;
use crate::err::{AppResult, forbidden, not_found};
use crate::model::definitions::Definition;
use crate::model::language_families::LanguageFamily;
use crate::model::language_family_members::LanguageFamilyMember;
use crate::model::language_invites::LanguageInvite;
use crate::model::language_permissions::LanguagePermission;
use crate::model::languages::Language;
use crate::model::quotation_suggestions::QuotationSuggestion;
use crate::model::quotations::Quotation;
use crate::model::reports::Report;
use crate::model::translatable::Translatable;
use crate::model::translations::Translation;
use crate::model::user_tags::UserTagRepository;
use crate::model::users::User;
use crate::model::word_categories::WordCategory;
use crate::model::word_classes::WordClass;
use crate::model::word_relations::WordRelation;
use crate::model::words::Word;
use crate::pagination::{PaginatedRequest, PaginatedResponse};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "auditable_resource", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum AuditableResource {
    User,
    Language,
    #[sqlx(rename = "language_family_res")]
    LanguageFamily,
    LanguageFamilyMember,
    Word,
    WordClass,
    WordCategory,
    Translation,
    Translatable,
    WordRelation,
    Invite,
    Permission,
    Quotation,
    Definition,
    QuotationSuggestion,
    Report,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionCheck {
    NoPermission,
    HasPermission,
    Audited,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WordClassResolved {
    pub word_class: WordClass,
    pub language_code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WordCategoryResolved {
    pub word_category: WordCategory,
    pub language_code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum AuditableResourceResolved {
    User(User),
    Language(Language),
    LanguageFamily(LanguageFamily),
    LanguageFamilyMember(LanguageFamilyMember),
    Word(Word),
    WordClass(WordClassResolved),
    WordCategory(WordCategoryResolved),
    Translation(Translation),
    Translatable(Translatable),
    WordRelation(WordRelation),
    Invite(LanguageInvite),
    Permission(LanguagePermission),
    Quotation(Quotation),
    Definition(Definition),
    QuotationSuggestion(QuotationSuggestion),
    Report(Report),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "audit_action_type", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum AuditActionType {
    Created,
    Updated,
    Deleted,
    UpdatedReport,
    UserBan,
    UserUnban,
    AddTag,
    RemoveTag,
    Imported,
}

impl AuditActionType {
    pub fn as_str(self) -> &'static str {
        match self {
            AuditActionType::Created => "created",
            AuditActionType::Updated | AuditActionType::UpdatedReport => "updated",
            AuditActionType::Deleted => "deleted",
            AuditActionType::UserBan => "banned",
            AuditActionType::UserUnban => "unbanned",
            AuditActionType::AddTag => "added a tag to",
            AuditActionType::RemoveTag => "removed a tag from",
            AuditActionType::Imported => "imported into",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLog {
    pub id: Uuid,
    pub user_id: Option<Uuid>,
    pub action: AuditActionType,
    pub action_at: DateTime<Utc>,
    pub resource_type: AuditableResource,
    pub resource_id: Uuid,
    pub details: JsonValue,

    pub user: Option<User>,
    pub resource: Option<AuditableResourceResolved>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CreateAuditLog {
    pub user_id: Option<Uuid>,
    pub action: AuditActionType,
    pub resource_type: AuditableResource,
    pub resource_id: Uuid,
    pub details: JsonValue,
}

/// Filter options for searching audit logs
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct AuditLogFilter {
    pub user_id: Option<Uuid>,
    pub action: Option<AuditActionType>,
    pub resource_type: Option<AuditableResource>,
    pub resource_id: Option<Uuid>,
}

pub struct AuditLogRepository {
    state: AppState,
}

impl AuditLogRepository {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    /// Helper to fetch a user by ID
    async fn fetch_user(&self, user_id: Option<Uuid>) -> AppResult<Option<User>> {
        if let Some(user_id) = user_id {
            Ok(sqlx::query_as!(
                User,
                r#"
                select
                    users.id,
                    users.username,
                    users.email,
                    users.password_hash,
                    users.display_name,
                    users.description,
                    users.pronouns,
                    users.gender,
                    users.profile_picture_object_id,
                    users.banner_object_id,
                    users.tags,
                    users.created_at,
                    users.updated_at,
                    users.verified_at,
                    COALESCE(b.slug, '')::text as "bookmark!"
                from users
                left join bookmarks b on b.item = users.id and b.resource = 'user'
                where users.id = $1
                "#,
                user_id
            )
            .fetch_optional(&self.state.pool)
            .await?)
        } else {
            Ok(None)
        }
    }

    /// Helper to resolve a resource by type and ID
    /// Returns None if the resource has been deleted or can't be found
    async fn resolve_resource(
        &self,
        resource_type: AuditableResource,
        resource_id: Uuid,
    ) -> Option<AuditableResourceResolved> {
        use crate::model::definitions::DefinitionRepository;
        use crate::model::language_family_members::LanguageFamilyMemberRepository;
        use crate::model::language_invites::LanguageInviteRepository;
        use crate::model::language_permissions::LanguagePermissionRepository;
        use crate::model::languages::LanguageRepository;
        use crate::model::quotation_suggestions::QuotationSuggestionRepository;
        use crate::model::quotations::QuotationRepository;
        use crate::model::translatable::TranslatableRepository;
        use crate::model::translations::TranslationRepository;
        use crate::model::users::UserRepository;
        use crate::model::word_categories::WordCategoryRepository;
        use crate::model::word_classes::WordClassRepository;
        use crate::model::words::WordRepository;

        match resource_type {
            AuditableResource::User => {
                let repo = UserRepository::new(self.state.clone());
                repo.find_by_id(resource_id)
                    .await
                    .ok()
                    .map(AuditableResourceResolved::User)
            }
            AuditableResource::Language => {
                let repo = LanguageRepository::new(self.state.clone());
                repo.find_by_id(resource_id)
                    .await
                    .ok()
                    .map(AuditableResourceResolved::Language)
            }
            AuditableResource::LanguageFamily => {
                let repo = crate::model::language_families::LanguageFamilyRepository::new(
                    self.state.clone(),
                );
                repo.find_by_id(resource_id)
                    .await
                    .ok()
                    .map(AuditableResourceResolved::LanguageFamily)
            }
            AuditableResource::LanguageFamilyMember => {
                let repo = LanguageFamilyMemberRepository::new(self.state.clone());
                repo.find_by_id(resource_id)
                    .await
                    .ok()
                    .map(AuditableResourceResolved::LanguageFamilyMember)
            }
            AuditableResource::Word => {
                let repo = WordRepository::new(self.state.clone());
                repo.find_by_id(resource_id)
                    .await
                    .ok()
                    .map(AuditableResourceResolved::Word)
            }
            AuditableResource::WordClass => {
                let repo = WordClassRepository::new(self.state.clone());
                let word_class = repo.find_by_id(resource_id).await.ok()?;
                let language = LanguageRepository::new(self.state.clone())
                    .find_by_id(word_class.language)
                    .await
                    .ok()?;
                Some(AuditableResourceResolved::WordClass(WordClassResolved {
                    word_class,
                    language_code: language.code,
                }))
            }
            AuditableResource::WordCategory => {
                let repo = WordCategoryRepository::new(self.state.clone());
                let word_category = repo.find_by_id(resource_id).await.ok()?;
                let language = LanguageRepository::new(self.state.clone())
                    .find_by_id(word_category.language)
                    .await
                    .ok()?;
                Some(AuditableResourceResolved::WordCategory(
                    WordCategoryResolved {
                        word_category,
                        language_code: language.code,
                    },
                ))
            }
            AuditableResource::Translation => {
                let repo = TranslationRepository::new(self.state.clone());
                repo.find_by_id(resource_id)
                    .await
                    .ok()
                    .map(AuditableResourceResolved::Translation)
            }
            AuditableResource::Translatable => {
                let repo = TranslatableRepository::new(self.state.clone());
                repo.find_by_id(resource_id)
                    .await
                    .ok()
                    .map(AuditableResourceResolved::Translatable)
            }
            AuditableResource::WordRelation => {
                // WordRelation doesn't have a find_by_id method, so we can't resolve it
                None
            }
            AuditableResource::Invite => {
                let repo = LanguageInviteRepository::new(self.state.clone());
                repo.find_by_id(resource_id)
                    .await
                    .ok()
                    .map(AuditableResourceResolved::Invite)
            }
            AuditableResource::Permission => {
                let repo = LanguagePermissionRepository::new(self.state.clone());
                repo.find_by_id(resource_id)
                    .await
                    .ok()
                    .map(AuditableResourceResolved::Permission)
            }
            AuditableResource::Quotation => {
                let repo = QuotationRepository::new(self.state.clone());
                repo.find_by_id(resource_id)
                    .await
                    .ok()
                    .map(AuditableResourceResolved::Quotation)
            }
            AuditableResource::Definition => {
                let repo = DefinitionRepository::new(self.state.clone());
                repo.find_by_id(resource_id)
                    .await
                    .ok()
                    .map(AuditableResourceResolved::Definition)
            }
            AuditableResource::QuotationSuggestion => {
                let repo = QuotationSuggestionRepository::new(self.state.clone());
                repo.find_by_id(resource_id)
                    .await
                    .ok()
                    .map(AuditableResourceResolved::QuotationSuggestion)
            }
            AuditableResource::Report => {
                // Report requires a user parameter, which we don't have here
                // We could fetch it differently, but for now just return None
                None
            }
        }
    }

    /// Create a new audit log entry without permission checks.
    /// Use this when you've already verified the user is admin/mod.
    pub(crate) async fn create_internal(&self, req: CreateAuditLog) -> AppResult<AuditLog> {
        let mut tx = self.state.pool.begin().await?;
        let log = self.create_internal_tx(&mut tx, req).await?;
        tx.commit().await?;
        Ok(log)
    }

    pub(crate) async fn create_internal_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        req: CreateAuditLog,
    ) -> AppResult<AuditLog> {
        req.validate()?;

        let record = sqlx::query!(
            r#"
            insert into audit_logs (user_id, action, resource_type, resource_id, details)
            values ($1, $2, $3, $4, $5)
            returning
                id,
                user_id,
                action as "action: AuditActionType",
                action_at,
                resource_type as "resource_type: AuditableResource",
                resource_id,
                details
            "#,
            req.user_id,
            req.action as AuditActionType,
            req.resource_type as AuditableResource,
            req.resource_id,
            req.details
        )
        .fetch_one(&mut **tx)
        .await?;

        let user = self.fetch_user(record.user_id).await?;
        let resource = self
            .resolve_resource(record.resource_type, record.resource_id)
            .await;

        Ok(AuditLog {
            id: record.id,
            user_id: record.user_id,
            action: record.action,
            action_at: record.action_at,
            resource_type: record.resource_type,
            resource_id: record.resource_id,
            details: record.details,
            user,
            resource,
        })
    }

    /// Create a new audit log entry. Only callable by mods/admins.
    #[allow(dead_code)]
    pub async fn create(&self, requestor: &User, req: CreateAuditLog) -> AppResult<AuditLog> {
        // Check mod/admin permissions
        let user_tags = UserTagRepository::new(self.state.clone());
        let is_admin = user_tags.is_admin(requestor.id).await?;
        let is_moderator = user_tags.is_moderator(requestor.id).await?;

        if !(is_admin || is_moderator) {
            return Err(forbidden(
                "Only moderators and admins can create audit logs",
            ));
        }

        self.create_internal(req).await
    }

    /// Find a single audit log by ID. Only accessible to mods/admins.
    pub async fn find_by_id(&self, requestor: &User, id: Uuid) -> AppResult<AuditLog> {
        // Check mod/admin permissions
        let user_tags = UserTagRepository::new(self.state.clone());
        let is_admin = user_tags.is_admin(requestor.id).await?;
        let is_moderator = user_tags.is_moderator(requestor.id).await?;

        if !(is_admin || is_moderator) {
            return Err(forbidden("Only moderators and admins can view audit logs"));
        }

        let record = sqlx::query!(
            r#"
            select
                id,
                user_id,
                action as "action: AuditActionType",
                action_at,
                resource_type as "resource_type: AuditableResource",
                resource_id,
                details
            from audit_logs
            where id = $1
            "#,
            id
        )
        .fetch_optional(&self.state.pool)
        .await?
        .ok_or_else(|| not_found("Audit log not found"))?;

        let user = self.fetch_user(record.user_id).await?;
        let resource = self
            .resolve_resource(record.resource_type, record.resource_id)
            .await;

        Ok(AuditLog {
            id: record.id,
            user_id: record.user_id,
            action: record.action,
            action_at: record.action_at,
            resource_type: record.resource_type,
            resource_id: record.resource_id,
            details: record.details,
            user,
            resource,
        })
    }

    /// List all audit logs with pagination. Only accessible to mods/admins.
    #[allow(dead_code)]
    pub async fn list(
        &self,
        requestor: &User,
        pagination: PaginatedRequest,
    ) -> AppResult<PaginatedResponse<AuditLog>> {
        // Check mod/admin permissions
        let user_tags = UserTagRepository::new(self.state.clone());
        let is_admin = user_tags.is_admin(requestor.id).await?;
        let is_moderator = user_tags.is_moderator(requestor.id).await?;

        if !(is_admin || is_moderator) {
            return Err(forbidden("Only moderators and admins can view audit logs"));
        }

        let records = sqlx::query!(
            r#"
            select
                id,
                user_id,
                action as "action: AuditActionType",
                action_at,
                resource_type as "resource_type: AuditableResource",
                resource_id,
                details
            from audit_logs
            order by action_at desc
            limit $1 offset $2
            "#,
            i64::from(pagination.limit),
            i64::from(pagination.offset)
        )
        .fetch_all(&self.state.pool)
        .await?;

        let mut logs = Vec::new();
        for record in records {
            let user = self.fetch_user(record.user_id).await?;
            let resource = self
                .resolve_resource(record.resource_type, record.resource_id)
                .await;
            logs.push(AuditLog {
                id: record.id,
                user_id: record.user_id,
                action: record.action,
                action_at: record.action_at,
                resource_type: record.resource_type,
                resource_id: record.resource_id,
                details: record.details,
                user,
                resource,
            });
        }

        let total = sqlx::query_scalar!("select count(*) from audit_logs")
            .fetch_one(&self.state.pool)
            .await?
            .unwrap_or(0);

        let has_more =
            (i64::from(pagination.offset) + i64::try_from(logs.len()).unwrap_or(i64::MAX)) < total;

        Ok(PaginatedResponse {
            items: logs,
            total,
            offset: pagination.offset,
            limit: pagination.limit,
            has_more,
        })
    }

    /// Search audit logs with filters and pagination. Only accessible to mods/admins.
    pub async fn search(
        &self,
        requestor: &User,
        pagination: PaginatedRequest,
        filter: AuditLogFilter,
    ) -> AppResult<PaginatedResponse<AuditLog>> {
        // Check mod/admin permissions
        let user_tags = UserTagRepository::new(self.state.clone());
        let is_admin = user_tags.is_admin(requestor.id).await?;
        let is_moderator = user_tags.is_moderator(requestor.id).await?;

        if !(is_admin || is_moderator) {
            return Err(forbidden("Only moderators and admins can view audit logs"));
        }

        // Build dynamic query based on filters
        let records = sqlx::query!(
            r#"
            select
                id,
                user_id,
                action as "action: AuditActionType",
                action_at,
                resource_type as "resource_type: AuditableResource",
                resource_id,
                details
            from audit_logs
            where
                ($1::uuid is null or user_id = $1)
                and ($2::audit_action_type is null or action = $2)
                and ($3::auditable_resource is null or resource_type = $3)
                and ($4::uuid is null or resource_id = $4)
            order by action_at desc
            limit $5 offset $6
            "#,
            filter.user_id,
            filter.action as Option<AuditActionType>,
            filter.resource_type as Option<AuditableResource>,
            filter.resource_id,
            i64::from(pagination.limit),
            i64::from(pagination.offset)
        )
        .fetch_all(&self.state.pool)
        .await?;

        let mut logs = Vec::new();
        for record in records {
            let user = self.fetch_user(record.user_id).await?;
            let resource = self
                .resolve_resource(record.resource_type, record.resource_id)
                .await;
            logs.push(AuditLog {
                id: record.id,
                user_id: record.user_id,
                action: record.action,
                action_at: record.action_at,
                resource_type: record.resource_type,
                resource_id: record.resource_id,
                details: record.details,
                user,
                resource,
            });
        }

        let total = sqlx::query_scalar!(
            r#"
            select count(*)
            from audit_logs
            where
                ($1::uuid is null or user_id = $1)
                and ($2::audit_action_type is null or action = $2)
                and ($3::auditable_resource is null or resource_type = $3)
                and ($4::uuid is null or resource_id = $4)
            "#,
            filter.user_id,
            filter.action as Option<AuditActionType>,
            filter.resource_type as Option<AuditableResource>,
            filter.resource_id
        )
        .fetch_one(&self.state.pool)
        .await?
        .unwrap_or(0);

        let has_more =
            (i64::from(pagination.offset) + i64::try_from(logs.len()).unwrap_or(i64::MAX)) < total;

        Ok(PaginatedResponse {
            items: logs,
            total,
            offset: pagination.offset,
            limit: pagination.limit,
            has_more,
        })
    }
}

crate::util::repo_from_parts!(AuditLogRepository);
