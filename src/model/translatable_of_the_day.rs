use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    err::{AppResult, bad_request, forbidden},
    model::{
        translatable::{Translatable, TranslatableRepository, TranslatableWithMeta},
        users::{User, UserRepository},
    },
    pagination::{PaginatedRequest, PaginatedResponse},
    util::AppState,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslatableOfTheDay {
    pub date: NaiveDate,
    pub translatable_id: Uuid,
    pub assigned_by: Option<Uuid>,
    pub assigned_at: Option<DateTime<Utc>>,
    pub is_auto: bool,
}

/// A featured entry materialised for display — pairs a schedule row with its
/// translatable (with meta) and resolved scheduler user.
#[derive(Debug, Clone, Serialize)]
pub struct TotdEntry {
    pub date: NaiveDate,
    pub translatable: TranslatableWithMeta,
    pub assigned_by: Option<User>,
    pub assigned_at: Option<DateTime<Utc>>,
    pub is_auto: bool,
}

/// Why a peek row appeared in the merge walk. The UI uses this to label
/// scheduled vs predicted picks, and to gate the unschedule button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum PeekKind {
    /// Admin-scheduled for this date (future or today, not auto-picked).
    Scheduled,
    /// Today's pick, already committed via auto-pick.
    AutoLockedIn,
    /// Predicted auto-pick for a future date — no queue row yet.
    AutoPredicted,
    /// No scheduled row and the unscheduled queue is empty for this date.
    Empty,
}

#[derive(Debug, Clone, Serialize)]
pub struct PeekRow {
    pub date: NaiveDate,
    pub entry: Option<TotdEntry>,
    pub kind: PeekKind,
}

impl PeekRow {
    /// True when admins should see an unschedule button — only future
    /// committed scheduled rows are reversible.
    pub fn can_unschedule(&self, today: &NaiveDate) -> bool {
        self.kind == PeekKind::Scheduled && self.date > *today
    }

    /// True when admins should see a schedule button — predicted picks
    /// for a future date have a translatable lined up but no committed
    /// queue row, so the click promotes the prediction to a real schedule.
    pub fn can_schedule(&self, today: &NaiveDate) -> bool {
        self.kind == PeekKind::AutoPredicted && self.date > *today
    }
}

pub struct TranslatableOfTheDayRepository {
    state: AppState,
}

fn is_staff(user: &User) -> bool {
    user.is_admin() || user.is_moderator()
}

/// Publishes a translatable currently sitting in the totd queue, and logs
/// the deferred `CreateTranslatable` activity against its creator. No-op
/// if already published. Used by both the auto-pick path and the
/// manual-schedule paths so a featured translatable always loses its draft
/// state on its scheduled day.
async fn publish_translatable_for_totd_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    translatable_id: Uuid,
) -> AppResult<()> {
    let published = sqlx::query!(
        r#"
            UPDATE translatable
            SET published_at = CURRENT_TIMESTAMP
            WHERE id = $1 AND published_at IS NULL
            RETURNING created_by
        "#,
        translatable_id,
    )
    .fetch_optional(&mut **tx)
    .await?;

    if let Some(row) = published {
        sqlx::query!(
            r#"
                INSERT INTO user_activities
                    (user_id, activity, entity_id, entity_type)
                VALUES ($1, 'create_translatable', $2, 'translatable')
            "#,
            row.created_by,
            translatable_id,
        )
        .execute(&mut **tx)
        .await?;
    }

    Ok(())
}

impl TranslatableOfTheDayRepository {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    /// Fetch today's TotD. If none is scheduled, atomically picks the next
    /// unscheduled draft by sort_key, claims today's slot, publishes the
    /// translatable, and returns it. Returns `Ok(None)` only if the draft
    /// queue is empty.
    pub async fn today(&self, requestor: Option<&User>) -> AppResult<Option<TotdEntry>> {
        let today = Utc::now().date_naive();
        self.for_date(today, requestor).await
    }

