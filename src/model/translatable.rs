use askama::Template;
use chrono::{DateTime, NaiveDate, Utc};
use nanoid::nanoid;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use validator::Validate;

use crate::{
    err::{AppResult, bad_request, forbidden, not_found},
    model::users::User,
    pagination::{PaginatedRequest, PaginatedResponse},
    util::{AppState, ensure_verified, sanitize_external_url},
};

fn validate_external_url(url: &str) -> Result<(), validator::ValidationError> {
    if url.is_empty() {
        return Ok(());
    }
    sanitize_external_url(url)?;
    Ok(())
}

/// `source_url` is `not null default ''`, so the empty string — not None — is the
/// "no source url" state. Keep `Some("")` intact: folding it to None would mean
/// "leave unchanged" to the COALESCE in `update`, making the field impossible to clear.
fn sanitize_source_url(url: Option<String>) -> AppResult<Option<String>> {
    url.map(|u| {
        if u.is_empty() {
            Ok(u)
        } else {
            sanitize_external_url(&u).map_err(|e| bad_request(e.to_string()))
        }
    })
    .transpose()
}

fn is_staff(user: Option<&User>) -> bool {
    user.is_some_and(|u| u.is_admin() || u.is_moderator())
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Translatable {
    pub id: Uuid,
    pub slug: String,
    pub title: String,
    pub english: String,
    pub source_name: String,
    pub source_url: String,
    pub source_content: String,
    pub source_language: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub translations_count: i64,
    pub description: String,
    pub like_count: i64,
    pub published_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing)]
    pub created_by: Uuid,
    #[serde(skip_serializing)]
    pub updated_by: Uuid,
}

/// Draft visibility filter for translatable searches. `PublishedOnly` is
/// the default and the only value the public API is allowed to use; staff
/// can request drafts via the admin queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DraftFilter {
    #[default]
    PublishedOnly,
    DraftsOnly,
    All,
}

#[derive(Debug, Clone, Serialize)]
pub struct TranslatableWithMeta {
    pub translatable: Translatable,
    pub is_liked: bool,
    pub creator: User,
    pub updater: Option<User>,
}

impl Translatable {
    pub fn words_count(&self) -> usize {
        self.english.split_whitespace().count()
    }

    pub fn is_published(&self) -> bool {
        self.published_at.is_some()
    }

    pub fn is_draft(&self) -> bool {
        self.published_at.is_none()
    }
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CreateTranslatable {
    #[validate(length(min = 1, max = 40))]
    pub title: String,

    #[validate(length(min = 1, max = 100_000))]
    pub english: String,

    #[validate(length(max = 1000))]
    pub source_name: Option<String>,

    #[validate(custom(function = "validate_external_url"))]
    #[validate(length(max = 2000))]
    pub source_url: Option<String>,

    #[validate(length(max = 100_000))]
    pub source_content: Option<String>,

    #[validate(length(max = 100))]
    pub source_language: Option<String>,

    #[validate(length(max = 1000))]
    pub description: Option<String>,

    /// Staff-only: when true, the translatable is created as a draft
    /// (published_at = NULL). Controllers MUST force this to false for
    /// non-staff requestors. Activity logging is deferred until publish.
    #[serde(default)]
    pub as_draft: bool,
}

#[derive(Debug, Serialize, Deserialize, Validate, Clone)]
pub struct UpdateTranslatable {
    #[validate(length(min = 1, max = 40))]
    pub title: Option<String>,

    #[validate(length(min = 1, max = 100_000))]
    pub english: Option<String>,

    #[validate(length(max = 1000))]
    pub source_name: Option<String>,

    #[validate(custom(function = "validate_external_url"))]
    #[validate(length(max = 2000))]
    pub source_url: Option<String>,

    #[validate(length(max = 100_000))]
    pub source_content: Option<String>,

    #[validate(length(max = 100))]
    pub source_language: Option<String>,

