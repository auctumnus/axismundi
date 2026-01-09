use crate::AppState;
use crate::err::{AppResult, forbidden};
use crate::model::users::User;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use validator::Validate;

// Main data struct - matches database table
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct UserTag {
    #[serde(skip_serializing)]
    pub id: Uuid,
    #[serde(skip_serializing)]
    #[allow(dead_code)]
    pub user_id: Uuid,
    pub tag: String,
    pub hidden: bool,
    pub created_at: DateTime<Utc>,
}

// Create request - used for POST
#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CreateUserTag {
    #[validate(length(min = 1, max = 100))]
    pub tag: String,
    #[serde(default)]
    pub hidden: bool,
}

// Repository - handles all database operations
pub struct UserTagRepository {
    state: AppState,
}

impl UserTagRepository {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    pub async fn create(
        &self,
        requestor: &User,
        user_id: Uuid,
        req: CreateUserTag,
    ) -> AppResult<UserTag> {
        req.validate()?;

        crate::model::user_bans::UserBanRepository::new(self.state.clone())
            .ensure_not_banned(requestor.id)
            .await?;

        let is_admin = self.is_admin(requestor.id).await?;
        let is_moderator = self.is_moderator(requestor.id).await?;

        if !(is_admin || is_moderator) {
            return Err(forbidden(
                "You do not have permission to add tags to users.",
            ));
        }

        if req.tag == "admin" {
            return Err(forbidden(
                "Admins can only be created via the database. Please contact your system administrator.",
            ));
        }

        if req.tag == "moderator" && !is_admin {
            return Err(forbidden("Only admins can create moderators."));
        }

        let user_tag = sqlx::query_as!(
            UserTag,
            r#"
            insert into user_tags (user_id, tag, hidden)
            values ($1, $2, $3)
            returning id, user_id, tag, hidden, created_at
            "#,
            user_id,
            req.tag,
            req.hidden
        )
        .fetch_one(&self.state.pool)
        .await?;

        Ok(user_tag)
    }

    async fn find(&self, user: &User, tag: &str) -> AppResult<Option<UserTag>> {
        let user_tag = sqlx::query_as!(
            UserTag,
            "select id, user_id, tag, hidden, created_at from user_tags where user_id = $1 and tag = $2",
            user.id,
            tag
        )
        .fetch_optional(&self.state.pool)
        .await?;

        Ok(user_tag)
    }

    pub async fn find_all_by_user_id(&self, user_id: Uuid) -> AppResult<Vec<UserTag>> {
        let user_tags = sqlx::query_as!(
            UserTag,
            "select id, user_id, tag, hidden, created_at from user_tags where user_id = $1 order by created_at desc",
            user_id
        )
        .fetch_all(&self.state.pool)
        .await?;

        Ok(user_tags)
    }

    pub async fn delete(&self, requestor: &User, user: &User, tag: String) -> AppResult<()> {
        crate::model::user_bans::UserBanRepository::new(self.state.clone())
            .ensure_not_banned(requestor.id)
            .await?;

        let is_admin = self.is_admin(requestor.id).await?;
        let is_moderator = self.is_moderator(requestor.id).await?;

        if !(is_admin || is_moderator) {
            return Err(forbidden(
                "You do not have permission to remove tags from users.",
            ));
        }

        let Some(tag) = self.find(user, &tag).await? else {
            return Ok(());
        };
        let id = tag.id;

        if tag.tag == "admin" {
            return Err(forbidden(
                "Admins can only be removed via the database. Please contact your system administrator.",
            ));
        }

        if tag.tag == "moderator" && !is_admin {
            return Err(forbidden("Only admins can remove moderators."));
        }

        sqlx::query!("delete from user_tags where id = $1", id)
            .execute(&self.state.pool)
            .await?;
        Ok(())
    }

    pub async fn is_admin(&self, user_id: Uuid) -> AppResult<bool> {
        let result = sqlx::query_scalar!(
            "select exists(select 1 from user_tags where user_id = $1 and tag = 'admin')",
            user_id
        )
        .fetch_one(&self.state.pool)
        .await?;

        Ok(result.unwrap_or(false))
    }

    pub async fn is_moderator(&self, user_id: Uuid) -> AppResult<bool> {
        let result = sqlx::query_scalar!(
            "select exists(select 1 from user_tags where user_id = $1 and tag = 'moderator')",
            user_id
        )
        .fetch_one(&self.state.pool)
        .await?;

        Ok(result.unwrap_or(false))
    }
}

crate::util::repo_from_parts!(UserTagRepository);