    /// Fetch the TotD for a specific date. For today's date, runs the lazy
    /// auto-pick if no row exists. For other dates, returns whatever's
    /// scheduled (or `Ok(None)`).
    pub async fn for_date(
        &self,
        date: NaiveDate,
        requestor: Option<&User>,
    ) -> AppResult<Option<TotdEntry>> {
        let today = Utc::now().date_naive();
        let translatable_id = if date == today {
            self.fetch_or_auto_pick_today().await?
        } else {
            sqlx::query_scalar!(
                r#"
                    SELECT translatable_id
                    FROM totd_queue
                    WHERE scheduled_date = $1
                "#,
                date
            )
            .fetch_optional(&self.state.pool)
            .await?
        };

        let Some(translatable_id) = translatable_id else {
            return Ok(None);
        };

        self.materialize_entry(date, translatable_id, requestor)
            .await
            .map(Some)
    }

    /// Schedule a draft translatable for a future (or today's) date. The
    /// caller must be staff. Errors if the date is in the past, the
    /// translatable isn't in the queue (already featured or never a draft),
    /// or the date is already taken.
    pub async fn schedule(
        &self,
        admin: &User,
        date: NaiveDate,
        translatable_id: Uuid,
    ) -> AppResult<TranslatableOfTheDay> {
        if !is_staff(admin) {
            return Err(forbidden("only admins or moderators can schedule TotD entries"));
        }
        let today = Utc::now().date_naive();
        if date < today {
            return Err(bad_request("cannot schedule TotD for a past date"));
        }

        let mut tx = self.state.pool.begin().await?;

        let row = sqlx::query!(
            r#"
                UPDATE totd_queue
                SET scheduled_date = $1,
                    assigned_by = $2,
                    assigned_at = now(),
                    is_auto = false
                WHERE translatable_id = $3 AND scheduled_date IS NULL
                RETURNING scheduled_date AS "scheduled_date!", translatable_id,
                          assigned_by, assigned_at, is_auto
            "#,
            date,
            admin.id,
            translatable_id,
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| match e {
            sqlx::Error::Database(ref db_err) if db_err.is_unique_violation() => {
                bad_request("that date is already scheduled")
            }
            other => other.into(),
        })?;

        let Some(row) = row else {
            return Err(bad_request(
                "that translatable isn't in the draft queue — it may already be featured or published",
            ));
        };

        // scheduling for today means the translatable is live now — publish
        // it so it doesn't appear as a draft on its own day.
        if date == today {
            publish_translatable_for_totd_tx(&mut tx, translatable_id).await?;
        }

        let audit = crate::model::audit_log::AuditLogRepository::new(self.state.clone());
        audit
            .create_internal_tx(
                &mut tx,
                crate::model::audit_log::CreateAuditLog {
                    user_id: Some(admin.id),
                    action: crate::model::audit_log::AuditActionType::Created,
                    resource_type: crate::model::audit_log::AuditableResource::Translatable,
                    resource_id: translatable_id,
                    details: serde_json::json!({
                        "kind": "totd_schedule",
                        "date": date.to_string(),
                    }),
                },
            )
            .await?;

        tx.commit().await?;

        Ok(TranslatableOfTheDay {
            date: row.scheduled_date,
            translatable_id: row.translatable_id,
            assigned_by: row.assigned_by,
            assigned_at: row.assigned_at,
            is_auto: row.is_auto,
        })
    }

