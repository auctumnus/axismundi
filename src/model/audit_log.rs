use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sqlx::FromRow;
use uuid::Uuid;
use validator::Validate;

use crate::AppState;
use crate::err::{AppResult, forbidden, not_found};
use crate::model::user_tags::UserTagRepository;
use crate::model::users::User;
use crate::pagination::{PaginatedRequest, PaginatedResponse};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "auditable_resource", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum AuditableResource {
    User,
    Language,
    Word,
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
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct AuditLog {
    pub id: Uuid,
    pub user_id: Option<Uuid>,
    pub action: AuditActionType,
    pub action_at: DateTime<Utc>,
    pub resource_type: AuditableResource,
    pub resource_id: Uuid,
    pub details: JsonValue,
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

    /// Create a new audit log entry without permission checks.
    /// Use this when you've already verified the user is admin/mod.
    pub(crate) async fn create_internal(&self, req: CreateAuditLog) -> AppResult<AuditLog> {
        req.validate()?;

        let log = sqlx::query_as!(
            AuditLog,
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
        .fetch_one(&self.state.pool)
        .await?;

        Ok(log)
    }

    /// Create a new audit log entry. Only callable by mods/admins.
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

        let log = sqlx::query_as!(
            AuditLog,
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

        Ok(log)
    }

    /// List all audit logs with pagination. Only accessible to mods/admins.
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

        let logs = sqlx::query_as!(
            AuditLog,
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
        let logs = sqlx::query_as!(
            AuditLog,
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