    #[validate(length(max = 1000))]
    pub description: Option<String>,
}

pub struct TranslatableRepository {
    state: AppState,
}

impl TranslatableRepository {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    pub async fn materialize(
        &self,
        translatable: Translatable,
        requestor: Option<&User>,
    ) -> AppResult<TranslatableWithMeta> {
        let is_liked = if let Some(user) = requestor {
            self.is_liked(&translatable.id, &user.id).await?
        } else {
            false
        };
        let users = crate::model::users::UserRepository::new(self.state.clone());
        let creator = users.find_by_id(translatable.created_by).await?;
        let updater = if translatable.updated_by != translatable.created_by {
            Some(users.find_by_id(translatable.updated_by).await?)
        } else {
            None
        };
        Ok(TranslatableWithMeta {
            translatable,
            is_liked,
            creator,
            updater,
        })
    }

    pub async fn find_by_id(&self, id: Uuid) -> AppResult<Translatable> {
        let result = sqlx::query_as!(
            Translatable,
            r#"
                SELECT
                    id,
                    slug,
                    title,
                    english,
                    source_name,
                    source_url,
                    source_content,
                    source_language,
                    created_at,
                    updated_at,
                    created_by,
                    updated_by,
                    like_count,
                    description,
                    published_at,
                    (
                        SELECT COUNT(*)
                        FROM translation
                        WHERE translation.translatable = translatable.id
                    ) AS "translations_count!"
                FROM translatable
                WHERE id = $1
            "#,
            id
        )
        .fetch_optional(&self.state.pool)
        .await?;

        result.ok_or_else(|| not_found(format!("translatable with id '{id}'")))
    }

    /// Returns the translatable only if it is visible to `requestor`.
    /// Drafts are visible only to staff (admins/mods). Otherwise returns
    /// `NotFound` to avoid leaking existence.
    #[allow(dead_code)]
    pub async fn find_by_id_for(
        &self,
        id: Uuid,
        requestor: Option<&User>,
    ) -> AppResult<Translatable> {
        let translatable = self.find_by_id(id).await?;
        if translatable.is_draft() && !is_staff(requestor) {
            return Err(not_found(format!("translatable with id '{id}'")));
        }
        Ok(translatable)
    }

    pub async fn find_by_slug(&self, slug: &str) -> AppResult<Translatable> {
        let result = sqlx::query_as!(
            Translatable,
            r#"
                SELECT
                    id,
                    slug,
                    title,
                    english,
                    source_name,
                    source_url,
                    source_content,
                    source_language,
                    created_at,
                    updated_at,
                    created_by,
                    updated_by,
                    like_count,
                    description,
                    published_at,
                    (
                        SELECT COUNT(*)
                        FROM translation
                        WHERE translation.translatable = translatable.id
                    ) AS "translations_count!"
                FROM translatable
                WHERE slug = $1
            "#,
            slug
        )
        .fetch_optional(&self.state.pool)
        .await?;

        result.ok_or_else(|| not_found(format!("translatable with slug '{slug}'")))
    }

    /// Visibility-aware lookup. Drafts return `NotFound` for non-staff.
    pub async fn find_by_slug_for(
        &self,
        slug: &str,
        requestor: Option<&User>,
    ) -> AppResult<Translatable> {
        let translatable = self.find_by_slug(slug).await?;
        if translatable.is_draft() && !is_staff(requestor) {
            return Err(not_found(format!("translatable with slug '{slug}'")));
        }
        Ok(translatable)
    }

    pub async fn create(
        &self,
        requestor: &User,
        translatable: CreateTranslatable,
    ) -> AppResult<Translatable> {
        translatable.validate()?;
        let source_url = sanitize_source_url(translatable.source_url)?;

        ensure_verified(requestor)?;

        crate::model::user_bans::UserBanRepository::new(self.state.clone())
            .ensure_not_banned(requestor.id)
            .await?;

        let slug = slug::slugify(&translatable.title);
        let slug = format!("{slug}-{}", nanoid!(6));

        // staff-only: drafts have published_at = NULL. controllers MUST
        // force as_draft=false for non-staff requestors.
        let as_draft = translatable.as_draft;

        let result = sqlx::query_as!(
            Translatable,
            r#"
                INSERT INTO translatable (slug, title, english, source_name, source_url, source_content, source_language, description, created_by, updated_by, published_at)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $9, CASE WHEN $10::bool THEN NULL ELSE CURRENT_TIMESTAMP END)
                RETURNING id, slug, title, english, source_name, source_url, source_content, source_language, created_at, updated_at, created_by, updated_by, like_count, description, published_at, 0 AS "translations_count!"
            "#,
            slug,
            translatable.title,
            translatable.english,
            translatable.source_name.unwrap_or_default(),
            source_url.unwrap_or_default(),
            translatable.source_content.unwrap_or_default(),
            translatable.source_language.unwrap_or_default(),
            translatable.description.unwrap_or_default(),
            requestor.id,
            as_draft
        )
        .fetch_one(&self.state.pool)
        .await?;

