use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use validator::Validate;

use crate::AppState;
use crate::err::{AppResult, bad_request, forbidden, not_found};
use crate::model::user_tags::UserTagRepository;
use crate::model::users::User;
use crate::pagination::{PaginatedRequest, PaginatedResponse};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "reportable_resource", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ReportableResource {
    User,
    Language,
    Word,
    Translation,
    Translatable,
    WordRelation,
    Invite,
    Permission,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "resolution_status_type", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ResolutionStatus {
    Pending,
    InProgress,
    Dismissed,
    ActionTaken,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "report_priority", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ReportPriority {
    Low,
    Medium,
    High,
    Urgent,
}

/// Main report struct. Fields are Option<> when they might be hidden from non-mod users.
/// Use ReportRepository methods to fetch - they'll automatically populate or hide fields
/// based on the requestor's permissions.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Report {
    pub id: Uuid,
    pub reporter: Option<Uuid>,
    pub resource_type: ReportableResource,
    pub resource_id: Uuid,
    pub reason: String,
    pub reported_at: DateTime<Utc>,

    // Hidden from normal users
    pub priority: Option<ReportPriority>,

    // Hidden from normal users
    pub resolved_by: Option<Uuid>,

    // Shown to normal users only if resolution_status_hidden is false
    pub resolution_status: Option<ResolutionStatus>,
    pub resolved_at: Option<DateTime<Utc>>,

    // Shown to normal users only if resolution_note_hidden is false
    pub resolution_note: Option<String>,

    pub user_updated_at: Option<DateTime<Utc>>,

    // Hidden from normal users
    pub mods_updated_at: Option<DateTime<Utc>>,
    pub mods_updated_by: Option<Uuid>,

    // These fields control visibility - they're always present for mods but
    // not exposed to regular users
    #[serde(skip_serializing)]
    pub resolution_status_hidden: Option<bool>,
    #[serde(skip_serializing)]
    pub resolution_note_hidden: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CreateReport {
    pub resource_type: ReportableResource,
    pub resource_id: Uuid,
    #[validate(length(min = 1, max = 5000))]
    pub reason: String,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct UpdateReportModerator {
    pub priority: Option<ReportPriority>,
    pub resolution_status: Option<ResolutionStatus>,
    pub resolution_note: Option<String>,
    pub resolution_status_hidden: Option<bool>,
    pub resolution_note_hidden: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ReportSearch {
    pub text_query: Option<String>,
    pub resource_type: Option<ReportableResource>,
    pub resource_id: Option<Uuid>,
    pub reporter: Option<Uuid>,
    pub resolution_status: Option<ResolutionStatus>,
    pub priority: Option<ReportPriority>,
}

pub struct ReportRepository {
    state: AppState,
}

impl ReportRepository {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    /// Create a new report
    pub async fn create(&self, reporter: &User, req: CreateReport) -> AppResult<Report> {
        req.validate()?;

        let report = sqlx::query_as!(
            Report,
            r#"
            insert into reports (reporter, resource_type, resource_id, reason)
            values ($1, $2, $3, $4)
            returning
                id,
                reporter,
                resource_type as "resource_type: ReportableResource",
                resource_id,
                reason,
                reported_at,
                priority as "priority: ReportPriority",
                resolved_by,
                resolution_status as "resolution_status: ResolutionStatus",
                resolved_at,
                resolution_note,
                user_updated_at,
                mods_updated_at,
                mods_updated_by,
                resolution_status_hidden,
                resolution_note_hidden
            "#,
            reporter.id,
            req.resource_type as ReportableResource,
            req.resource_id,
            req.reason
        )
        .fetch_one(&self.state.pool)
        .await?;

        // Sanitize the report for the reporter (they're not a mod)
        Ok(self.sanitize_for_user(&report, reporter))
    }

    /// Find a report by ID. Automatically filters based on requestor permissions.
    /// - Mods/admins can see any report with all fields
    /// - Users can only see their own reports, with fields hidden
    pub async fn find_by_id(&self, requestor: &User, id: Uuid) -> AppResult<Report> {
        let user_tags = UserTagRepository::new(self.state.clone());

        let report = sqlx::query_as!(
            Report,
            r#"
            select
                id,
                reporter,
                resource_type as "resource_type: ReportableResource",
                resource_id,
                reason,
                reported_at,
                priority as "priority: ReportPriority",
                resolved_by,
                resolution_status as "resolution_status: ResolutionStatus",
                resolved_at,
                resolution_note,
                user_updated_at,
                mods_updated_at,
                mods_updated_by,
                resolution_status_hidden,
                resolution_note_hidden
            from reports
            where id = $1
            "#,
            id
        )
        .fetch_optional(&self.state.pool)
        .await?
        .ok_or_else(|| not_found("Report not found"))?;

        // Check permissions
        let is_admin = user_tags.is_admin(requestor.id).await?;
        let is_moderator = user_tags.is_moderator(requestor.id).await?;
        let is_mod = is_admin || is_moderator;

        if !is_mod {
            // Regular users can only see their own reports
            if report.reporter != Some(requestor.id) {
                return Err(forbidden("You can only view your own reports"));
            }
            return Ok(self.sanitize_for_user(&report, requestor));
        }

        // Mods/admins see everything
        Ok(report)
    }

    /// Search all reports. Only accessible to mods/admins.
    pub async fn search(
        &self,
        requestor: &User,
        pagination: PaginatedRequest,
        search: ReportSearch,
    ) -> AppResult<PaginatedResponse<Report>> {
        let user_tags = UserTagRepository::new(self.state.clone());

        let is_admin = user_tags.is_admin(requestor.id).await?;
        let is_moderator = user_tags.is_moderator(requestor.id).await?;

        if !(is_admin || is_moderator) {
            return Err(forbidden("Only moderators can search all reports"));
        }

        let items_future = sqlx::query_as!(
            Report,
            r#"
            select
                id,
                reporter,
                resource_type as "resource_type: ReportableResource",
                resource_id,
                reason,
                reported_at,
                priority as "priority: ReportPriority",
                resolved_by,
                resolution_status as "resolution_status: ResolutionStatus",
                resolved_at,
                resolution_note,
                user_updated_at,
                mods_updated_at,
                mods_updated_by,
                resolution_status_hidden,
                resolution_note_hidden
            from reports
            where
                ($1::text is null or reason ilike '%' || $1 || '%')
                and ($2::reportable_resource is null or resource_type = $2)
                and ($3::uuid is null or resource_id = $3)
                and ($4::uuid is null or reporter = $4)
                and ($5::resolution_status_type is null or resolution_status = $5)
                and ($6::report_priority is null or priority = $6)
            order by reported_at desc
            limit $7 offset $8
            "#,
            search.text_query,
            search.resource_type as Option<ReportableResource>,
            search.resource_id,
            search.reporter,
            search.resolution_status as Option<ResolutionStatus>,
            search.priority as Option<ReportPriority>,
            i64::from(pagination.limit),
            i64::from(pagination.offset)
        )
        .fetch_all(&self.state.pool);

        let count_future = sqlx::query_scalar!(
            r#"
            select count(*)
            from reports
            where
                ($1::text is null or reason ilike '%' || $1 || '%')
                and ($2::reportable_resource is null or resource_type = $2)
                and ($3::uuid is null or resource_id = $3)
                and ($4::uuid is null or reporter = $4)
                and ($5::resolution_status_type is null or resolution_status = $5)
                and ($6::report_priority is null or priority = $6)
            "#,
            search.text_query,
            search.resource_type as Option<ReportableResource>,
            search.resource_id,
            search.reporter,
            search.resolution_status as Option<ResolutionStatus>,
            search.priority as Option<ReportPriority>
        )
        .fetch_one(&self.state.pool);

        let (items, total_count) = tokio::try_join!(items_future, count_future)?;

        let total = total_count.unwrap_or(0);
        let has_more =
            (i64::from(pagination.offset) + i64::try_from(items.len()).unwrap_or(i64::MAX)) < total;

        Ok(PaginatedResponse {
            items,
            total,
            offset: pagination.offset,
            limit: pagination.limit,
            has_more,
        })
    }

    /// Search a user's own reports. Returns sanitized data.
    pub async fn search_own(
        &self,
        user: &User,
        pagination: PaginatedRequest,
        search: ReportSearch,
    ) -> AppResult<PaginatedResponse<Report>> {
        let items_future = sqlx::query_as!(
            Report,
            r#"
            select
                id,
                reporter,
                resource_type as "resource_type: ReportableResource",
                resource_id,
                reason,
                reported_at,
                priority as "priority: ReportPriority",
                resolved_by,
                resolution_status as "resolution_status: ResolutionStatus",
                resolved_at,
                resolution_note,
                user_updated_at,
                mods_updated_at,
                mods_updated_by,
                resolution_status_hidden,
                resolution_note_hidden
            from reports
            where
                reporter = $1
                and ($2::text is null or reason ilike '%' || $2 || '%')
                and ($3::reportable_resource is null or resource_type = $3)
                and ($4::uuid is null or resource_id = $4)
                and ($5::resolution_status_type is null or resolution_status = $5)
            order by reported_at desc
            limit $6 offset $7
            "#,
            user.id,
            search.text_query,
            search.resource_type as Option<ReportableResource>,
            search.resource_id,
            search.resolution_status as Option<ResolutionStatus>,
            i64::from(pagination.limit),
            i64::from(pagination.offset)
        )
        .fetch_all(&self.state.pool);

        let count_future = sqlx::query_scalar!(
            r#"
            select count(*)
            from reports
            where
                reporter = $1
                and ($2::text is null or reason ilike '%' || $2 || '%')
                and ($3::reportable_resource is null or resource_type = $3)
                and ($4::uuid is null or resource_id = $4)
                and ($5::resolution_status_type is null or resolution_status = $5)
            "#,
            user.id,
            search.text_query,
            search.resource_type as Option<ReportableResource>,
            search.resource_id,
            search.resolution_status as Option<ResolutionStatus>
        )
        .fetch_one(&self.state.pool);

        let (items, total_count) = tokio::try_join!(items_future, count_future)?;

        // Sanitize all reports for the user
        let sanitized_items: Vec<Report> = items
            .into_iter()
            .map(|r| self.sanitize_for_user(&r, user))
            .collect();

        let total = total_count.unwrap_or(0);
        let has_more = (i64::from(pagination.offset)
            + i64::try_from(sanitized_items.len()).unwrap_or(i64::MAX))
            < total;

        Ok(PaginatedResponse {
            items: sanitized_items,
            total,
            offset: pagination.offset,
            limit: pagination.limit,
            has_more,
        })
    }

    /// Update a report. Only accessible to mods/admins.
    pub async fn update(
        &self,
        requestor: &User,
        id: Uuid,
        req: UpdateReportModerator,
    ) -> AppResult<Report> {
        let user_tags = UserTagRepository::new(self.state.clone());

        let is_admin = user_tags.is_admin(requestor.id).await?;
        let is_moderator = user_tags.is_moderator(requestor.id).await?;

        if !(is_admin || is_moderator) {
            return Err(forbidden("Only moderators can update reports"));
        }

        req.validate()?;

        // Validate the constraint: can't have visible note if status is hidden
        if req.resolution_note_hidden == Some(false) && req.resolution_status_hidden == Some(true) {
            return Err(bad_request(
                "Resolution note cannot be visible if resolution status is hidden",
            ));
        }

        // Fetch existing report
        let mut report = sqlx::query_as!(
            Report,
            r#"
            select
                id,
                reporter,
                resource_type as "resource_type: ReportableResource",
                resource_id,
                reason,
                reported_at,
                priority as "priority: ReportPriority",
                resolved_by,
                resolution_status as "resolution_status: ResolutionStatus",
                resolved_at,
                resolution_note,
                user_updated_at,
                mods_updated_at,
                mods_updated_by,
                resolution_status_hidden,
                resolution_note_hidden
            from reports
            where id = $1
            "#,
            id
        )
        .fetch_optional(&self.state.pool)
        .await?
        .ok_or_else(|| not_found("Report not found"))?;

        // Update fields if provided
        if let Some(priority) = req.priority {
            report.priority = Some(priority);
        }
        if let Some(status) = req.resolution_status {
            report.resolution_status = Some(status);
        }
        if let Some(note) = req.resolution_note {
            report.resolution_note = Some(note);
        }
        if let Some(hidden) = req.resolution_status_hidden {
            report.resolution_status_hidden = Some(hidden);
        }
        if let Some(hidden) = req.resolution_note_hidden {
            report.resolution_note_hidden = Some(hidden);
        }

        // If we're marking as resolved/dismissed/action_taken, set resolved_at and resolved_by
        // If we're marking as pending/in_progress, clear those fields
        let (resolved_at, resolved_by) = match report.resolution_status {
            Some(ResolutionStatus::Pending) | Some(ResolutionStatus::InProgress) => {
                // Clear resolution fields
                (None, None)
            }
            Some(ResolutionStatus::Dismissed) | Some(ResolutionStatus::ActionTaken) => {
                // Set resolution fields if not already set
                let resolved_at = if report.resolved_at.is_none() {
                    Some(Utc::now())
                } else {
                    report.resolved_at
                };
                let resolved_by = if report.resolved_by.is_none() {
                    Some(requestor.id)
                } else {
                    report.resolved_by
                };
                (resolved_at, resolved_by)
            }
            None => {
                // No status set, keep existing values
                (report.resolved_at, report.resolved_by)
            }
        };

        // Update in database
        let updated = sqlx::query_as!(
            Report,
            r#"
            update reports
            set
                priority = $1,
                resolution_status = $2,
                resolved_at = $3,
                resolved_by = $4,
                resolution_note = $5,
                resolution_status_hidden = $6,
                resolution_note_hidden = $7,
                mods_updated_at = current_timestamp,
                mods_updated_by = $8
            where id = $9
            returning
                id,
                reporter,
                resource_type as "resource_type: ReportableResource",
                resource_id,
                reason,
                reported_at,
                priority as "priority: ReportPriority",
                resolved_by,
                resolution_status as "resolution_status: ResolutionStatus",
                resolved_at,
                resolution_note,
                user_updated_at,
                mods_updated_at,
                mods_updated_by,
                resolution_status_hidden,
                resolution_note_hidden
            "#,
            report.priority as Option<ReportPriority>,
            report.resolution_status as Option<ResolutionStatus>,
            resolved_at,
            resolved_by,
            report.resolution_note,
            report.resolution_status_hidden,
            report.resolution_note_hidden,
            requestor.id,
            id
        )
        .fetch_one(&self.state.pool)
        .await?;

        Ok(updated)
    }

    /// Delete a report. Only accessible to admins.
    pub async fn delete(&self, requestor: &User, id: Uuid) -> AppResult<()> {
        let user_tags = UserTagRepository::new(self.state.clone());

        // Only admins can delete reports (not just mods)
        if !user_tags.is_admin(requestor.id).await? {
            return Err(forbidden("Only administrators can delete reports"));
        }

        sqlx::query!("delete from reports where id = $1", id)
            .execute(&self.state.pool)
            .await?;

        Ok(())
    }

    /// Sanitize a report for a regular user (hide mod-only fields)
    fn sanitize_for_user(&self, report: &Report, _user: &User) -> Report {
        let mut sanitized = report.clone();

        // Always hide mod-only fields
        sanitized.priority = None;
        sanitized.resolved_by = None;
        sanitized.mods_updated_at = None;
        sanitized.mods_updated_by = None;

        // Hide resolution status/resolved_at if resolution_status_hidden is true
        if report.resolution_status_hidden.unwrap_or(false) {
            sanitized.resolution_status = None;
            sanitized.resolved_at = None;
        }

        // Hide resolution note if resolution_note_hidden is true
        if report.resolution_note_hidden.unwrap_or(false) {
            sanitized.resolution_note = None;
        }

        // Don't expose the hidden flags to users
        sanitized.resolution_status_hidden = None;
        sanitized.resolution_note_hidden = None;

        sanitized
    }
}

crate::util::repo_from_parts!(ReportRepository);