    /// Remove a future schedule entry. Today's entry and past entries cannot
    /// be unscheduled (they're already "live" history). The translatable
    /// drops back into the unscheduled queue at its sort_key position.
    pub async fn unschedule(&self, admin: &User, translatable_id: Uuid) -> AppResult<()> {
        if !is_staff(admin) {
            return Err(forbidden("only admins or moderators can unschedule TotD entries"));
        }

        let row = sqlx::query!(
            r#"
                UPDATE totd_queue q
                SET scheduled_date = NULL,
                    assigned_by = NULL,
                    assigned_at = NULL,
                    is_auto = false
                FROM (
                    SELECT scheduled_date AS old_date
                    FROM totd_queue
                    WHERE translatable_id = $1
                ) src
                WHERE q.translatable_id = $1
                  AND src.old_date IS NOT NULL
                  AND src.old_date > current_date
                RETURNING src.old_date AS "old_date!"
            "#,
            translatable_id,
        )
        .fetch_optional(&self.state.pool)
        .await?;

        let Some(row) = row else {
            return Err(bad_request(
                "translatable isn't scheduled for a future date — today's and past entries can't be unscheduled",
            ));
        };

        let audit = crate::model::audit_log::AuditLogRepository::new(self.state.clone());
        audit
            .create_internal(crate::model::audit_log::CreateAuditLog {
                user_id: Some(admin.id),
                action: crate::model::audit_log::AuditActionType::Deleted,
                resource_type: crate::model::audit_log::AuditableResource::Translatable,
                resource_id: translatable_id,
                details: serde_json::json!({
                    "kind": "totd_unschedule",
                    "date": row.old_date.to_string(),
                }),
            })
            .await?;

        Ok(())
    }

    /// Look up the scheduled_date for a translatable, if any. Returns None
    /// when the translatable isn't in the queue or has no scheduled date.
    pub async fn scheduled_date_for(
        &self,
        translatable_id: Uuid,
    ) -> AppResult<Option<NaiveDate>> {
        Ok(sqlx::query_scalar!(
            r#"
                SELECT scheduled_date
                FROM totd_queue
                WHERE translatable_id = $1
            "#,
            translatable_id,
        )
        .fetch_optional(&self.state.pool)
        .await?
        .flatten())
    }