        // log a CreateTranslatable activity only for published translatables.
        // drafts defer their activity until publish().
        if !as_draft {
            let activity_repo =
                crate::model::user_activities::UserActivityRepository::new(self.state.clone());
            let _activity = activity_repo
                .create(
                    requestor.id,
                    crate::model::user_activities::ActivityType::CreateTranslatable,
                    result.id,
                    "translatable",
                    None,
                    None,
                )
                .await?;
        } else {
            // every draft joins the totd queue with a stable sort_key
            sqlx::query!(
                r#"
                    INSERT INTO totd_queue (translatable_id, sort_key)
                    SELECT $1::uuid, hashtextextended($1::uuid::text || c.seed, 0)
                    FROM totd_queue_config c
                "#,
                result.id,
            )
            .execute(&self.state.pool)
            .await?;
        }

        Ok(result)
    }

    /// Staff-only atomic equivalent of `create()` for drafts that should be
    /// scheduled as TotD on a specific date. Either both the draft and the
    /// schedule row land, or neither does — avoids leaking an unscheduled
    /// draft when the chosen date is already taken.
    pub async fn create_and_schedule(
        &self,
        requestor: &User,
        translatable: CreateTranslatable,
        scheduled_date: NaiveDate,
    ) -> AppResult<Translatable> {
        if !is_staff(Some(requestor)) {
            return Err(forbidden(
                "only admins or moderators can schedule a translatable on creation",
            ));
        }

        translatable.validate()?;
        let source_url = sanitize_source_url(translatable.source_url)?;

        ensure_verified(requestor)?;

        crate::model::user_bans::UserBanRepository::new(self.state.clone())
            .ensure_not_banned(requestor.id)
            .await?;

        let today = Utc::now().date_naive();
        if scheduled_date < today {
            return Err(bad_request("cannot schedule TotD for a past date"));
        }

        let slug = slug::slugify(&translatable.title);
        let slug = format!("{slug}-{}", nanoid!(6));

        let mut tx = self.state.pool.begin().await?;

        let result = sqlx::query_as!(
            Translatable,
            r#"
                INSERT INTO translatable (slug, title, english, source_name, source_url, source_content, source_language, description, created_by, updated_by, published_at)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $9, NULL)
                RETURNING id, slug, title, english, source_name, source_url, source_content, source_language, created_at, updated_at, created_by, updated_by, like_count, description, published_at, 0 AS "translations_count!"
            "#,
            slug,
            translatable.title,
            translatable.english,
            translatable.source_name.unwrap_or_default(),
            source_url.unwrap_or_default(),
            translatable.source_content.unwrap_or_default(),
            translatable.source_language.unwrap_or_default(),
            translatable.description.unwrap_or_default(),
            requestor.id,
        )
        .fetch_one(&mut *tx)
        .await?;

        sqlx::query!(
            r#"
                INSERT INTO totd_queue
                    (translatable_id, sort_key, scheduled_date, assigned_by, assigned_at, is_auto)
                SELECT $1::uuid,
                       hashtextextended($1::uuid::text || c.seed, 0),
                       $2,
                       $3,
                       now(),
                       false
                FROM totd_queue_config c
            "#,
            result.id,
            scheduled_date,
            requestor.id,
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| match e {
            sqlx::Error::Database(ref db_err) if db_err.is_unique_violation() => {
                bad_request("that date is already scheduled")
            }
            other => other.into(),
        })?;

