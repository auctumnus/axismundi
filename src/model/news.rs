use chrono::{DateTime, Utc};
use nanoid::nanoid;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use validator::Validate;

use crate::{
    err::{AppResult, forbidden, not_found},
    model::users::User,
    pagination::{PaginatedRequest, PaginatedResponse},
    util::{AppState, ensure_verified},
};

fn is_staff(user: Option<&User>) -> bool {
    user.is_some_and(|u| u.is_admin() || u.is_moderator())
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct News {
    pub id: Uuid,
    pub slug: String,
    pub title: String,
    pub content: String,
    pub published_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing)]
    pub created_by: Uuid,
    #[serde(skip_serializing)]
    pub updated_by: Uuid,
}

impl News {
    pub fn is_published(&self) -> bool {
        self.published_at.is_some()
    }

    pub fn is_draft(&self) -> bool {
        self.published_at.is_none()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct NewsWithCreator {
    pub news: News,
    pub creator: User,
    pub updater: Option<User>,
}

/// Draft visibility filter. `PublishedOnly` is the default and the only value
/// the public is allowed to use; staff can request drafts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DraftFilter {
    #[default]
    PublishedOnly,
    DraftsOnly,
    All,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CreateNews {
    #[validate(length(min = 1, max = 200))]
    pub title: String,

    #[validate(length(min = 1, max = 100_000))]
    pub content: String,

    /// Staff-only: when true, the article is created as a draft
    /// (published_at = NULL). Controllers MUST force this to false for
    /// non-staff requestors (though create() also rejects non-staff outright).
    #[serde(default)]
    pub as_draft: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct UpdateNews {
    #[validate(length(min = 1, max = 200))]
    pub title: Option<String>,

    #[validate(length(min = 1, max = 100_000))]
    pub content: Option<String>,
}

pub struct NewsRepository {
    state: AppState,
}

impl NewsRepository {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    pub async fn materialize(&self, news: News) -> AppResult<NewsWithCreator> {
        let users = crate::model::users::UserRepository::new(self.state.clone());
        let creator = users.find_by_id(news.created_by).await?;
        let updater = if news.updated_by != news.created_by {
            Some(users.find_by_id(news.updated_by).await?)
        } else {
            None
        };
        Ok(NewsWithCreator {
            news,
            creator,
            updater,
        })
    }

    pub async fn find_by_id(&self, id: Uuid) -> AppResult<News> {
        let result = sqlx::query_as!(
            News,
            r#"
                SELECT
                    id,
                    slug,
                    title,
                    content,
                    published_at,
                    created_at,
                    updated_at,
                    created_by,
                    updated_by
                FROM news
                WHERE id = $1
            "#,
            id
        )
        .fetch_optional(&self.state.pool)
        .await?;

        result.ok_or_else(|| not_found(format!("news with id '{id}'")))
    }

    /// Visibility-aware lookup. Drafts return `NotFound` for non-staff.
    #[allow(dead_code)]
    pub async fn find_by_id_for(&self, id: Uuid, requestor: Option<&User>) -> AppResult<News> {
        let news = self.find_by_id(id).await?;
        if news.is_draft() && !is_staff(requestor) {
            return Err(not_found(format!("news with id '{id}'")));
        }
        Ok(news)
    }

    pub async fn find_by_slug(&self, slug: &str) -> AppResult<News> {
        let result = sqlx::query_as!(
            News,
            r#"
                SELECT
                    id,
                    slug,
                    title,
                    content,
                    published_at,
                    created_at,
                    updated_at,
                    created_by,
                    updated_by
                FROM news
                WHERE slug = $1
            "#,
            slug
        )
        .fetch_optional(&self.state.pool)
        .await?;

        result.ok_or_else(|| not_found(format!("news with slug '{slug}'")))
    }

    /// Visibility-aware lookup. Drafts return `NotFound` for non-staff.
    pub async fn find_by_slug_for(
        &self,
        slug: &str,
        requestor: Option<&User>,
    ) -> AppResult<News> {
        let news = self.find_by_slug(slug).await?;
        if news.is_draft() && !is_staff(requestor) {
            return Err(not_found(format!("news with slug '{slug}'")));
        }
        Ok(news)
    }

    pub async fn create(&self, requestor: &User, news: CreateNews) -> AppResult<News> {
        news.validate()?;
        ensure_verified(requestor)?;

        if !is_staff(Some(requestor)) {
            return Err(forbidden("only admins or moderators can create news"));
        }

        crate::model::user_bans::UserBanRepository::new(self.state.clone())
            .ensure_not_banned(requestor.id)
            .await?;

        let slug = slug::slugify(&news.title);
        let slug = format!("{slug}-{}", nanoid!(6));

        let as_draft = news.as_draft;

        let result = sqlx::query_as!(
            News,
            r#"
                INSERT INTO news (slug, title, content, created_by, updated_by, published_at)
                VALUES ($1, $2, $3, $4, $4, CASE WHEN $5::bool THEN NULL ELSE CURRENT_TIMESTAMP END)
                RETURNING id, slug, title, content, published_at, created_at, updated_at, created_by, updated_by
            "#,
            slug,
            news.title,
            news.content,
            requestor.id,
            as_draft,
        )
        .fetch_one(&self.state.pool)
        .await?;

        if !as_draft {
            let activity_repo =
                crate::model::user_activities::UserActivityRepository::new(self.state.clone());
            let _ = activity_repo
                .create(
                    requestor.id,
                    crate::model::user_activities::ActivityType::PublishNews,
                    result.id,
                    "news",
                    None,
                    None,
                )
                .await?;
        }

        Ok(result)
    }

    pub async fn update(
        &self,
        requestor: &User,
        id: Uuid,
        updates: UpdateNews,
    ) -> AppResult<News> {
        updates.validate()?;
        ensure_verified(requestor)?;

        if !is_staff(Some(requestor)) {
            return Err(forbidden("only admins or moderators can edit news"));
        }

        crate::model::user_bans::UserBanRepository::new(self.state.clone())
            .ensure_not_banned(requestor.id)
            .await?;

        // ensure it exists
        let _existing = self.find_by_id(id).await?;

        let new_slug = if let Some(ref title) = updates.title {
            let slugified = slug::slugify(title);
            Some(format!("{slugified}-{}", nanoid!(6)))
        } else {
            None
        };

        let result = sqlx::query_as!(
            News,
            r#"
                UPDATE news
                SET slug = COALESCE($2, slug),
                    title = COALESCE($3, title),
                    content = COALESCE($4, content),
                    updated_by = $5,
                    updated_at = CURRENT_TIMESTAMP
                WHERE id = $1
                RETURNING id, slug, title, content, published_at, created_at, updated_at, created_by, updated_by
            "#,
            id,
            new_slug,
            updates.title,
            updates.content,
            requestor.id,
        )
        .fetch_optional(&self.state.pool)
        .await?;

        result.ok_or_else(|| not_found(format!("news with id '{id}'")))
    }

    pub async fn delete(&self, requestor: &User, news: News) -> AppResult<bool> {
        ensure_verified(requestor)?;

        if !is_staff(Some(requestor)) {
            return Err(forbidden("only admins or moderators can delete news"));
        }

        crate::model::user_bans::UserBanRepository::new(self.state.clone())
            .ensure_not_banned(requestor.id)
            .await?;

        let result = sqlx::query!("DELETE FROM news WHERE id = $1", news.id)
            .execute(&self.state.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Staff-only: publish a draft article. Logs a `PublishNews` activity
    /// attributed to the publishing staff member. Idempotent.
    pub async fn publish(&self, requestor: &User, id: Uuid) -> AppResult<News> {
        if !is_staff(Some(requestor)) {
            return Err(forbidden("only admins or moderators can publish news"));
        }

        let existing = self.find_by_id(id).await?;
        if existing.is_published() {
            return Ok(existing);
        }

        let result = sqlx::query_as!(
            News,
            r#"
                UPDATE news
                SET published_at = CURRENT_TIMESTAMP
                WHERE id = $1
                RETURNING id, slug, title, content, published_at, created_at, updated_at, created_by, updated_by
            "#,
            id,
        )
        .fetch_one(&self.state.pool)
        .await?;

        let activity_repo =
            crate::model::user_activities::UserActivityRepository::new(self.state.clone());
        let _ = activity_repo
            .create(
                requestor.id,
                crate::model::user_activities::ActivityType::PublishNews,
                result.id,
                "news",
                None,
                None,
            )
            .await?;

        Ok(result)
    }

    /// Staff-only: revert an article to draft. Does not delete the historical
    /// activity entry; the activity resolver hides drafts from non-staff at
    /// read time.
    pub async fn unpublish(&self, requestor: &User, id: Uuid) -> AppResult<News> {
        if !is_staff(Some(requestor)) {
            return Err(forbidden("only admins or moderators can unpublish news"));
        }

        let result = sqlx::query_as!(
            News,
            r#"
                UPDATE news
                SET published_at = NULL
                WHERE id = $1
                RETURNING id, slug, title, content, published_at, created_at, updated_at, created_by, updated_by
            "#,
            id,
        )
        .fetch_optional(&self.state.pool)
        .await?;

        result.ok_or_else(|| not_found(format!("news with id '{id}'")))
    }

    pub async fn search(
        &self,
        pagination: PaginatedRequest,
        search: NewsSearch,
        requestor: Option<&User>,
    ) -> AppResult<PaginatedResponse<News>> {
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
            News,
            r#"
                SELECT
                    id,
                    slug,
                    title,
                    content,
                    published_at,
                    created_at,
                    updated_at,
                    created_by,
                    updated_by
                FROM news
                WHERE (
                    ($3::bool AND published_at IS NOT NULL)
                    OR ($4::bool AND published_at IS NULL)
                )
                AND (
                    $5::TEXT IS NULL
                    OR title ILIKE '%' || $5 || '%'
                    OR content ILIKE '%' || $5 || '%'
                )
                ORDER BY
                    COALESCE(published_at, created_at) DESC,
                    id DESC
                LIMIT $1
                OFFSET $2
            "#,
            i64::from(pagination.limit),
            i64::from(pagination.offset),
            include_published,
            include_drafts,
            search.q,
        )
        .fetch_all(&self.state.pool);

        let count_future = sqlx::query_scalar!(
            r#"
                SELECT COUNT(*)
                FROM news
                WHERE (
                    ($1::bool AND published_at IS NOT NULL)
                    OR ($2::bool AND published_at IS NULL)
                )
                AND (
                    $3::TEXT IS NULL
                    OR title ILIKE '%' || $3 || '%'
                    OR content ILIKE '%' || $3 || '%'
                )
            "#,
            include_published,
            include_drafts,
            search.q,
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

    /// Small helper for the home/landing pages: returns the most recent
    /// published articles. Limited to `limit` items.
    pub async fn list_recent(&self, limit: i64) -> AppResult<Vec<News>> {
        let items = sqlx::query_as!(
            News,
            r#"
                SELECT
                    id,
                    slug,
                    title,
                    content,
                    published_at,
                    created_at,
                    updated_at,
                    created_by,
                    updated_by
                FROM news
                WHERE published_at IS NOT NULL
                ORDER BY published_at DESC, id DESC
                LIMIT $1
            "#,
            limit,
        )
        .fetch_all(&self.state.pool)
        .await?;

        Ok(items)
    }
}

#[derive(Default, Debug, Deserialize, Serialize, Clone, askama::Template)]
#[template(path = "news/fragments/query.html")]
pub struct NewsSearch {
    pub q: Option<String>,
    #[serde(default)]
    pub draft_status: DraftFilter,
}

crate::util::text_query!(NewsSearch);

crate::util::repo_from_parts!(NewsRepository);
