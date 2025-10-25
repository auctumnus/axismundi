use std::sync::LazyLock;

use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize, Serializer};
use sqlx::FromRow;
use uuid::Uuid;
use validator::Validate;
use zxcvbn::Score;

use crate::{
    err::{AppResult, bad_request, internal_error, not_found},
    pagination::{PaginatedRequest, PaginatedResponse},
    util::{AppState, ensure_verified, re, s3::S3},
};

use super::email_verification_tokens::EmailVerificationTokenRepository;

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
    #[validate(email)]
    pub email: Option<String>,
    #[validate(length(min = 2, max = 30))]
    pub display_name: Option<String>,
    #[validate(length(max = 1000))]
    pub description: Option<String>,
    #[validate(length(min = 2, max = 15))]
    pub pronouns: Option<String>,
    #[validate(regex(path = GENDER_REGEX))]
    pub gender: Option<String>,

    #[validate(length(min = 8, max = 100))]
    #[serde(skip_serializing)]
    pub current_password: Option<String>,

    #[validate(length(min = 8, max = 100))]
    #[serde(skip_serializing)]
    pub new_password: Option<String>,
}

#[derive(Deserialize)]
pub struct UserSearch {
    pub text_query: Option<String>,
    pub created_before: Option<DateTime<Utc>>,
    pub created_after: Option<DateTime<Utc>>,
    pub verified: Option<bool>,
}

#[derive(Clone)]
pub struct UserRepository {
    state: AppState,
}

impl UserRepository {
    pub fn new(state: AppState) -> Self {
        Self { state }
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
            let message = password_strength
                .feedback()
                .map_or("password is too weak".to_string(), |reason| {
                    format!("password is too weak: {reason}")
                });

            return Err(bad_request(message));
        }

        let password_hash = crate::util::hash(&user.password)
            .map_err(|_| internal_error("password hash failed"))?;

        // Begin transaction: create user + verification token atomically
        let mut tx = self.state.pool.begin().await?;

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
        .fetch_one(&mut *tx)
        .await?;

        let token_repo = EmailVerificationTokenRepository::new(self.state.clone());
        let token = token_repo
            .create(&mut tx, result.id, result.email.clone())
            .await?;

        // Commit transaction - both user and token are now persisted
        tx.commit().await?;

        // Send email outside transaction - if this fails, token exists for retry/resend
        if let Err(e) = token_repo.send(result.id, &result.email, &token).await {
            // Log the error but don't fail the registration
            // User exists with token, can implement resend endpoint later
            tracing::error!(
                "Failed to send verification email to {}: {}",
                result.email,
                e
            );
        }