        // scheduling for today means the translatable is live now — publish
        // it so it isn't shown as a draft on its own day, and log the
        // deferred CreateTranslatable activity against the creator.
        let result = if scheduled_date == today {
            let published = sqlx::query_as!(
                Translatable,
                r#"
                    UPDATE translatable
                    SET published_at = CURRENT_TIMESTAMP
                    WHERE id = $1
                    RETURNING id, slug, title, english, source_name, source_url, source_content, source_language, created_at, updated_at, created_by, updated_by, like_count, description, published_at, 0 AS "translations_count!"
                "#,
                result.id,
            )
            .fetch_one(&mut *tx)
            .await?;

            sqlx::query!(
                r#"
                    INSERT INTO user_activities
                        (user_id, activity, entity_id, entity_type)
                    VALUES ($1, 'create_translatable', $2, 'translatable')
                "#,
                published.created_by,
                published.id,
            )
            .execute(&mut *tx)
            .await?;

            published
        } else {
            result
        };

        let audit = crate::model::audit_log::AuditLogRepository::new(self.state.clone());
        audit
            .create_internal_tx(
                &mut tx,
                crate::model::audit_log::CreateAuditLog {
                    user_id: Some(requestor.id),
                    action: crate::model::audit_log::AuditActionType::Created,
                    resource_type: crate::model::audit_log::AuditableResource::Translatable,
                    resource_id: result.id,
                    details: serde_json::json!({
                        "kind": "totd_schedule",
                        "date": scheduled_date.to_string(),
                    }),
                },
            )
            .await?;

        tx.commit().await?;

