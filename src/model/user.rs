use std::sync::LazyLock;

use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize, Serializer};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;
use validator::Validate;
use zxcvbn::Score;

use crate::{
    err::{bad_request, internal_error, not_found, AppResult}, pagination::{PaginatedRequest, PaginatedResponse}, util::{re, s3::S3}
};

use super::email_verification_token::EmailVerificationTokenRepository;

#[allow(clippy::ref_option)] // due to serde
fn serialize_object_key<S>(object_id: &Option<String>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match object_id {
        Some(id) => serializer.serialize_str(&S3.get_object_url(id)),
        None => serializer.serialize_none(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct User {
    #[serde(skip_serializing)]
    pub id: Uuid,
    #[serde(skip_serializing)]
    pub email: String,
    #[serde(skip_serializing)]
    pub password_hash: String,
    #[serde(skip_serializing)]
    pub verified_at: Option<DateTime<Utc>>,

    pub username: String,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub pronouns: Option<String>,
    pub gender: Option<String>,

    #[serde(
        rename(serialize = "profile_picture_url"),
        serialize_with = "serialize_object_key"
    )]
    pub profile_picture_object_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl User {
    pub const fn is_verified(&self) -> bool {
        self.verified_at.is_some()
    }
}

static USERNAME_REGEX: LazyLock<Regex> = re!("^([a-z0-9](-|_)?)+[a-z0-9]");
static GENDER_REGEX: LazyLock<Regex> = re!("^([a-fA-F0-9]{3}){1,2}$"); // hex code

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CreateUser {
    #[validate(length(min = 2, max = 30), regex(path = USERNAME_REGEX))]
    pub username: String,

    #[validate(email)]
    pub email: String,

    #[validate(length(min = 8, max = 100))]
    #[serde(skip_serializing)]
    pub password: String,

    #[validate(length(min = 2, max = 30))]
    pub display_name: Option<String>,

    #[validate(length(max = 1000))]
    pub description: Option<String>,

    #[validate(length(min = 2, max = 15))]
    pub pronouns: Option<String>,

    #[validate(regex(path = GENDER_REGEX))]
    pub gender: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct UpdateUser {
    #[validate(length(min = 2, max = 30), regex(path = USERNAME_REGEX))]
    pub username: Option<String>,
    #[validate(length(min = 2, max = 30))]
    pub display_name: Option<String>,
    #[validate(length(max = 1000))]
    pub description: Option<String>,
    #[validate(length(min = 2, max = 15))]
    pub pronouns: Option<String>,
    #[validate(regex(path = GENDER_REGEX))]
    pub gender: Option<String>,
}

pub struct UserSearch {
    pub pagination: PaginatedRequest,
    pub text_query: Option<String>,
    pub created_before: Option<DateTime<Utc>>,
    pub created_after: Option<DateTime<Utc>>,
    pub verified: Option<bool>,
}


pub struct UserRepository {
    pool: PgPool,
}

impl UserRepository {
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, user: CreateUser) -> AppResult<User> {
        user.validate()?;

        if self.username_exists(&user.username).await? {
            return Err(bad_request("username is in use"));
        }

        if self.email_exists(&user.email).await? {
            return Err(bad_request("email is in use"));
        }

        // TODO: we should also check against haveibeenpwned

        // https://thecopenhagenbook.com/password-authentication#input-validation
        // > Use libraries like zxcvbn to check for weak passwords.
        let password_strength = zxcvbn::zxcvbn(&user.password, &[]);
        if password_strength.score() < Score::Three {
            let message = 
                password_strength.feedback().map_or("password is too weak".to_string(), |reason| format!("password is too weak: {reason}"));

            return Err(bad_request(message));
        }

        let password_hash = crate::util::hash(&user.password)
            .map_err(|_| internal_error("password hash failed"))?;

        let result = sqlx::query_as!(
            User,
            r#"
                insert into users
                    (username, email, password_hash, display_name, description,
                     pronouns, gender)
                values
                    ($1, $2, $3, $4, $5, $6, $7)
                returning *
                "#,
            user.username,
            user.email.to_lowercase(),
            password_hash,
            user.display_name,
            user.description,
            user.pronouns,
            user.gender
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(result)
    }

    pub async fn find_by_id(&self, id: Uuid) -> AppResult<User> {
        let result = sqlx::query_as!(
            User,
            "SELECT id, username, email, password_hash, display_name, description, pronouns, gender, profile_picture_object_id, verified_at, created_at, updated_at FROM users WHERE id = $1",
            id
        )
        .fetch_optional(&self.pool)
        .await?;

        if let Some(user) = result {
            Ok(user)
        } else {
            Err(not_found(format!("user '{id}'")))
        }
    }

    pub async fn find_by_username(&self, username: &str) -> AppResult<User> {
        let result = sqlx::query_as!(
            User,
            "SELECT id, username, email, password_hash, display_name, description, pronouns, gender, profile_picture_object_id, verified_at, created_at, updated_at FROM users WHERE username = $1",
            username
        )
        .fetch_optional(&self.pool)
        .await?;

        if let Some(user) = result {
            Ok(user)
        } else {
            Err(not_found(format!("user '{username}'")))
        }
    }

    pub async fn find_by_email(&self, email: &str) -> AppResult<User> {
        let result = sqlx::query_as!(
            User,
            "SELECT id, username, email, password_hash, display_name, description, pronouns, gender, profile_picture_object_id, verified_at, created_at, updated_at FROM users WHERE email = $1",
            email
        )
        .fetch_optional(&self.pool)
        .await?;

        if let Some(user) = result {
            Ok(user)
        } else {
            Err(not_found(format!("user '{email}'")))
        }
    }

    pub async fn update(&self, id: Uuid, updates: UpdateUser) -> AppResult<User> {
        if let Some(username) = &updates.username {
            if self.username_exists(username).await? {
                return Err(bad_request("Username is in use"));
            }
        }

        let result = sqlx::query_as!(
            User,
            r#"
            UPDATE users 
            SET username = COALESCE($2, username),
                display_name = COALESCE($3, display_name),
                description = COALESCE($4, description),
                pronouns = COALESCE($5, pronouns),
                gender = COALESCE($6, gender),
                updated_at = CURRENT_TIMESTAMP
            WHERE id = $1
            RETURNING id, username, email, password_hash, display_name, description, pronouns, gender, profile_picture_object_id, verified_at, created_at, updated_at
            "#,
            id,
            updates.username,
            updates.display_name,
            updates.description,
            updates.pronouns,
            updates.gender
        )
        .fetch_optional(&self.pool)
        .await?;

        if let Some(user) = result {
            Ok(user)
        } else {
            Err(not_found(format!("user '{id}'")))
        }
    }

    pub async fn delete(&self, id: Uuid) -> AppResult<bool> {
        let result = sqlx::query!("DELETE FROM users WHERE id = $1", id)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn search(&self, search: UserSearch) -> AppResult<PaginatedResponse<User>> {
        let limit = search.pagination.limit as i64 + 1; // fetch one extra to check if there's more

        let query = if let Some(text) = &search.text_query {
            sqlx::query_as!(
                User,
                r#"
                SELECT id, username, email, password_hash, display_name, description,
                       pronouns, gender, profile_picture_object_id, verified_at, created_at, updated_at
                FROM users
                WHERE (similarity(username, $1) > 0.2 OR similarity(COALESCE(description, ''), $1) > 0.2)
                  AND ($2::timestamptz IS NULL OR created_at <= $2)
                  AND ($3::timestamptz IS NULL OR created_at >= $3)
                  AND ($4::bool IS NULL OR (verified_at IS NOT NULL) = $4)
                  AND ($5::uuid IS NULL OR (
                    CASE WHEN $6 = 'Forward' THEN id > $5
                         WHEN $6 = 'Backward' THEN id < $5
                    END
                  ))
                ORDER BY
                  GREATEST(similarity(username, $1), similarity(COALESCE(description, ''), $1)) DESC,
                  id DESC
                LIMIT $7
                "#,
                text,
                search.created_before,
                search.created_after,
                search.verified,
                search.pagination.cursor,
                format!("{:?}", search.pagination.direction),
                limit
            )
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query_as!(
                User,
                r#"
                SELECT id, username, email, password_hash, display_name, description,
                       pronouns, gender, profile_picture_object_id, verified_at, created_at, updated_at
                FROM users
                WHERE ($1::timestamptz IS NULL OR created_at <= $1)
                  AND ($2::timestamptz IS NULL OR created_at >= $2)
                  AND ($3::bool IS NULL OR (verified_at IS NOT NULL) = $3)
                  AND ($4::uuid IS NULL OR (
                    CASE WHEN $5 = 'Forward' THEN id > $4
                         WHEN $5 = 'Backward' THEN id < $4
                    END
                  ))
                ORDER BY created_at DESC, id DESC
                LIMIT $6
                "#,
                search.created_before,
                search.created_after,
                search.verified,
                search.pagination.cursor,
                format!("{:?}", search.pagination.direction),
                limit
            )
            .fetch_all(&self.pool)
            .await?
        };

        let mut items = query;
        // TODO: uhhh lol is that right?
        let has_more = items.len() > search.pagination.limit.try_into().unwrap_or(0);

        if has_more {
            items.pop();
        }

        let next_cursor = if has_more {
            items.last().map(|u| u.id)
        } else {
            None
        };

        let previous_cursor = if search.pagination.cursor.is_some() {
            items.first().map(|u| u.id)
        } else {
            None
        };

        Ok(PaginatedResponse {
            items,
            pages_left: if has_more { 1 } else { 0 },
            next_cursor,
            previous_cursor,
        })
    }

    pub async fn list(&self, limit: i64, offset: i64) -> AppResult<Vec<User>> {
        let result = sqlx::query_as!(
            User,
            "SELECT id, username, email, password_hash, display_name, description, pronouns, gender, profile_picture_object_id, verified_at, created_at, updated_at FROM users ORDER BY created_at DESC LIMIT $1 OFFSET $2",
            limit,
            offset
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(result)
    }

    pub async fn count(&self) -> AppResult<i64> {
        let result = sqlx::query!("SELECT COUNT(*) as count FROM users")
            .fetch_one(&self.pool)
            .await?;

        Ok(result.count.unwrap_or(0))
    }

    pub async fn username_exists(&self, username: &str) -> AppResult<bool> {
        let result = sqlx::query!(
            "SELECT 1 as exists FROM users WHERE username = $1",
            username
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(result.is_some())
    }

    pub async fn email_exists(&self, email: &str) -> AppResult<bool> {
        let result = sqlx::query!("SELECT 1 as exists FROM users WHERE email = $1", email)
            .fetch_optional(&self.pool)
            .await?;

        Ok(result.is_some())
    }

    pub async fn verify(
        &self,
        user_id: Uuid,
        email: &str,
        email_verification_token: &str,
    ) -> AppResult<User> {
        let token_repo = EmailVerificationTokenRepository::new(self.pool.clone());
        let token = token_repo
            .find(user_id, email, email_verification_token)
            .await?;
        if let Some(token) = token {
            let mut tx = self.pool.begin().await?;
            // verify user, invalidate token
            let result = sqlx::query_as!(
                User,
                r#"
                UPDATE users
                SET verified_at = NOW()
                WHERE id = $1 AND verified_at IS NULL
                RETURNING *
                "#,
                user_id
            )
            .fetch_optional(&mut *tx)
            .await?;

            if let Some(user) = result {
                token_repo.invalidate(token.id).await?;
                tx.commit().await?;
                Ok(user)
            } else {
                tx.rollback().await?;
                Err(not_found(format!("user '{user_id}'")))
            }
        } else {
            Err(not_found("verification token".to_string()))
        }
    }

    pub async fn update_profile_picture(
        &self,
        user_id: Uuid,
        object_key: &str,
    ) -> AppResult<Option<User>> {
        let result = sqlx::query_as!(
            User,
            r#"
            UPDATE users 
            SET profile_picture_object_id = $2,
                updated_at = CURRENT_TIMESTAMP
            WHERE id = $1
            RETURNING id, username, email, password_hash, display_name, description, pronouns, gender, profile_picture_object_id, verified_at, created_at, updated_at
            "#,
            user_id,
            Some(object_key)
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(result)
    }
}

crate::util::repo_from_parts!(UserRepository);
