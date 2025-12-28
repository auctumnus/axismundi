use crate::AppState;
use crate::err::{AppResult, forbidden, not_found};
use crate::model::user_tags::{CreateUserTag, UserTagRepository};
use crate::model::users::{User, UserRepository};
use crate::pagination::{PaginatedRequest, PaginatedResponse};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use validator::Validate;

// Main data struct - matches database table
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct UserBan {
    pub id: Uuid,
    pub user_id: Uuid,
    pub reason: String,
    pub banned_at: DateTime<Utc>,
    pub banned_by: Uuid,
}

// Create request - used for POST
#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CreateUserBan {
    pub user_id: Uuid,
    #[validate(length(min = 1, max = 1000))]
    pub reason: String,
}

// Search request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserBanSearch {
    pub text_query: Option<String>,
    pub banned_by: Option<Uuid>,
}

// Repository - handles all database operations
pub struct UserBanRepository {
    state: AppState,
}

impl UserBanRepository {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    pub async fn create(&self, requestor: &User, req: CreateUserBan) -> AppResult<UserBan> {
        req.validate()?;

        let user_tags = UserTagRepository::new(self.state.clone());

        // Check if requestor is admin or moderator
        let is_admin = user_tags.is_admin(requestor.id).await?;
        let is_moderator = user_tags.is_moderator(requestor.id).await?;

        if !(is_admin || is_moderator) {
            return Err(forbidden("Only moderators and admins can ban users"));
        }

        // Check if target user is admin or moderator
        let target_is_admin = user_tags.is_admin(req.user_id).await?;
        let target_is_moderator = user_tags.is_moderator(req.user_id).await?;

        if target_is_admin || target_is_moderator {
            return Err(forbidden("Cannot ban moderators or admins"));
        }

        let user_ban = sqlx::query_as!(
            UserBan,
            r#"
            insert into user_bans (user_id, reason, banned_by)
            values ($1, $2, $3)
            returning id, user_id, reason, banned_at, banned_by
            "#,
            req.user_id,
            req.reason,
            requestor.id
        )
        .fetch_one(&self.state.pool)
        .await?;

        // Add the "banned" tag to the user
        user_tags
            .create(
                requestor,
                req.user_id,
                CreateUserTag {
                    tag: "banned".to_string(),
                    hidden: false,
                },
            )
            .await?;

        Ok(user_ban)
    }

    pub async fn find_by_user_id(&self, user_id: Uuid) -> AppResult<Option<UserBan>> {
        let user_ban = sqlx::query_as!(
            UserBan,
            "select id, user_id, reason, banned_at, banned_by from user_bans where user_id = $1",
            user_id
        )
        .fetch_optional(&self.state.pool)
        .await?;

        Ok(user_ban)
    }

    pub async fn is_banned(&self, user_id: Uuid) -> AppResult<bool> {
        let result = sqlx::query_scalar!(
            "select exists(select 1 from user_bans where user_id = $1)",
            user_id
        )
        .fetch_one(&self.state.pool)
        .await?;

        Ok(result.unwrap_or(false))
    }

    pub async fn ensure_not_banned(&self, user_id: Uuid) -> AppResult<()> {
        let user_ban = self.find_by_user_id(user_id).await?;
        match user_ban {
            Some(ban) => Err(forbidden(format!("you are banned ({})", ban.reason))),
            None => Ok(()),
        }
    }

    pub async fn delete(&self, requestor: &User, user_id: Uuid) -> AppResult<()> {
        let user_tags = UserTagRepository::new(self.state.clone());
        let user_repo = UserRepository::new(self.state.clone());

        // Check if requestor is admin or moderator
        let is_admin = user_tags.is_admin(requestor.id).await?;
        let is_moderator = user_tags.is_moderator(requestor.id).await?;

        if !(is_admin || is_moderator) {
            return Err(forbidden("Only moderators and admins can unban users"));
        }

        let result = sqlx::query!("delete from user_bans where user_id = $1", user_id)
            .execute(&self.state.pool)
            .await?;

        if result.rows_affected() == 0 {
            return Err(not_found("User is not banned"));
        }

        // Remove the "banned" tag from the user
        let user = user_repo.find_by_id(user_id).await?;
        user_tags
            .delete(requestor, &user, "banned".to_string())
            .await?;

        Ok(())
    }

    pub async fn search(
        &self,
        requestor: &User,
        pagination: PaginatedRequest,
        search: UserBanSearch,
    ) -> AppResult<PaginatedResponse<UserBan>> {
        let user_tags = UserTagRepository::new(self.state.clone());

        // Check if requestor is admin or moderator
        let is_admin = user_tags.is_admin(requestor.id).await?;
        let is_moderator = user_tags.is_moderator(requestor.id).await?;

        if !(is_admin || is_moderator) {
            return Err(forbidden(
                "Only moderators and admins can view banned users",
            ));
        }

        let items_future = sqlx::query_as!(
            UserBan,
            r#"
                select id, user_id, reason, banned_at, banned_by
                from user_bans
                where
                    ($1::TEXT IS NULL OR reason ILIKE '%' || $1 || '%')
                    and ($2::UUID IS NULL OR banned_by = $2)
                order by banned_at desc
                limit $3
                offset $4
            "#,
            search.text_query,
            search.banned_by,
            i64::from(pagination.limit),
            i64::from(pagination.offset)
        )
        .fetch_all(&self.state.pool);

        let count_future = sqlx::query_scalar!(
            r#"
                select count(*)
                from user_bans
                where
                    ($1::TEXT IS NULL OR reason ILIKE '%' || $1 || '%')
                    and ($2::UUID IS NULL OR banned_by = $2)
            "#,
            search.text_query,
            search.banned_by
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
}

crate::util::repo_from_parts!(UserBanRepository);