        Ok(result)
    }

    pub async fn update(
        &self,
        requestor: &User,
        id: Uuid,
        updates: UpdateTranslatable,
    ) -> AppResult<Translatable> {
        updates.validate()?;
        let source_url = sanitize_source_url(updates.source_url)?;

        ensure_verified(requestor)?;

        crate::model::user_bans::UserBanRepository::new(self.state.clone())
            .ensure_not_banned(requestor.id)
            .await?;

        let existing = self.find_by_id(id).await?;
        // Only allow creator to update
        if existing.created_by != requestor.id {
            return Err(crate::err::forbidden(
                "only the creator can update this translatable",
            ));
        }

        let slug = updates.title.as_ref().map(slug::slugify);

        let result = sqlx::query_as!(
            Translatable,
            r#"
                UPDATE translatable
                SET slug = COALESCE($2, slug),
                    title = COALESCE($3, title),
                    english = COALESCE($4, english),
                    source_name = COALESCE($5, source_name),
                    source_url = COALESCE($6, source_url),
                    source_content = COALESCE($7, source_content),
                    source_language = COALESCE($8, source_language),
                    description = COALESCE($9, description),
                    updated_by = $10,
                    updated_at = CURRENT_TIMESTAMP
                WHERE id = $1
                RETURNING id, slug, title, english, source_name, source_url, source_content, source_language, created_at, updated_at, created_by, updated_by, like_count, description, published_at,
                    (
                        SELECT COUNT(*)
                        FROM translation
                        WHERE translation.translatable = translatable.id
                    ) AS "translations_count!"
            "#,
            id,
            slug,
            updates.title,
            updates.english,
            updates.source_name,
            source_url,
            updates.source_content,
            updates.source_language,
            updates.description,
            requestor.id
        )
        .fetch_optional(&self.state.pool)
        .await?;

        let updated_translatable =
            result.ok_or_else(|| not_found(format!("translatable with id '{id}'")))?;

        Ok(updated_translatable)
    }

    pub async fn delete(&self, requestor: &User, translatable: Translatable) -> AppResult<bool> {
        ensure_verified(requestor)?;

        crate::model::user_bans::UserBanRepository::new(self.state.clone())
            .ensure_not_banned(requestor.id)
            .await?;

        // Only allow creator to delete
        if translatable.created_by != requestor.id {
            return Err(crate::err::forbidden(
                "only the creator can delete this translatable",
            ));
        }

        let result = sqlx::query!("DELETE FROM translatable WHERE id = $1", translatable.id)
            .execute(&self.state.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn search(
        &self,
        pagination: PaginatedRequest,
        search: TranslatableSearch,
        requestor: Option<&User>,
    ) -> AppResult<PaginatedResponse<Translatable>> {
        let owner = if let Some(ref username) = search.created_by {
            let user_repo = crate::model::users::UserRepository::new(self.state.clone());
            let user = user_repo.find_by_username(username).await?;
            Some(user.id)
        } else {
            None
        };

        // staff-only filter modes (DraftsOnly / All) are forced down to
        // PublishedOnly for non-staff requestors. callers may explicitly
        // pass a staff-only filter when they've already gated the route.
        let effective_filter = if is_staff(requestor) {
            search.draft_status
        } else {
            DraftFilter::PublishedOnly
        };
        let (include_published, include_drafts) = match effective_filter {
            DraftFilter::PublishedOnly => (true, false),
            DraftFilter::DraftsOnly => (false, true),
            DraftFilter::All => (true, true),
        };

        let items_future = sqlx::query_as!(
            Translatable,
            r#"
                SELECT
                    id,
                    slug,
                    title,
                    english,
                    source_name,
                    source_url,
                    source_content,
                    source_language,
                    created_at,
                    updated_at,
                    created_by,
                    updated_by,
                    like_count,
                    description,
                    published_at,
                    (
                        SELECT COUNT(*)
                        FROM translation
                        WHERE translation.translatable = translatable.id
                    ) AS "translations_count!"
                FROM translatable
                WHERE
                ($1::TIMESTAMPTZ IS NULL OR created_at < $1)
                AND ($2::TIMESTAMPTZ IS NULL OR created_at > $2)
                AND ($3::TEXT IS NULL OR source_language = $3)
                AND ($7::UUID IS NULL OR created_by = $7)
                AND (
                    ($8::bool AND published_at IS NOT NULL)
                    OR ($9::bool AND published_at IS NULL)
                )
                ORDER BY (
                    CASE
                        WHEN $4::TEXT IS NOT NULL AND english ILIKE '%' || $4 || '%' THEN 100.0
                        WHEN $4::TEXT IS NOT NULL AND source_content ILIKE '%' || $4 || '%' THEN 90.0
                        WHEN $4::TEXT IS NOT NULL AND source_name ILIKE '%' || $4 || '%' THEN 80.0
                        ELSE 0.0
                    END +
                    CASE WHEN $4::TEXT IS NOT NULL THEN
                        similarity(english, $4) * 3.0 +
                        COALESCE(similarity(source_content, $4), 0.0) * 2.0 +
                        COALESCE(similarity(source_name, $4), 0.0) * 1.0
                    ELSE 0.0
                    END
                ) DESC, id DESC
                LIMIT $5
                OFFSET $6
            "#,
            search.created_before,
            search.created_after,
            search.source_language,
            search.q,
            i64::from(pagination.limit),
            i64::from(pagination.offset),
            owner,
            include_published,
            include_drafts,
        )
        .fetch_all(&self.state.pool);

        let count_future = sqlx::query_scalar!(
            r#"
                SELECT COUNT(*)
                FROM translatable
                WHERE
                ($1::TIMESTAMPTZ IS NULL OR created_at < $1)
                AND ($2::TIMESTAMPTZ IS NULL OR created_at > $2)
                AND ($3::TEXT IS NULL OR source_language = $3)
                AND (
                    ($4::bool AND published_at IS NOT NULL)
                    OR ($5::bool AND published_at IS NULL)
                )
            "#,
            search.created_before,
            search.created_after,
            search.source_language,
            include_published,
            include_drafts,
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

    pub async fn is_liked(&self, translatable_id: &Uuid, user_id: &Uuid) -> AppResult<bool> {
        let result = sqlx::query!(
            r#"
                SELECT 1 as exists FROM translatable_likes
                WHERE translatable_id = $1 AND user_id = $2
            "#,
            translatable_id,
            user_id
        )
        .fetch_optional(&self.state.pool)
        .await?;

        Ok(result.is_some())
    }

    pub async fn like_translatable(
        &self,
        translatable_id: Uuid,
        user_id: Uuid,
    ) -> AppResult<Option<i64>> {
        crate::model::user_bans::UserBanRepository::new(self.state.clone())
            .ensure_not_banned(user_id)
            .await?;

        let mut tx = self.state.pool.begin().await?;
        let result = sqlx::query!(
            r#"
                INSERT INTO translatable_likes (translatable_id, user_id)
                VALUES ($1, $2)
                ON CONFLICT DO NOTHING
            "#,
            translatable_id,
            user_id
        )
        .execute(&mut *tx)
        .await?;

        let likes = if result.rows_affected() > 0 {
            let likes = sqlx::query_scalar!(
                r#"
                    UPDATE translatable
                    SET like_count = like_count + 1
                    WHERE id = $1
                    RETURNING like_count
                "#,
                translatable_id
            )
            .fetch_one(&mut *tx)
            .await?;

            Some(likes)
        } else {
            None
        };
        tx.commit().await?;

        Ok(likes)
    }

    pub async fn unlike_translatable(
        &self,
        translatable_id: Uuid,
        user_id: Uuid,
    ) -> AppResult<Option<i64>> {
        crate::model::user_bans::UserBanRepository::new(self.state.clone())
            .ensure_not_banned(user_id)
            .await?;

        let mut tx = self.state.pool.begin().await?;
        let result = sqlx::query!(
            r#"
                DELETE FROM translatable_likes
                WHERE translatable_id = $1 AND user_id = $2
            "#,
            translatable_id,
            user_id
        )
        .execute(&mut *tx)
        .await?;

        let likes = if result.rows_affected() > 0 {
            let likes = sqlx::query_scalar!(
                r#"
                    UPDATE translatable
                    SET like_count = GREATEST(like_count - 1, 0)
                    WHERE id = $1
                    RETURNING like_count
                "#,
                translatable_id
            )
            .fetch_one(&mut *tx)
            .await?;

            Some(likes)
        } else {
            None
        };
        tx.commit().await?;

        Ok(likes)
    }

    /// Staff-only: publish a draft translatable. Logs the deferred
    /// `CreateTranslatable` activity (as the *creator's* attribution, not
    /// the publishing staff member) and records the staff action in the
    /// audit log. Idempotent — returns the existing published_at if the
    /// translatable was already published.
    pub async fn publish(&self, requestor: &User, id: Uuid) -> AppResult<Translatable> {
        if !is_staff(Some(requestor)) {
            return Err(crate::err::forbidden(
                "only admins or moderators can publish translatables",
            ));
        }

        let existing = self.find_by_id(id).await?;
        if existing.is_published() {
            return Ok(existing);
        }

        let result = sqlx::query_as!(
            Translatable,
            r#"
                UPDATE translatable
                SET published_at = CURRENT_TIMESTAMP
                WHERE id = $1
                RETURNING id, slug, title, english, source_name, source_url, source_content, source_language, created_at, updated_at, created_by, updated_by, like_count, description, published_at,
                    (
                        SELECT COUNT(*)
                        FROM translation
                        WHERE translation.translatable = translatable.id
                    ) AS "translations_count!"
            "#,
            id,
        )
        .fetch_one(&self.state.pool)
        .await?;

        // attribute the activity to the original creator: the queue feature
        // exists so the creator's contribution surfaces on its public day.
        let activity_repo =
            crate::model::user_activities::UserActivityRepository::new(self.state.clone());
        let _ = activity_repo
            .create(
                existing.created_by,
                crate::model::user_activities::ActivityType::CreateTranslatable,
                result.id,
                "translatable",
                None,
                None,
            )
            .await?;

        // manual publish takes the translatable out of the totd queue. if it
        // was scheduled for a future date that schedule is dropped — the
        // translatable is public now and won't be re-featured.
        sqlx::query!(
            r#"DELETE FROM totd_queue WHERE translatable_id = $1"#,
            id,
        )
        .execute(&self.state.pool)
        .await?;

        let audit_repo = crate::model::audit_log::AuditLogRepository::new(self.state.clone());
        audit_repo
            .create_internal(crate::model::audit_log::CreateAuditLog {
                user_id: Some(requestor.id),
                action: crate::model::audit_log::AuditActionType::Updated,
                resource_type: crate::model::audit_log::AuditableResource::Translatable,
                resource_id: id,
                details: serde_json::json!({ "kind": "publish" }),
            })
            .await?;

        Ok(result)
    }

    /// Staff-only: unpublish a translatable, reverting it to draft state.
    /// Audit-logged. Does not touch the activity feed — already-logged
    /// CreateTranslatable activities are defensively filtered at
    /// resolve time.
    pub async fn unpublish(&self, requestor: &User, id: Uuid) -> AppResult<Translatable> {
        if !is_staff(Some(requestor)) {
            return Err(crate::err::forbidden(
                "only admins or moderators can unpublish translatables",
            ));
        }

        let result = sqlx::query_as!(
            Translatable,
            r#"
                UPDATE translatable
                SET published_at = NULL
                WHERE id = $1
                RETURNING id, slug, title, english, source_name, source_url, source_content, source_language, created_at, updated_at, created_by, updated_by, like_count, description, published_at,
                    (
                        SELECT COUNT(*)
                        FROM translation
                        WHERE translation.translatable = translatable.id
                    ) AS "translations_count!"
            "#,
            id,
        )
        .fetch_optional(&self.state.pool)
        .await?;

        let translatable =
            result.ok_or_else(|| not_found(format!("translatable with id '{id}'")))?;

        // unpublish puts the translatable back into the totd queue as
        // unscheduled. on conflict (previously featured, queue row still
        // there as history) the existing row is reset to unscheduled so it
        // can be featured again.
        sqlx::query!(
            r#"
                INSERT INTO totd_queue (translatable_id, sort_key)
                SELECT $1::uuid, hashtextextended($1::uuid::text || c.seed, 0)
                FROM totd_queue_config c
                ON CONFLICT (translatable_id) DO UPDATE
                    SET scheduled_date = NULL,
                        assigned_by = NULL,
                        assigned_at = NULL,
                        is_auto = false
            "#,
            id,
        )
        .execute(&self.state.pool)
        .await?;

        let audit_repo = crate::model::audit_log::AuditLogRepository::new(self.state.clone());
        audit_repo
            .create_internal(crate::model::audit_log::CreateAuditLog {
                user_id: Some(requestor.id),
                action: crate::model::audit_log::AuditActionType::Updated,
                resource_type: crate::model::audit_log::AuditableResource::Translatable,
                resource_id: id,
                details: serde_json::json!({ "kind": "unpublish" }),
            })
            .await?;

        Ok(translatable)
    }

    pub async fn as_json_ld(&self, translatable: &Translatable) -> AppResult<serde_json::Value> {
        let user_repo = crate::model::users::UserRepository::new(self.state.clone());
        let creator = user_repo.find_by_id(translatable.created_by).await?;

        let json_ld = serde_json::json!({
            "@context": "https://schema.org",
            "@type": "CreativeWork",
            "identifier": translatable.slug,
            "name": translatable.title,
            "text": translatable.english,
            "inLanguage": "en",
            "dateCreated": translatable.created_at.to_rfc3339(),
            "dateModified": translatable.updated_at.to_rfc3339(),
            "author": crate::model::users::UserRepository::as_json_ld(&creator),
            "url": format!("{}/translatable/{}", crate::config::CONFIG.public_url_base, translatable.slug),
        });

        Ok(json_ld)
    }
}

#[derive(Default, Debug, Deserialize, Serialize, Clone, Template)]
#[template(path = "translatables/fragments/query.html")]
pub struct TranslatableSearch {
    pub q: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::util::deserialize_optional_form_string"
    )]
    pub source_language: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::util::deserialize_optional_form_string"
    )]
    pub created_by: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::util::deserialize_optional_form_datetime"
    )]
    pub created_before: Option<DateTime<Utc>>,
    #[serde(
        default,
        deserialize_with = "crate::util::deserialize_optional_form_datetime"
    )]
    pub created_after: Option<DateTime<Utc>>,
    #[serde(default)]
    pub draft_status: DraftFilter,
}

crate::util::text_query!(TranslatableSearch);

crate::util::repo_from_parts!(TranslatableRepository);