        Ok(result)
    }

    pub async fn find_by_id(&self, id: Uuid) -> AppResult<User> {
        let result = sqlx::query_as!(
            User,
            "SELECT id, username, email, password_hash, display_name, description, pronouns, gender, profile_picture_object_id, verified_at, created_at, updated_at FROM users WHERE id = $1",
            id
        )
        .fetch_optional(&self.state.pool)
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
        .fetch_optional(&self.state.pool)
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
            email.to_lowercase()
        )
        .fetch_optional(&self.state.pool)
        .await?;

        if let Some(user) = result {
            Ok(user)
        } else {
            Err(not_found(format!("user '{email}'")))
        }
    }

    pub async fn update(&self, requestor: &User, id: Uuid, updates: UpdateUser) -> AppResult<User> {
        if requestor.id != id {
            return Err(crate::err::forbidden(
                "cannot update another user's profile",
            ));
        }

        updates.validate()?;

        ensure_verified(requestor)?;

        if let Some(username) = &updates.username {
            if self.username_exists(username).await? {
                return Err(bad_request("Username is in use"));
            }
        }

        let tokens = EmailVerificationTokenRepository::new(self.state.clone());
        let mut tx = self.state.pool.begin().await?;

        // we have to handle sending the email outside of the tx
        // otherwise the tx could be held open too long
        let token = if let Some(email) = &updates.email {
            if self.email_exists(email).await? {
                return Err(bad_request("Email is in use"));
            }

            // changing email requires re-verification
            Some(tokens.create(&mut tx, id, email.to_lowercase()).await?)
        } else {
            None
        };

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
        .fetch_optional(&mut *tx)
        .await?;

        tx.commit().await?;

        if let Some(token) = token {
            let email = updates.email.unwrap();
            if let Err(e) = tokens.send(id, &email, &token).await {
                // Log the error but don't fail the update
                // User exists with token, can implement resend endpoint later
                tracing::error!(
                    "Failed to send verification email to {}: {}",
                    email,
                    e
                );
            }
            if let Err(e) = self.state.email_service.send_email_change_notification(
                id,
                &requestor.email,
                &email,
            ).await {
                tracing::error!(
                    "Failed to send email change notification to {}: {}",
                    &requestor.email,
                    e
                );
            }
        }

        if let Some(user) = result {
            Ok(user)
        } else {
            Err(not_found(format!("user '{id}'")))
        }
    }

    pub async fn delete(&self, requestor: &User, id: Uuid) -> AppResult<bool> {
        if requestor.id != id {
            return Err(crate::err::forbidden(
                "cannot delete another user's profile",
            ));
        }

        ensure_verified(requestor)?;

        let result = sqlx::query!("DELETE FROM users WHERE id = $1", id)
            .execute(&self.state.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn search(
        &self,
        pagination: PaginatedRequest,
        search: UserSearch,
    ) -> AppResult<PaginatedResponse<User>> {
        // search strategy:
        // - exact matches in username, display_name are weighted highly
        // - otherwise, we use similarity on username, display_name, description
        // - filter by verified status if specified

        let items_future = sqlx::query_as!(
            User,
            r#"
                SELECT *
                FROM users
                WHERE
                ($1::BOOL IS NULL OR (verified_at IS NOT NULL) = $1)
                AND ($2::TIMESTAMPTZ IS NULL OR created_at < $2)
                AND ($3::TIMESTAMPTZ IS NULL OR created_at > $3)
                ORDER BY (
                    CASE
                        WHEN $4::TEXT IS NOT NULL AND username ILIKE '%' || $4 || '%' THEN 100.0
                        WHEN $4::TEXT IS NOT NULL AND display_name ILIKE '%' || $4 || '%' THEN 90.0
                        WHEN $4::TEXT IS NOT NULL AND description ILIKE '%' || $4 || '%' THEN 80.0
                        ELSE 0.0
                    END +
                    CASE WHEN $4::TEXT IS NOT NULL THEN
                        similarity(username, $4) * 3.0 +
                        COALESCE(similarity(display_name, $4), 0.0) * 2.0 +
                        COALESCE(similarity(description, $4), 0.0) * 1.0
                    ELSE 0.0
                    END
                ) DESC, id
                LIMIT $5
                OFFSET $6
            "#,
            search.verified,
            search.created_before,
            search.created_after,
            search.text_query,
            i64::from(pagination.limit),
            i64::from(pagination.offset)
        )
        .fetch_all(&self.state.pool);

        let count_future = sqlx::query_scalar!(
            r#"
                SELECT COUNT(*)
                FROM users
                WHERE
                ($1::BOOL IS NULL OR (verified_at IS NOT NULL) = $1)
                AND ($2::TIMESTAMPTZ IS NULL OR created_at < $2)
                AND ($3::TIMESTAMPTZ IS NULL OR created_at > $3)
            "#,
            search.verified,
            search.created_before,
            search.created_after
        )
        .fetch_one(&self.state.pool);

        let (items, total_count) = tokio::try_join!(items_future, count_future)?;

        let total = total_count.unwrap_or(0);
        let has_more = (i64::from(pagination.offset) + items.len() as i64) < total;

        Ok(PaginatedResponse {
            items,
            total,
            offset: pagination.offset,
            limit: pagination.limit,
            has_more,
        })
    }

    pub async fn list(&self, limit: i64, offset: i64) -> AppResult<Vec<User>> {
        let result = sqlx::query_as!(
            User,
            "SELECT id, username, email, password_hash, display_name, description, pronouns, gender, profile_picture_object_id, verified_at, created_at, updated_at FROM users ORDER BY created_at DESC LIMIT $1 OFFSET $2",
            limit,
            offset
        )
        .fetch_all(&self.state.pool)
        .await?;

        Ok(result)
    }

    pub async fn count(&self) -> AppResult<i64> {
        let result = sqlx::query!("SELECT COUNT(*) as count FROM users")
            .fetch_one(&self.state.pool)
            .await?;

        Ok(result.count.unwrap_or(0))
    }

    pub async fn username_exists(&self, username: &str) -> AppResult<bool> {
        let result = sqlx::query!(
            "SELECT 1 as exists FROM users WHERE username = $1",
            username
        )
        .fetch_optional(&self.state.pool)
        .await?;

        Ok(result.is_some())
    }

    pub async fn email_exists(&self, email: &str) -> AppResult<bool> {
        let result = sqlx::query!(
            "SELECT 1 as exists FROM users WHERE email = $1",
            email.to_lowercase()
        )
        .fetch_optional(&self.state.pool)
        .await?;

        Ok(result.is_some())
    }

    pub async fn verify(
        &self,
        user_id: Uuid,
        email: &str,
        email_verification_token: &str,
    ) -> AppResult<User> {
        let token_repo = EmailVerificationTokenRepository::new(self.state.clone());
        let token = token_repo
            .find(user_id, email, email_verification_token)
            .await?;
        if let Some(token) = token {
            let mut tx = self.state.pool.begin().await?;
            // verify user, invalidate token
            // we set the email as well; this way, we can use email verification for
            // email change
            let result = sqlx::query_as!(
                User,
                r#"
                UPDATE users
                SET verified_at = NOW(), email = $2
                WHERE id = $1 AND verified_at IS NULL
                RETURNING *
                "#,
                user_id,
                email
            )
            .fetch_optional(&mut *tx)
            .await?;

            if let Some(user) = result {
                token_repo.invalidate(token.id).await?;

                // log out all sessions
                let session_repo =
                    crate::model::sessions::SessionRepository::new(self.state.clone());
                session_repo.invalidate_all(user_id).await?;

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
        requestor: &User,
        user_id: Uuid,
        object_key: &str,
    ) -> AppResult<Option<User>> {
        if requestor.id != user_id {
            return Err(crate::err::forbidden(
                "cannot update another user's profile picture",
            ));
        }

        ensure_verified(requestor)?;

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
        .fetch_optional(&self.state.pool)
        .await?;

        Ok(result)
    }
}

crate::util::repo_from_parts!(UserRepository);