    /// Drafts not yet scheduled — the curation queue, ordered by the stable
    /// sort_key shuffle. Staff-only at the controller layer.
    pub async fn queue(
        &self,
        pagination: PaginatedRequest,
    ) -> AppResult<PaginatedResponse<Translatable>> {
        let items_future = sqlx::query_as!(
            Translatable,
            r#"
                SELECT
                    t.id, t.slug, t.title, t.english, t.source_name, t.source_url,
                    t.source_content, t.source_language, t.created_at, t.updated_at,
                    t.created_by, t.updated_by, t.like_count, t.description, t.published_at,
                    (
                        SELECT COUNT(*)
                        FROM translation
                        WHERE translation.translatable = t.id
                    ) AS "translations_count!"
                FROM totd_queue q
                JOIN translatable t ON t.id = q.translatable_id
                WHERE q.scheduled_date IS NULL
                ORDER BY q.sort_key
                LIMIT $1 OFFSET $2
            "#,
            i64::from(pagination.limit),
            i64::from(pagination.offset),
        )
        .fetch_all(&self.state.pool);

        let count_future = sqlx::query_scalar!(
            r#"
                SELECT COUNT(*)
                FROM totd_queue
                WHERE scheduled_date IS NULL
            "#
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

    /// Scheduled entries with date < today, newest first, paginated.
    pub async fn archive(
        &self,
        pagination: PaginatedRequest,
        requestor: Option<&User>,
    ) -> AppResult<PaginatedResponse<TotdEntry>> {
        let today = Utc::now().date_naive();
        let rows = sqlx::query!(
            r#"
                SELECT scheduled_date AS "scheduled_date!", translatable_id
                FROM totd_queue
                WHERE scheduled_date < $1
                ORDER BY scheduled_date DESC
                LIMIT $2 OFFSET $3
            "#,
            today,
            i64::from(pagination.limit),
            i64::from(pagination.offset),
        )
        .fetch_all(&self.state.pool)
        .await?;

        let total = sqlx::query_scalar!(
            r#"
                SELECT COUNT(*)
                FROM totd_queue
                WHERE scheduled_date < $1
            "#,
            today
        )
        .fetch_one(&self.state.pool)
        .await?
        .unwrap_or(0);

        let mut items = Vec::with_capacity(rows.len());
        for row in rows {
            items.push(
                self.materialize_entry(row.scheduled_date, row.translatable_id, requestor)
                    .await?,
            );
        }

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

    /// Merge-walk peek of the upcoming totd schedule, paginated by row index.
    /// Each row is one date forward from today. Scheduled rows are emitted
    /// at their date; otherwise unscheduled drafts fill the gap in sort_key
    /// order; otherwise (queue empty but scheduled rows exist further out)
    /// `Empty` rows fill until the next scheduled date.
    pub async fn peek_upcoming(
        &self,
        pagination: PaginatedRequest,
        requestor: Option<&User>,
    ) -> AppResult<PaginatedResponse<PeekRow>> {
        let today = Utc::now().date_naive();
        let offset = pagination.offset as i64;
        let limit = pagination.limit as i64;
        let max_rows = (offset + limit) as usize;

        // all scheduled rows >= today. usually a small set; we walk them in
        // lockstep with date advancement.
        let scheduled_rows = sqlx::query!(
            r#"
                SELECT scheduled_date AS "scheduled_date!", translatable_id, is_auto
                FROM totd_queue
                WHERE scheduled_date >= $1
                ORDER BY scheduled_date
            "#,
            today,
        )
        .fetch_all(&self.state.pool)
        .await?;

        // enough unscheduled rows to fill the window (limit+offset+1 so we
        // can detect has_more by remainder).
        let unsched_fetch = (offset + limit + 1).max(1);
        let unscheduled_rows = sqlx::query!(
            r#"
                SELECT translatable_id
                FROM totd_queue
                WHERE scheduled_date IS NULL
                ORDER BY sort_key
                LIMIT $1
            "#,
            unsched_fetch,
        )
        .fetch_all(&self.state.pool)
        .await?;

        let total_unscheduled = sqlx::query_scalar!(
            r#"SELECT COUNT(*) FROM totd_queue WHERE scheduled_date IS NULL"#,
        )
        .fetch_one(&self.state.pool)
        .await?
        .unwrap_or(0);

        // pragmatic total: every scheduled row + every unscheduled row each
        // produces one peek row. empty gaps add to the actual total but are
        // not counted here — pagination ui treats this as a lower bound.
        let total = total_unscheduled + scheduled_rows.len() as i64;

        // walk
        let mut scheduled_iter = scheduled_rows.into_iter().peekable();
        let mut unscheduled_iter = unscheduled_rows.into_iter();
        let mut all_rows: Vec<(NaiveDate, Option<Uuid>, PeekKind)> = Vec::with_capacity(max_rows);
        let mut d = today;

        while all_rows.len() < max_rows {
            let scheduled_next_date = scheduled_iter.peek().map(|s| s.scheduled_date);

            if Some(d) == scheduled_next_date {
                let s = scheduled_iter.next().unwrap();
                let kind = if d == today && s.is_auto {
                    PeekKind::AutoLockedIn
                } else {
                    PeekKind::Scheduled
                };
                all_rows.push((d, Some(s.translatable_id), kind));
            } else if let Some(s) = unscheduled_iter.next() {
                let kind = if d == today {
                    PeekKind::AutoLockedIn
                } else {
                    PeekKind::AutoPredicted
                };
                all_rows.push((d, Some(s.translatable_id), kind));
            } else if scheduled_next_date.is_some() {
                all_rows.push((d, None, PeekKind::Empty));
            } else {
                break;
            }
            d = match d.succ_opt() {
                Some(next) => next,
                None => break,
            };
        }

        let has_more = all_rows.len() == max_rows
            && (scheduled_iter.peek().is_some() || unscheduled_iter.next().is_some());

        let window: Vec<_> = all_rows
            .into_iter()
            .skip(offset as usize)
            .take(limit as usize)
            .collect();

        let mut items = Vec::with_capacity(window.len());
        for (date, maybe_id, kind) in window {
            let entry = if let Some(id) = maybe_id {
                Some(self.materialize_peek_entry(date, id, kind, requestor).await?)
            } else {
                None
            };
            items.push(PeekRow { date, entry, kind });
        }

        Ok(PaginatedResponse {
            items,
            total,
            offset: pagination.offset,
            limit: pagination.limit,
            has_more,
        })
    }

    // ------- internals -------

    /// Race-safe lazy auto-pick. Uses a transaction-level advisory lock so
    /// only one first-viewer claims a draft; concurrent viewers wait, then
    /// observe the committed pick on the fast path.
    async fn fetch_or_auto_pick_today(&self) -> AppResult<Option<Uuid>> {
        let mut tx = self.state.pool.begin().await?;

        // serialize concurrent auto-picks
        sqlx::query!("SELECT pg_advisory_xact_lock(hashtext('totd_auto_pick')::bigint)")
            .execute(&mut *tx)
            .await?;

        // fast path: today already has a row
        let existing = sqlx::query_scalar!(
            r#"SELECT translatable_id FROM totd_queue WHERE scheduled_date = CURRENT_DATE"#
        )
        .fetch_optional(&mut *tx)
        .await?;

        if let Some(id) = existing {
            // pre-scheduled rows skipped the auto-pick publish; if the
            // translatable is still a draft, publish it now so the
            // featured-on-its-day translatable isn't shown as a draft.
            publish_translatable_for_totd_tx(&mut tx, id).await?;
            tx.commit().await?;
            return Ok(Some(id));
        }

        // claim the next unscheduled draft by sort_key
        let claimed = sqlx::query_scalar!(
            r#"
                UPDATE totd_queue
                SET scheduled_date = CURRENT_DATE,
                    is_auto = true,
                    assigned_at = now()
                WHERE id = (
                    SELECT id FROM totd_queue
                    WHERE scheduled_date IS NULL
                    ORDER BY sort_key
                    LIMIT 1
                )
                RETURNING translatable_id
            "#
        )
        .fetch_optional(&mut *tx)
        .await?;

        if let Some(id) = claimed {
            publish_translatable_for_totd_tx(&mut tx, id).await?;
        }

        tx.commit().await?;
        Ok(claimed)
    }

    async fn materialize_entry(
        &self,
        date: NaiveDate,
        translatable_id: Uuid,
        requestor: Option<&User>,
    ) -> AppResult<TotdEntry> {
        let translatables = TranslatableRepository::new(self.state.clone());
        let translatable = translatables.find_by_id(translatable_id).await?;
        let translatable = translatables.materialize(translatable, requestor).await?;

        let row = sqlx::query!(
            r#"
                SELECT assigned_by, assigned_at, is_auto
                FROM totd_queue
                WHERE scheduled_date = $1
            "#,
            date
        )
        .fetch_one(&self.state.pool)
        .await?;

        let assigned_by = if let Some(id) = row.assigned_by {
            let users = UserRepository::new(self.state.clone());
            users.find_by_id(id).await.ok()
        } else {
            None
        };

        Ok(TotdEntry {
            date,
            translatable,
            assigned_by,
            assigned_at: row.assigned_at,
            is_auto: row.is_auto,
        })
    }

    /// Materializer for peek rows. Predicted rows don't have a queue row
    /// matching their date yet, so we synthesize defaults instead of
    /// querying.
    async fn materialize_peek_entry(
        &self,
        date: NaiveDate,
        translatable_id: Uuid,
        kind: PeekKind,
        requestor: Option<&User>,
    ) -> AppResult<TotdEntry> {
        match kind {
            PeekKind::Scheduled | PeekKind::AutoLockedIn => {
                self.materialize_entry(date, translatable_id, requestor).await
            }
            PeekKind::AutoPredicted => {
                let translatables = TranslatableRepository::new(self.state.clone());
                let translatable = translatables.find_by_id(translatable_id).await?;
                let translatable = translatables.materialize(translatable, requestor).await?;
                Ok(TotdEntry {
                    date,
                    translatable,
                    assigned_by: None,
                    assigned_at: None,
                    is_auto: true,
                })
            }
            PeekKind::Empty => unreachable!("materialize_peek_entry called with Empty kind"),
        }
    }
}

crate::util::repo_from_parts!(TranslatableOfTheDayRepository);
