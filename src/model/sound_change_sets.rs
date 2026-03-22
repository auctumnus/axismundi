use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use crate::{err::{AppResult, forbidden, internal_error, not_found}, lexurgy, model::{language_permissions::LanguagePermissionRepository, languages::Language, user_bans::UserBanRepository, users::User}, pagination::{PaginatedRequest, PaginatedResponse}, util::{AppState, repo_from_parts}};

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SoundChangeSet {
    pub id: Uuid,
    #[serde(skip_serializing)]
    pub language_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub changes: String,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing)]
    pub created_by: Uuid,
    #[serde(skip_serializing)]
    pub updated_by: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewSoundChangeSet {
    pub name: String,
    pub description: Option<String>,
    pub changes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateSoundChangeSet {
    pub name: Option<String>,
    pub description: Option<String>,
    pub changes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchSoundChangeSets {
    pub q: Option<String>,
    pub author: Option<String>,
}

pub struct SoundChangeSetRepository {
    state: AppState,
}

impl SoundChangeSetRepository {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    pub async fn create(&self, requestor: &User, language: &Language, new_set: NewSoundChangeSet) -> AppResult<SoundChangeSet> {
        UserBanRepository::new(self.state.clone())
            .ensure_not_banned(requestor.id)
            .await?;


        let language_permissions = LanguagePermissionRepository::new(self.state.clone());
        let can_edit_language = language_permissions.has_permission(requestor.id, language.id, crate::model::language_invites::PermissionLevel::Editor).await?;

        if !can_edit_language {
            return Err(forbidden("You do not have permission to create a sound change set for this language"));
        }

        let result = sqlx::query_as!(
            SoundChangeSet,
            "insert into sound_change_sets (language_id, name, description, changes, created_by, updated_by) values ($1, $2, $3, $4, $5, $6) returning *",
            language.id,
            new_set.name,
            new_set.description,
            new_set.changes,
            requestor.id,
            requestor.id
        )
        .fetch_one(&self.state.pool)
        .await?;

        Ok(result)
    }

    pub async fn get(&self, id: Uuid) -> AppResult<Option<SoundChangeSet>> {
        let set = sqlx::query_as!(
            SoundChangeSet,
            "select * from sound_change_sets where id = $1",
            id
        )
        .fetch_optional(&self.state.pool)
        .await?;

        Ok(set)
    }

    pub async fn update(&self, requestor: &User, set_id: &Uuid, update: UpdateSoundChangeSet) -> AppResult<SoundChangeSet> {
        UserBanRepository::new(self.state.clone())
            .ensure_not_banned(requestor.id)
            .await?;

        let Some(set) = self.get(*set_id).await? else {
            return Err(not_found("SoundChangeSet not found"));
        };

        let language_permissions = LanguagePermissionRepository::new(self.state.clone());
        let can_edit_language = language_permissions.has_permission(requestor.id, set.language_id, crate::model::language_invites::PermissionLevel::Editor).await?;

        if !can_edit_language {
            return Err(forbidden("You do not have permission to edit this sound change set"));
        }


        let updated = sqlx::query_as!(
            SoundChangeSet,
            "update sound_change_sets set name = coalesce($1, name), description = coalesce($2, description), changes = coalesce($3, changes), updated_by = $4, updated_at = now() where id = $5 returning *",
            update.name,
            update.description,
            update.changes,
            requestor.id,
            set_id
        )
        .fetch_one(&self.state.pool)
        .await?;

        Ok(updated)
    }

    pub async fn delete(&self, requestor: &User, set_id: &Uuid) -> AppResult<()> {
        UserBanRepository::new(self.state.clone())
            .ensure_not_banned(requestor.id)
            .await?;

        let Some(set) = self.get(*set_id).await? else {
            return Err(not_found("SoundChangeSet not found"));
        };

        let language_permissions = LanguagePermissionRepository::new(self.state.clone());
        let can_edit_language = language_permissions.has_permission(requestor.id, set.language_id, crate::model::language_invites::PermissionLevel::Editor).await?;

        if !can_edit_language {
            return Err(forbidden("You do not have permission to delete this sound change set"));
        }

        sqlx::query!(
            "delete from sound_change_sets where id = $1",
            set_id
        )
        .execute(&self.state.pool)
        .await?;

        Ok(())
    }

    pub async fn search(
        &self,
        language: &Language,
        pagination: PaginatedRequest,
        search: SearchSoundChangeSets,
    ) -> AppResult<PaginatedResponse<SoundChangeSet>> {
        let items_future = sqlx::query_as!(
            SoundChangeSet,
            r#"
                SELECT scs.*
                FROM sound_change_sets scs
                JOIN users u ON u.id = scs.created_by
                WHERE
                scs.language_id = $1
                AND ($5::TEXT IS NULL OR u.username = $5)
                ORDER BY (
                    CASE
                        WHEN $2::TEXT IS NOT NULL AND scs.name ILIKE '%' || $2 || '%' THEN 100.0
                        WHEN $2::TEXT IS NOT NULL AND scs.description ILIKE '%' || $2 || '%' THEN 80.0
                        ELSE 0.0
                    END +
                    CASE WHEN $2::TEXT IS NOT NULL THEN
                        similarity(scs.name, $2) * 3.0 +
                        COALESCE(similarity(scs.description, $2), 0.0) * 1.0
                    ELSE 0.0
                    END
                ) DESC, scs.created_at DESC, scs.id DESC
                LIMIT $3
                OFFSET $4
            "#,
            language.id,
            search.q,
            i64::from(pagination.limit),
            i64::from(pagination.offset),
            search.author,
        )
        .fetch_all(&self.state.pool);

        let count_future = sqlx::query_scalar!(
            r#"
                SELECT COUNT(*)
                FROM sound_change_sets scs
                JOIN users u ON u.id = scs.created_by
                WHERE scs.language_id = $1
                AND ($2::TEXT IS NULL OR u.username = $2)
            "#,
            language.id,
            search.author,
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

    pub async fn run(&self, set_id: &Uuid, input_words: Vec<String>) -> AppResult<lexurgy::Response> {
        let set = self.get(*set_id).await?;

        if let Some(set) = set {
            let response = crate::lexurgy::run_sound_changes(set.changes, input_words).await?;

            match response {
                Ok(response) => Ok(response),
                Err(error) => Err(internal_error(format!("Failed to run sound changes: {error}"))),
            }
        } else {
           Err(not_found(format!("sound change set with id {set_id}")))
        }
    }
}

repo_from_parts!(SoundChangeSetRepository);