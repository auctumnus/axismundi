use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use crate::{
    controller::html::LanguagesWithContributors,
    err::{AppResult, forbidden, not_found},
    model::{
        language_families::FamilyWithContributors, news::NewsWithCreator,
        translatable::TranslatableWithMeta, translations::TranslationWithLanguageAndContributor,
        users::User, words::WordWithMeta,
    },
    pagination::{PaginatedRequest, PaginatedResponse},
    util::AppState,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "activity_type", rename_all = "snake_case")]
pub enum ActivityType {
    CreateWord,
    UpdateWord,
    CreateTranslatable,
    UpdateTranslatable,
    CreateTranslation,
    UpdateTranslation,
    CreateLanguage,
    UpdateLanguage,
    CreateLanguageFamily,
    UpdateLanguageFamily,
    UserJoined,
    PublishNews,
}

impl ActivityType {
    pub fn verb(&self) -> &str {
        match self {
            ActivityType::CreateLanguage
            | ActivityType::CreateWord
            | ActivityType::CreateTranslatable
            | ActivityType::CreateTranslation
            | ActivityType::CreateLanguageFamily => "added",
            ActivityType::UpdateLanguage
            | ActivityType::UpdateWord
            | ActivityType::UpdateTranslatable
            | ActivityType::UpdateTranslation
            | ActivityType::UpdateLanguageFamily => "updated",
            ActivityType::UserJoined => "joined",
            ActivityType::PublishNews => "published",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityEntity {
    Word(Box<WordWithMeta>, String),
    Language(LanguagesWithContributors),
    User(User),
    Translatable(TranslatableWithMeta),
    Translation(Box<TranslationWithLanguageAndContributor>, String),
    LanguageFamily(FamilyWithContributors),
    News(NewsWithCreator),
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct UserActivity {
    pub id: Uuid,
    #[serde(skip_serializing)]
    pub user_id: Uuid,
    pub activity: ActivityType,
    pub entity_id: Uuid,
    pub entity_type: String,
    #[allow(dead_code)]
    #[serde(skip_serializing)]
    pub related_entity_id: Option<Uuid>,
    #[allow(dead_code)]
    #[serde(skip_serializing)]
    pub related_entity_type: Option<String>,
    pub timestamp: DateTime<Utc>,
    pub count: i32,
    pub user: User,

    // materialized
    pub entity: ActivityEntity,
    pub related_entity: Option<ActivityEntity>,
}

impl UserActivity {
    /// Count-aware verb phrase shown in the feed. For coalesced rows, the
    /// count is woven into the phrase ("added 23 words"); otherwise this
    /// falls back to the bare verb followed by an entity card in the template.
    pub fn phrase(&self) -> String {
        match (self.activity, self.count) {
            (ActivityType::UserJoined, _) => "joined".to_string(),
            (ActivityType::CreateWord, n) if n > 1 => format!("added {n} words"),
            (ActivityType::CreateTranslation, n) if n > 1 => {
                format!("added {n} translations")
            }
            _ => self.activity.verb().to_string(),
        }
    }
}

pub struct UserActivityRepository {
    state: AppState,
}

impl UserActivityRepository {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    pub async fn find_by_id(&self, id: Uuid, requestor: Option<&User>) -> AppResult<UserActivity> {
        if let Some(record) = sqlx::query!(
            r#"
                SELECT
                    ua.id,
                    ua.user_id,
                    ua.activity as "activity: ActivityType",
                    ua.entity_id,
                    ua.entity_type,
                    ua.related_entity_id,
                    ua.related_entity_type,
                    ua.timestamp,
                    ua.count,
                    u.id as "u_id!",
                    u.username as "u_username!",
                    u.email as "u_email!",
                    u.password_hash as "u_password_hash!",
                    u.display_name as "u_display_name",
                    u.description as "u_description",
                    u.pronouns as "u_pronouns",
                    u.gender as "u_gender",
                    u.profile_picture_object_id as "u_profile_picture_object_id",
                    u.tags as "u_tags!",
                    u.created_at as "u_created_at!",
                    u.updated_at as "u_updated_at!",
                    u.verified_at as "u_verified_at",
                    COALESCE(b.slug, '')::text as "u_bookmark!"
                FROM user_activities ua
                JOIN users u ON ua.user_id = u.id
                LEFT JOIN bookmarks b ON b.item = u.id AND b.resource = 'user'
                WHERE ua.id = $1
            "#,
            id
        )
        .fetch_optional(&self.state.pool)
        .await?
        {
            let entity = self
                .resolve_entity(record.entity_id, record.entity_type.as_str(), requestor)
                .await?
                .ok_or_else(|| not_found(format!("user activity with id '{id}'")))?;
            Ok(UserActivity {
                id: record.id,
                user_id: record.user_id,
                activity: record.activity,
                entity_id: record.entity_id,
                related_entity_id: record.related_entity_id,
                timestamp: record.timestamp,
                count: record.count,
                user: User {
                    id: record.u_id,
                    username: record.u_username,
                    email: record.u_email,
                    password_hash: record.u_password_hash,
                    display_name: record.u_display_name,
                    description: record.u_description,
                    pronouns: record.u_pronouns,
                    gender: record.u_gender,
                    profile_picture_object_id: record.u_profile_picture_object_id,
                    banner_object_id: String::new(),
                    tags: record.u_tags,
                    created_at: record.u_created_at,
                    updated_at: record.u_updated_at,
                    verified_at: record.u_verified_at,
                    bookmark: record.u_bookmark,
                },
                entity,
                related_entity: if let Some(related_id) = record.related_entity_id {
                    self.resolve_related(
                        related_id,
                        record.related_entity_type.as_deref().unwrap_or(""),
                    )
                    .await?
                } else {
                    None
                },
                entity_type: record.entity_type,
                related_entity_type: record.related_entity_type,
            })
        } else {
            Err(not_found(format!("user activity with id '{id}'")))
        }
    }

    /// Create a new activity. This should be called when a user does an "exciting" activity
    /// on a PUBLIC language (private language activities are not logged per activities-plan.md)
    pub async fn create(
        &self,
        user_id: Uuid,
        activity: ActivityType,
        entity_id: Uuid,
        entity_type: &str,
        related_entity_id: Option<Uuid>,
        related_entity_type: Option<&str>,
    ) -> AppResult<Uuid> {
        let record = sqlx::query!(
            r#"
                INSERT INTO user_activities (user_id, activity, entity_id, entity_type, related_entity_id, related_entity_type)
                VALUES ($1, $2, $3, $4, $5, $6)
                RETURNING id
            "#,
            user_id,
            activity as ActivityType,
            entity_id,
            entity_type,
            related_entity_id,
            related_entity_type
        )
        .fetch_one(&self.state.pool)
        .await?;

        Ok(record.id)
    }

    /// Create a new activity within an existing transaction. Use this when the entity being
    /// recorded hasn't been committed yet, so that the activity and entity are atomic.
    pub async fn create_with_tx(
        &self,
        user_id: Uuid,
        activity: ActivityType,
        entity_id: Uuid,
        entity_type: &str,
        related_entity_id: Option<Uuid>,
        related_entity_type: Option<&str>,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ) -> AppResult<Uuid> {
        let record = sqlx::query!(
            r#"
                INSERT INTO user_activities (user_id, activity, entity_id, entity_type, related_entity_id, related_entity_type)
                VALUES ($1, $2, $3, $4, $5, $6)
                RETURNING id
            "#,
            user_id,
            activity as ActivityType,
            entity_id,
            entity_type,
            related_entity_id,
            related_entity_type
        )
        .fetch_one(&mut **tx)
        .await?;

        Ok(record.id)
    }

    /// Coalesce a repeated create activity (CreateWord / CreateTranslation) into the user's
    /// most recent matching row if one is still "active": either within the last 10 minutes,
    /// or within the last 24 hours AND the user hasn't done anything else since. Otherwise
    /// inserts a new row. Runs inside the caller's transaction so the activity write is
    /// atomic with the entity insert.
    pub async fn create_or_coalesce_with_tx(
        &self,
        user_id: Uuid,
        activity: ActivityType,
        entity_id: Uuid,
        entity_type: &str,
        related_entity_id: Option<Uuid>,
        related_entity_type: Option<&str>,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ) -> AppResult<Uuid> {
        // Find the latest matching activity (and lock it) plus the user's overall latest.
        let candidate = sqlx::query!(
            r#"
                WITH latest_matching AS (
                    SELECT id, timestamp
                    FROM user_activities
                    WHERE user_id = $1
                      AND activity = $2
                      AND related_entity_id = $3
                    ORDER BY timestamp DESC
                    LIMIT 1
                    FOR UPDATE
                ),
                latest_any AS (
                    SELECT id
                    FROM user_activities
                    WHERE user_id = $1
                    ORDER BY timestamp DESC
                    LIMIT 1
                )
                SELECT
                    lm.id AS "match_id?",
                    lm.timestamp AS "match_ts?",
                    la.id AS "any_id?"
                FROM latest_matching lm
                FULL OUTER JOIN latest_any la ON TRUE
            "#,
            user_id,
            activity as ActivityType,
            related_entity_id,
        )
        .fetch_optional(&mut **tx)
        .await?;

        let coalesce_target = candidate.and_then(|c| {
            let match_id = c.match_id?;
            let match_ts = c.match_ts?;
            let age = Utc::now() - match_ts;
            let in_burst = age < Duration::minutes(10);
            let uninterrupted = c.any_id == Some(match_id) && age < Duration::hours(24);
            (in_burst || uninterrupted).then_some(match_id)
        });

        if let Some(match_id) = coalesce_target {
            sqlx::query!(
                r#"
                    UPDATE user_activities
                    SET count = count + 1,
                        entity_id = $1,
                        timestamp = NOW()
                    WHERE id = $2
                "#,
                entity_id,
                match_id,
            )
            .execute(&mut **tx)
            .await?;
            Ok(match_id)
        } else {
            let record = sqlx::query!(
                r#"
                    INSERT INTO user_activities (user_id, activity, entity_id, entity_type, related_entity_id, related_entity_type)
                    VALUES ($1, $2, $3, $4, $5, $6)
                    RETURNING id
                "#,
                user_id,
                activity as ActivityType,
                entity_id,
                entity_type,
                related_entity_id,
                related_entity_type
            )
            .fetch_one(&mut **tx)
            .await?;
            Ok(record.id)
        }
    }

    /// Same as `create_or_coalesce_with_tx` but opens its own short transaction for callers
    /// that aren't already inside one (e.g. translation creation).
    pub async fn create_or_coalesce(
        &self,
        user_id: Uuid,
        activity: ActivityType,
        entity_id: Uuid,
        entity_type: &str,
        related_entity_id: Option<Uuid>,
        related_entity_type: Option<&str>,
    ) -> AppResult<Uuid> {
        let mut tx = self.state.pool.begin().await?;
        let id = self
            .create_or_coalesce_with_tx(
                user_id,
                activity,
                entity_id,
                entity_type,
                related_entity_id,
                related_entity_type,
                &mut tx,
            )
            .await?;
        tx.commit().await?;
        Ok(id)
    }

    /// Delete an activity by its ID. Only the related user can delete their own activities.
    pub async fn delete(&self, requestor: &User, id: Uuid) -> AppResult<bool> {
        let activity = self.find_by_id(id, Some(requestor)).await?;

        if activity.user_id != requestor.id {
            return Err(forbidden("you can only delete your own activities"));
        }

        let result = sqlx::query!("DELETE FROM user_activities WHERE id = $1", id)
            .execute(&self.state.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    /// List activities for a user. Returns the last 20 activities unless the requestor
    /// has edit permissions on the language (if `language_id` is provided).
    pub async fn list_by_user(
        &self,
        requestor: Option<&User>,
        user_id: Uuid,
        language_id: Option<Uuid>,
        pagination: PaginatedRequest,
    ) -> AppResult<PaginatedResponse<UserActivity>> {
        // Check if we need to limit to 20 activities
        let has_edit_permission = if let Some(lang_id) = language_id {
            if let Some(req) = requestor {
                let permissions =
                    crate::model::language_permissions::LanguagePermissionRepository::new(
                        self.state.clone(),
                    );
                if let Some(perm) = permissions
                    .find_by_user_and_language(req.id, lang_id)
                    .await?
                {
                    perm.permission != crate::model::language_invites::PermissionLevel::Viewer
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            false
        };

        // Limit to 20 if no edit permissions
        let effective_limit = if has_edit_permission {
            pagination.limit
        } else {
            std::cmp::min(pagination.limit, 20)
        };

        let effective_offset = if has_edit_permission {
            pagination.offset
        } else {
            std::cmp::min(pagination.offset, 20)
        };

        let records_future = sqlx::query!(
            r#"
                SELECT
                    ua.id,
                    ua.user_id,
                    ua.activity as "activity: ActivityType",
                    ua.entity_id,
                    ua.entity_type,
                    ua.related_entity_id,
                    ua.related_entity_type,
                    ua.timestamp,
                    ua.count,
                    u.id as "u_id!",
                    u.username as "u_username!",
                    u.email as "u_email!",
                    u.password_hash as "u_password_hash!",
                    u.display_name as "u_display_name",
                    u.description as "u_description",
                    u.pronouns as "u_pronouns",
                    u.gender as "u_gender",
                    u.profile_picture_object_id as "u_profile_picture_object_id",
                    u.tags as "u_tags!",
                    u.created_at as "u_created_at!",
                    u.updated_at as "u_updated_at!",
                    u.verified_at as "u_verified_at",
                    COALESCE(b.slug, '')::text as "u_bookmark!"
                FROM user_activities ua
                JOIN users u ON ua.user_id = u.id
                LEFT JOIN bookmarks b ON b.item = u.id AND b.resource = 'user'
                WHERE ua.user_id = $1
                AND ($2::UUID IS NULL OR ua.related_entity_id = $2)
                ORDER BY ua.timestamp DESC
                LIMIT $3
                OFFSET $4
            "#,
            user_id,
            language_id,
            i64::from(effective_limit),
            i64::from(effective_offset)
        )
        .fetch_all(&self.state.pool);

        let count_future = sqlx::query_scalar!(
            r#"
                SELECT COUNT(*)
                FROM user_activities
                WHERE user_id = $1
                AND ($2::UUID IS NULL OR related_entity_id = $2)
            "#,
            user_id,
            language_id
        )
        .fetch_one(&self.state.pool);

        let (records, total_count) = tokio::try_join!(records_future, count_future)?;
        let mut items = Vec::new();

        for record in records {
            let Some(entity) = self
                .resolve_entity(record.entity_id, record.entity_type.as_str(), requestor)
                .await?
            else {
                continue;
            };
            let related_entity = if let Some(related_id) = record.related_entity_id {
                self.resolve_related(
                    related_id,
                    record.related_entity_type.as_deref().unwrap_or(""),
                )
                .await?
            } else {
                None
            };
            items.push(UserActivity {
                id: record.id,
                user_id: record.user_id,
                activity: record.activity,
                entity_id: record.entity_id,
                related_entity_id: record.related_entity_id,
                timestamp: record.timestamp,
                count: record.count,
                user: User {
                    id: record.u_id,
                    username: record.u_username,
                    email: record.u_email,
                    password_hash: record.u_password_hash,
                    display_name: record.u_display_name,
                    description: record.u_description,
                    pronouns: record.u_pronouns,
                    gender: record.u_gender,
                    profile_picture_object_id: record.u_profile_picture_object_id,
                    banner_object_id: String::new(),
                    tags: record.u_tags,
                    created_at: record.u_created_at,
                    updated_at: record.u_updated_at,
                    verified_at: record.u_verified_at,
                    bookmark: record.u_bookmark,
                },
                entity,
                related_entity,
                entity_type: record.entity_type,
                related_entity_type: record.related_entity_type,
            });
        }

        let total = total_count.unwrap_or(0);

        // Cap total at 20 if no edit permissions
        let effective_total = if has_edit_permission {
            total
        } else {
            std::cmp::min(total, 20)
        };

        let has_more = (i64::from(effective_offset)
            + i64::try_from(items.len()).unwrap_or(i64::MAX))
            < effective_total;

        Ok(PaginatedResponse {
            items,
            total: effective_total,
            offset: effective_offset,
            limit: effective_limit,
            has_more,
        })
    }

    /// List activities for a language. Returns the last 20 activities unless the requestor
    /// has edit permissions on the language.
    pub async fn list_by_language(
        &self,
        requestor: Option<&User>,
        language_id: Uuid,
        pagination: PaginatedRequest,
    ) -> AppResult<PaginatedResponse<UserActivity>> {
        // Check if we need to limit to 20 activities
        let has_edit_permission = if let Some(req) = requestor {
            let permissions = crate::model::language_permissions::LanguagePermissionRepository::new(
                self.state.clone(),
            );
            if let Some(perm) = permissions
                .find_by_user_and_language(req.id, language_id)
                .await?
            {
                perm.permission != crate::model::language_invites::PermissionLevel::Viewer
            } else {
                false
            }
        } else {
            false
        };

        // Limit to 20 if no edit permissions
        let effective_limit = if has_edit_permission {
            pagination.limit
        } else {
            std::cmp::min(pagination.limit, 20)
        };

        let effective_offset = if has_edit_permission {
            pagination.offset
        } else {
            std::cmp::min(pagination.offset, 20)
        };

        let records_future = sqlx::query!(
            r#"
                SELECT
                    ua.id,
                    ua.user_id,
                    ua.activity as "activity: ActivityType",
                    ua.entity_id,
                    ua.entity_type,
                    ua.related_entity_id,
                    ua.related_entity_type,
                    ua.timestamp,
                    ua.count,
                    u.id as "u_id!",
                    u.username as "u_username!",
                    u.email as "u_email!",
                    u.password_hash as "u_password_hash!",
                    u.display_name as "u_display_name",
                    u.description as "u_description",
                    u.pronouns as "u_pronouns",
                    u.gender as "u_gender",
                    u.profile_picture_object_id as "u_profile_picture_object_id",
                    u.tags as "u_tags!",
                    u.created_at as "u_created_at!",
                    u.updated_at as "u_updated_at!",
                    u.verified_at as "u_verified_at",
                    COALESCE(b.slug, '')::text as "u_bookmark!"
                FROM user_activities ua
                JOIN users u ON ua.user_id = u.id
                LEFT JOIN bookmarks b ON b.item = u.id AND b.resource = 'user'
                WHERE ua.related_entity_id = $1
                ORDER BY ua.timestamp DESC
                LIMIT $2
                OFFSET $3
            "#,
            language_id,
            i64::from(effective_limit),
            i64::from(effective_offset)
        )
        .fetch_all(&self.state.pool);

        let count_future = sqlx::query_scalar!(
            r#"
                SELECT COUNT(*)
                FROM user_activities
                WHERE related_entity_id = $1
            "#,
            language_id
        )
        .fetch_one(&self.state.pool);

        let (records, total_count) = tokio::try_join!(records_future, count_future)?;

        let mut items = Vec::new();

        for record in records {
            let Some(entity) = self
                .resolve_entity(record.entity_id, record.entity_type.as_str(), requestor)
                .await?
            else {
                continue;
            };
            let related_entity = if let Some(related_id) = record.related_entity_id {
                self.resolve_related(
                    related_id,
                    record.related_entity_type.as_deref().unwrap_or(""),
                )
                .await?
            } else {
                None
            };
            items.push(UserActivity {
                id: record.id,
                user_id: record.user_id,
                activity: record.activity,
                entity_id: record.entity_id,
                related_entity_id: record.related_entity_id,
                timestamp: record.timestamp,
                count: record.count,
                user: User {
                    id: record.u_id,
                    username: record.u_username,
                    email: record.u_email,
                    password_hash: record.u_password_hash,
                    display_name: record.u_display_name,
                    description: record.u_description,
                    pronouns: record.u_pronouns,
                    gender: record.u_gender,
                    profile_picture_object_id: record.u_profile_picture_object_id,
                    banner_object_id: String::new(),
                    tags: record.u_tags,
                    created_at: record.u_created_at,
                    updated_at: record.u_updated_at,
                    verified_at: record.u_verified_at,
                    bookmark: record.u_bookmark,
                },
                entity,
                related_entity,
                entity_type: record.entity_type,
                related_entity_type: record.related_entity_type,
            });
        }

        let total = total_count.unwrap_or(0);

        // Cap total at 20 if no edit permissions
        let effective_total = if has_edit_permission {
            total
        } else {
            std::cmp::min(total, 20)
        };

        let has_more = (i64::from(effective_offset)
            + i64::try_from(items.len()).unwrap_or(i64::MAX))
            < effective_total;

        Ok(PaginatedResponse {
            items,
            total: effective_total,
            offset: effective_offset,
            limit: effective_limit,
            has_more,
        })
    }

    /// List the last 20 site-wide activities, ordered by timestamp descending.
    pub async fn list_site_wide(&self, requestor: Option<&User>) -> AppResult<Vec<UserActivity>> {
        let records = sqlx::query!(
            r#"
                SELECT
                    ua.id,
                    ua.user_id,
                    ua.activity as "activity: ActivityType",
                    ua.entity_id,
                    ua.entity_type,
                    ua.related_entity_id,
                    ua.related_entity_type,
                    ua.timestamp,
                    ua.count,
                    u.id as "u_id!",
                    u.username as "u_username!",
                    u.email as "u_email!",
                    u.password_hash as "u_password_hash!",
                    u.display_name as "u_display_name",
                    u.description as "u_description",
                    u.pronouns as "u_pronouns",
                    u.gender as "u_gender",
                    u.profile_picture_object_id as "u_profile_picture_object_id",
                    u.tags as "u_tags!",
                    u.created_at as "u_created_at!",
                    u.updated_at as "u_updated_at!",
                    u.verified_at as "u_verified_at",
                    COALESCE(b.slug, '')::text as "u_bookmark!"
                FROM user_activities ua
                JOIN users u ON ua.user_id = u.id
                LEFT JOIN bookmarks b ON b.item = u.id AND b.resource = 'user'
                ORDER BY ua.timestamp DESC
                LIMIT 20
            "#
        )
        .fetch_all(&self.state.pool)
        .await?;
        let mut activities = Vec::new();
        let requestor_is_staff =
            requestor.is_some_and(|u| u.is_admin() || u.is_moderator());

        for record in records {
            let Some(entity) = self
                .resolve_entity(record.entity_id, record.entity_type.as_str(), requestor)
                .await?
            else {
                continue;
            };

            // hide draft translatables from non-staff in the activity feed.
            // a translatable can become a draft after its CreateTranslatable
            // activity was logged (admin-driven unpublish), so we filter
            // defensively here in addition to deferring the create-activity
            // until publish time.
            if !requestor_is_staff {
                if let ActivityEntity::Translatable(ref t) = entity {
                    if t.translatable.is_draft() {
                        continue;
                    }
                }
                if let ActivityEntity::News(ref n) = entity {
                    if n.news.is_draft() {
                        continue;
                    }
                }
            }

            let related_entity = if let Some(related_id) = record.related_entity_id {
                self.resolve_related(
                    related_id,
                    record.related_entity_type.as_deref().unwrap_or(""),
                )
                .await?
            } else {
                None
            };

            activities.push(UserActivity {
                id: record.id,
                user_id: record.user_id,
                activity: record.activity,
                entity_id: record.entity_id,
                related_entity_id: record.related_entity_id,
                timestamp: record.timestamp,
                count: record.count,
                user: User {
                    id: record.u_id,
                    username: record.u_username,
                    email: record.u_email,
                    password_hash: record.u_password_hash,
                    display_name: record.u_display_name,
                    description: record.u_description,
                    pronouns: record.u_pronouns,
                    gender: record.u_gender,
                    profile_picture_object_id: record.u_profile_picture_object_id,
                    banner_object_id: String::new(),
                    tags: record.u_tags,
                    created_at: record.u_created_at,
                    updated_at: record.u_updated_at,
                    verified_at: record.u_verified_at,
                    bookmark: record.u_bookmark,
                },
                entity,
                related_entity,
                entity_type: record.entity_type,
                related_entity_type: record.related_entity_type,
            });
        }

        Ok(activities)
    }

    /// Materialize the activity's primary entity. Returns `Ok(None)` when the
    /// entity (or a required relation like a word's language) has been deleted
    /// since the activity was logged — callers should skip such activities in
    /// list views rather than failing the whole feed.
    pub async fn resolve_entity(
        &self,
        entity_id: Uuid,
        kind: &str,
        requestor: Option<&User>,
    ) -> AppResult<Option<ActivityEntity>> {
        match kind {
            "word" => {
                let words_repo = crate::model::words::WordRepository::new(self.state.clone());
                let Ok(word) = words_repo.find_by_id(entity_id).await else {
                    return Ok(None);
                };
                let Ok(lang) = crate::model::languages::LanguageRepository::new(self.state.clone())
                    .find_by_id(word.language)
                    .await
                else {
                    return Ok(None);
                };
                let word = words_repo.materialize(word, requestor).await?;
                Ok(Some(ActivityEntity::Word(Box::new(word), lang.code)))
            }
            "language" => {
                let languages_repo =
                    crate::model::languages::LanguageRepository::new(self.state.clone());
                let Ok(language) = languages_repo.find_by_id(entity_id).await else {
                    return Ok(None);
                };
                let language = languages_repo.materialize(language, requestor).await?;
                Ok(Some(ActivityEntity::Language(language)))
            }
            "user" => {
                let users_repo = crate::model::users::UserRepository::new(self.state.clone());
                let Ok(user) = users_repo.find_by_id(entity_id).await else {
                    return Ok(None);
                };
                Ok(Some(ActivityEntity::User(user)))
            }
            "translatable" => {
                let translatables_repo =
                    crate::model::translatable::TranslatableRepository::new(self.state.clone());
                let Ok(translatable) = translatables_repo.find_by_id(entity_id).await else {
                    return Ok(None);
                };
                let translatable = translatables_repo
                    .materialize(translatable, requestor)
                    .await?;
                Ok(Some(ActivityEntity::Translatable(translatable)))
            }
            "translation" => {
                let translations_repo =
                    crate::model::translations::TranslationRepository::new(self.state.clone());
                let Ok(translation) = translations_repo.find_by_id(entity_id).await else {
                    return Ok(None);
                };
                let Ok(lang) = crate::model::languages::LanguageRepository::new(self.state.clone())
                    .find_by_id(translation.language)
                    .await
                else {
                    return Ok(None);
                };
                let translation = translations_repo
                    .materialize(translation, requestor)
                    .await?;
                Ok(Some(ActivityEntity::Translation(
                    Box::new(translation),
                    lang.code,
                )))
            }
            "news" => {
                let news_repo = crate::model::news::NewsRepository::new(self.state.clone());
                let Ok(article) = news_repo.find_by_id(entity_id).await else {
                    return Ok(None);
                };
                let materialized = news_repo.materialize(article).await?;
                Ok(Some(ActivityEntity::News(materialized)))
            }
            "language_family" => {
                let families_repo = crate::model::language_families::LanguageFamilyRepository::new(
                    self.state.clone(),
                );
                let Ok(family) = families_repo.find_by_id(entity_id).await else {
                    return Ok(None);
                };
                let family = families_repo.materialize(family, requestor).await?;
                Ok(Some(ActivityEntity::LanguageFamily(family)))
            }
            _ => Ok(None),
        }
    }

    pub async fn resolve_related(
        &self,
        related_id: Uuid,
        kind: &str,
    ) -> AppResult<Option<ActivityEntity>> {
        match kind {
            "language" => {
                let languages_repo =
                    crate::model::languages::LanguageRepository::new(self.state.clone());
                if let Ok(language) = languages_repo.find_by_id(related_id).await {
                    let language = languages_repo.materialize(language, None).await?;
                    Ok(Some(ActivityEntity::Language(language)))
                } else {
                    Ok(None)
                }
            }
            _ => Ok(None),
        }
    }
}

crate::util::repo_from_parts!(UserActivityRepository);
