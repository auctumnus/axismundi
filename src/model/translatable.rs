use askama::Template;
use chrono::{DateTime, Utc};
use nanoid::nanoid;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use validator::Validate;

use crate::{
    err::{AppResult, bad_request, not_found},
    model::users::User,
    pagination::{PaginatedRequest, PaginatedResponse},
    util::{AppState, ensure_verified, sanitize_external_url},
};

fn validate_external_url(url: &str) -> Result<(), validator::ValidationError> {
    sanitize_external_url(url)?;
    Ok(())
}

fn sanitize_source_url(url: Option<String>) -> AppResult<Option<String>> {
    url.map(|u| sanitize_external_url(&u).map_err(|e| bad_request(e.to_string())))
        .transpose()
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
    #[serde(skip_serializing)]
    pub created_by: Uuid,
    #[serde(skip_serializing)]
    pub updated_by: Uuid,
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

        let result = sqlx::query_as!(
            Translatable,
            r#"
                INSERT INTO translatable (slug, title, english, source_name, source_url, source_content, source_language, description, created_by, updated_by)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $9)
                RETURNING id, slug, title, english, source_name, source_url, source_content, source_language, created_at, updated_at, created_by, updated_by, like_count, description, 0 AS "translations_count!"
            "#,
            slug,
            translatable.title,
            translatable.english,
            translatable.source_name.unwrap_or_default(),
            source_url.unwrap_or_default(),
            translatable.source_content.unwrap_or_default(),
            translatable.source_language.unwrap_or_default(),
            translatable.description.unwrap_or_default(),
            requestor.id
        )
        .fetch_one(&self.state.pool)
        .await?;

        // Create activity (translatables are always public)
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
                RETURNING id, slug, title, english, source_name, source_url, source_content, source_language, created_at, updated_at, created_by, updated_by, like_count, description,
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

        // Create activity (translatables are always public)
        let activity_repo =
            crate::model::user_activities::UserActivityRepository::new(self.state.clone());
        let _activity = activity_repo
            .create(
                requestor.id,
                crate::model::user_activities::ActivityType::UpdateTranslatable,
                updated_translatable.id,
                "translatable",
                None,
                None,
            )
            .await?;

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
    ) -> AppResult<PaginatedResponse<Translatable>> {
        let owner = if let Some(ref username) = search.created_by {
            let user_repo = crate::model::users::UserRepository::new(self.state.clone());
            let user = user_repo.find_by_username(username).await?;
            Some(user.id)
        } else {
            None
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
                ) DESC, id
                LIMIT $5
                OFFSET $6
            "#,
            search.created_before,
            search.created_after,
            search.source_language,
            search.q,
            i64::from(pagination.limit),
            i64::from(pagination.offset),
            owner
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
            "#,
            search.created_before,
            search.created_after,
            search.source_language
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
    pub source_language: Option<String>,
    pub created_by: Option<String>,
    pub created_before: Option<DateTime<Utc>>,
    pub created_after: Option<DateTime<Utc>>,
}

crate::util::text_query!(TranslatableSearch);

crate::util::repo_from_parts!(TranslatableRepository);
