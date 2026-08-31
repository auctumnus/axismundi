use std::sync::LazyLock;

use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize, Serializer};
use serde_json::{Value, json};
use sqlx::FromRow;
use uuid::Uuid;
use validator::{Validate, ValidateArgs, ValidationErrors};

use crate::{
    config::CONFIG,
    embed::{GenericEmbed, truncate_description},
    err::{AppResult, bad_request, internal_error, not_found},
    model::{
        email_verification_tokens::EmailVerificationToken,
        languages::Language,
        password_reset_tokens::{PasswordResetToken, PasswordResetTokenRepository},
    },
    pagination::{PaginatedRequest, PaginatedResponse},
    util::{AppState, PasswordValidationContext, ensure_verified, re, s3::S3, validate_password},
};

use super::email_verification_tokens::EmailVerificationTokenRepository;

fn serialize_object_key<S>(object_id: &str, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    if object_id.is_empty() {
        serializer.serialize_none()
    } else {
        serializer.serialize_str(&S3.get_profile_picture_url(object_id))
    }
}

fn serialize_banner_key<S>(object_id: &str, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    if object_id.is_empty() {
        serializer.serialize_none()
    } else {
        serializer.serialize_str(&S3.get_banner_url(object_id))
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
    pub display_name: String,
    pub description: String,
    pub pronouns: String,
    pub gender: String,
    pub bookmark: String,

    #[serde(
        rename(serialize = "profile_picture_url"),
        serialize_with = "serialize_object_key"
    )]
    pub profile_picture_object_id: String,
    #[serde(
        rename(serialize = "banner_url"),
        serialize_with = "serialize_banner_key"
    )]
    #[sqlx(default)]
    pub banner_object_id: String,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl User {
    pub const fn is_verified(&self) -> bool {
        self.verified_at.is_some()
    }

    pub fn get_profile_picture_url(&self) -> Option<String> {
        if self.profile_picture_object_id.is_empty() {
            None
        } else {
            Some(S3.get_profile_picture_url(&self.profile_picture_object_id))
        }
    }

    pub fn get_banner_url(&self) -> Option<String> {
        if self.banner_object_id.is_empty() {
            None
        } else {
            Some(S3.get_banner_url(&self.banner_object_id))
        }
    }

    pub fn is_admin(&self) -> bool {
        self.tags.iter().any(|tag| tag == "admin")
    }

    pub fn is_moderator(&self) -> bool {
        self.tags.iter().any(|tag| tag == "moderator")
    }

    pub fn is_banned(&self) -> bool {
        self.tags.iter().any(|tag| tag == "banned")
    }

    pub fn name(&self) -> &str {
        if self.display_name.is_empty() {
            &self.username
        } else {
            &self.display_name
        }
    }
}

pub static USERNAME_REGEX: LazyLock<Regex> = re!("^([a-z0-9](-|_)?)+[a-z0-9]$");
static GENDER_REGEX: LazyLock<Regex> = re!("^#?([a-fA-F0-9]{3}){1,2}$|^$"); // hex code

#[derive(Debug, Serialize, Deserialize, Validate)]
#[validate(context = "PasswordValidationContext<'v_a>")]
pub struct CreateUser {
    #[validate(length(min = 2, max = 30), regex(path = USERNAME_REGEX))]
    pub username: String,

    #[validate(email)]
    pub email: String,

    #[serde(skip_serializing)]
    #[validate(
        length(min = 8, max = 100),
        custom(function = "crate::util::validate_password", use_context)
    )]
    pub password: String,

    #[validate(length(max = 30))]
    pub display_name: Option<String>,

    #[validate(length(max = 1000))]
    pub description: Option<String>,

    #[validate(length(min = 2, max = 30))]
    pub pronouns: Option<String>,

    #[validate(regex(path = GENDER_REGEX))]
    pub gender: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
#[validate(context = "PasswordValidationContext<'v_a>")]
pub struct UpdateUser {
    #[validate(length(min = 2, max = 30), regex(path = USERNAME_REGEX))]
    pub username: Option<String>,
    #[validate(email)]
    pub email: Option<String>,
    #[validate(length(max = 30))]
    pub display_name: Option<String>,
    #[validate(length(max = 1000))]
    pub description: Option<String>,
    #[validate(length(max = 30))]
    pub pronouns: Option<String>,
    #[validate(regex(path = GENDER_REGEX))]
    pub gender: Option<String>,

    #[validate(length(min = 8, max = 100))]
    #[serde(skip_serializing)]
    pub current_password: Option<String>,

    #[validate(
        length(min = 8, max = 100),
        custom(function = "crate::util::validate_password", use_context)
    )]
    #[serde(skip_serializing)]
    pub new_password: Option<String>,
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct UserSearch {
    pub q: Option<String>,
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
    pub verified: Option<bool>,
}

crate::util::text_query!(UserSearch);

#[derive(Clone)]
pub struct UserRepository {
    state: AppState,
}

impl UserRepository {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    pub fn render_description(user: &User) -> AppResult<String> {
        if user.description.is_empty() {
            Ok(String::new())
        } else {
            Ok(crate::md::render_md(&user.description)?)
        }
    }

    pub async fn create(&self, user: CreateUser) -> AppResult<(User, EmailVerificationToken)> {
        let user_inputs = vec![
            &user.username,
            &user.email,
            user.display_name.as_deref().unwrap_or(""),
        ];

        user.validate_with_args(&user_inputs.as_ref())?;

        let username_lower = user.username.to_lowercase();

        if self.username_exists(&username_lower).await? {
            return Err(bad_request("username is in use"));
        }

        if self.email_exists(&user.email).await? {
            return Err(bad_request("email is in use"));
        }

        let password_hash = crate::util::hash(&user.password)
            .map_err(|_| internal_error("password hash failed"))?;

        // select a random default profile picture
        let default_pfps = [
            "default-pfps/1.webp",
            "default-pfps/2.webp",
            "default-pfps/3.webp",
        ];
        let random_pfp = default_pfps[rand::random::<usize>() % default_pfps.len()];

        // Begin transaction: create user + verification token + bookmark atomically
        let mut tx = self.state.pool.begin().await?;

        let gender = user
            .gender
            .map(|g| g.strip_prefix("#").unwrap_or(&g).to_lowercase())
            .unwrap_or_default();

        let user_result = sqlx::query!(
            r#"
                insert into users
                    (username, email, password_hash, display_name, description,
                     pronouns, gender, profile_picture_object_id)
                values
                    ($1, $2, $3, $4, $5, $6, $7, $8)
                returning id, username, email, password_hash, display_name, description, pronouns, gender, profile_picture_object_id, verified_at, created_at, updated_at
                "#,
            username_lower,
            user.email.to_lowercase(),
            password_hash,
            user.display_name.unwrap_or_default(),
            user.description.unwrap_or_default(),
            user.pronouns.unwrap_or_default(),
            gender,
            random_pfp
        )
        .fetch_one(&mut *tx)
        .await?;

        // Generate and insert bookmark
        let slug = crate::model::bookmarks::BookmarkRepository::generate_slug();
        sqlx::query!(
            "INSERT INTO bookmarks (slug, item, resource) VALUES ($1, $2, 'user')",
            slug,
            user_result.id
        )
        .execute(&mut *tx)
        .await?;

        let result = User {
            id: user_result.id,
            username: user_result.username,
            email: user_result.email,
            password_hash: user_result.password_hash,
            display_name: user_result.display_name,
            description: user_result.description,
            pronouns: user_result.pronouns,
            gender: user_result.gender,
            profile_picture_object_id: user_result.profile_picture_object_id,
            banner_object_id: String::new(),
            verified_at: user_result.verified_at,
            tags: vec![],
            created_at: user_result.created_at,
            updated_at: user_result.updated_at,
            bookmark: slug,
        };

        let token_repo = EmailVerificationTokenRepository::new(self.state.clone());
        let token = token_repo
            .create(&mut tx, result.id, result.email.clone())
            .await?;

        // Commit transaction - user, bookmark, and token are now persisted
        tx.commit().await?;

        // Send email outside transaction - if this fails, token exists for retry/resend
        if let Err(e) = token_repo
            .send(result.id, &result.email, &token.token)
            .await
        {
            // Log the error but don't fail the registration
            // User exists with token, can implement resend endpoint later
            tracing::error!(
                "Failed to send verification email to {}: {}",
                result.email,
                e
            );
        }

        Ok((result, token))
    }

    pub async fn find_by_id(&self, id: Uuid) -> AppResult<User> {
        let result = sqlx::query_as!(
            User,
            r#"
                SELECT
                    users.id,
                    users.username,
                    users.email,
                    users.password_hash,
                    users.display_name,
                    users.description,
                    users.pronouns,
                    users.gender,
                    users.profile_picture_object_id,
                    users.banner_object_id,
                    users.verified_at,
                    users.tags,
                    users.created_at,
                    users.updated_at,
                    COALESCE(bookmarks.slug, '')::text as "bookmark!"
                FROM users
                LEFT JOIN bookmarks ON bookmarks.item = users.id AND bookmarks.resource = 'user'
                WHERE users.id = $1
            "#,
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
        let username_lower = username.to_lowercase();
        let result = sqlx::query_as!(
            User,
            r#"
                SELECT
                    users.id,
                    users.username,
                    users.email,
                    users.password_hash,
                    users.display_name,
                    users.description,
                    users.pronouns,
                    users.gender,
                    users.profile_picture_object_id,
                    users.banner_object_id,
                    users.verified_at,
                    users.tags,
                    users.created_at,
                    users.updated_at,
                    COALESCE(bookmarks.slug, '')::text as "bookmark!"
                FROM users
                LEFT JOIN bookmarks ON bookmarks.item = users.id AND bookmarks.resource = 'user'
                WHERE users.username = $1
            "#,
            username_lower
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
            r#"
                SELECT
                    users.id,
                    users.username,
                    users.email,
                    users.password_hash,
                    users.display_name,
                    users.description,
                    users.pronouns,
                    users.gender,
                    users.profile_picture_object_id,
                    users.banner_object_id,
                    users.verified_at,
                    users.tags,
                    users.created_at,
                    users.updated_at,
                    COALESCE(bookmarks.slug, '')::text as "bookmark!"
                FROM users
                LEFT JOIN bookmarks ON bookmarks.item = users.id AND bookmarks.resource = 'user'
                WHERE users.email = $1
            "#,
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

        let user_inputs = vec![
            updates.username.as_deref().unwrap_or(&requestor.username),
            updates.email.as_deref().unwrap_or(&requestor.email),
            updates
                .display_name
                .as_deref()
                .unwrap_or(if requestor.display_name.is_empty() {
                    ""
                } else {
                    &requestor.display_name
                }),
        ];

        updates.validate_with_args(&user_inputs.as_ref())?;

        ensure_verified(requestor)?;

        let username_lower = updates.username.as_ref().map(|u| u.to_lowercase());

        if let Some(username) = &username_lower {
            if self.username_exists(username).await? {
                return Err(bad_request("Username is in use"));
            }
        }

        // disallow changing email and password at the same time
        if updates.email.is_some() && updates.new_password.is_some() {
            return Err(bad_request(
                "Cannot change email and password at the same time",
            ));
        }

        let tokens = EmailVerificationTokenRepository::new(self.state.clone());
        let mut tx = self.state.pool.begin().await?;

        // we have to handle sending the email outside of the tx
        // otherwise the tx could be held open too long
        let token = if let Some(email) = &updates.email {
            if self.email_exists(email).await? {
                return Err(bad_request("Email is in use"));
            }

            // The user should be asked for their password
            if let Some(current_password) = &updates.current_password {
                let is_valid = crate::util::verify(current_password, &requestor.password_hash)
                    .map_err(|_| internal_error("password verification failed"))?;
                if !is_valid {
                    return Err(bad_request("Current password is incorrect"));
                }
            } else {
                return Err(bad_request("Current password is required to change email"));
            }

            // changing email requires re-verification
            Some(tokens.create(&mut tx, id, email.to_lowercase()).await?)
        } else {
            None
        };

        let password_hash = if let Some(new_password) = &updates.new_password {
            // The user should be asked for their current password
            if let Some(current_password) = &updates.current_password {
                let is_valid = crate::util::verify(current_password, &requestor.password_hash)
                    .map_err(|_| internal_error("password verification failed"))?;
                if !is_valid {
                    return Err(bad_request("Current password is incorrect"));
                }
            } else {
                return Err(bad_request(
                    "Current password is required to change password",
                ));
            }

            &crate::util::hash(new_password).map_err(|_| internal_error("password hash failed"))?
        } else {
            &requestor.password_hash
        };

        // None = don't change, Some("") = clear, Some("value") = set
        let display_name_final = updates
            .display_name
            .as_ref()
            .map_or_else(|| requestor.display_name.clone(), |s| s.clone());
        let description_final = updates
            .description
            .as_ref()
            .map_or_else(|| requestor.description.clone(), |s| s.clone());
        let pronouns_final = updates
            .pronouns
            .as_ref()
            .map_or_else(|| requestor.pronouns.clone(), |s| s.clone());

        // Handle gender (which includes # prefix stripping)
        let gender_final = if let Some(g) = &updates.gender {
            if g.is_empty() {
                String::new()
            } else {
                g.strip_prefix("#").unwrap_or(g).to_lowercase()
            }
        } else {
            requestor.gender.clone()
        };

        let result = sqlx::query_as!(
            User,
            r#"
            UPDATE users
            SET username = COALESCE($2, username),
                display_name = $3,
                description = $4,
                pronouns = $5,
                gender = $6,
                updated_at = CURRENT_TIMESTAMP,
                password_hash = $7
            WHERE id = $1
            RETURNING users.*, (SELECT slug FROM bookmarks WHERE item = users.id AND resource = 'user') as "bookmark!"
            "#,
            id,
            username_lower,
            display_name_final,
            description_final,
            pronouns_final,
            gender_final,
            password_hash
        )
        .fetch_optional(&mut *tx)
        .await?;

        tx.commit().await?;

        if let Some(token) = token {
            let email = updates.email.unwrap();
            if let Err(e) = tokens.send(id, &email, &token.token).await {
                // Log the error but don't fail the update
                // User exists with token, can implement resend endpoint later
                tracing::error!("Failed to send verification email to {}: {}", email, e);
            }
            if let Err(e) = self
                .state
                .email_service
                .send_email_change_notification(id, &requestor.email, &email)
                .await
            {
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

    #[allow(dead_code)]
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
                SELECT
                    users.id,
                    users.username,
                    users.email,
                    users.password_hash,
                    users.display_name,
                    users.description,
                    users.pronouns,
                    users.gender,
                    users.profile_picture_object_id,
                    users.banner_object_id,
                    users.verified_at,
                    users.tags,
                    users.created_at,
                    users.updated_at,
                    COALESCE(bookmarks.slug, '')::text as "bookmark!"
                FROM users
                LEFT JOIN bookmarks ON bookmarks.item = users.id AND bookmarks.resource = 'user'
                WHERE
                ($1::BOOL IS NULL OR (users.verified_at IS NOT NULL) = $1)
                AND ($2::TIMESTAMPTZ IS NULL OR users.created_at < $2)
                AND ($3::TIMESTAMPTZ IS NULL OR users.created_at > $3)
                ORDER BY (
                    CASE
                        WHEN $4::TEXT IS NOT NULL AND users.username ILIKE '%' || $4 || '%' THEN 100.0
                        WHEN $4::TEXT IS NOT NULL AND users.display_name ILIKE '%' || $4 || '%' THEN 90.0
                        WHEN $4::TEXT IS NOT NULL AND users.description ILIKE '%' || $4 || '%' THEN 80.0
                        ELSE 0.0
                    END +
                    CASE WHEN $4::TEXT IS NOT NULL THEN
                        similarity(users.username, $4) * 3.0 +
                        COALESCE(similarity(users.display_name, $4), 0.0) * 2.0 +
                        COALESCE(similarity(users.description, $4), 0.0) * 1.0
                    ELSE 0.0
                    END
                ) DESC, users.created_at DESC, users.id DESC
                LIMIT $5
                OFFSET $6
            "#,
            search.verified,
            search.created_before,
            search.created_after,
            search.q,
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

    #[allow(dead_code)]
    pub async fn list(&self, limit: i64, offset: i64) -> AppResult<Vec<User>> {
        let result = sqlx::query_as!(
            User,
            r#"
                SELECT
                    users.id,
                    users.username,
                    users.email,
                    users.password_hash,
                    users.display_name,
                    users.description,
                    users.pronouns,
                    users.gender,
                    users.profile_picture_object_id,
                    users.banner_object_id,
                    users.verified_at,
                    users.tags,
                    users.created_at,
                    users.updated_at,
                    COALESCE(bookmarks.slug, '')::text as "bookmark!"
                FROM users
                LEFT JOIN bookmarks ON bookmarks.item = users.id AND bookmarks.resource = 'user'
                ORDER BY users.created_at DESC
                LIMIT $1
                OFFSET $2
            "#,
            limit,
            offset
        )
        .fetch_all(&self.state.pool)
        .await?;

        Ok(result)
    }

    #[allow(dead_code)]
    pub async fn count(&self) -> AppResult<i64> {
        let result = sqlx::query!("SELECT COUNT(*) as count FROM users")
            .fetch_one(&self.state.pool)
            .await?;

        Ok(result.count.unwrap_or(0))
    }

    pub async fn username_exists(&self, username: &str) -> AppResult<bool> {
        let username_lower = username.to_lowercase();
        let result = sqlx::query!(
            "SELECT 1 as exists FROM users WHERE username = $1",
            username_lower
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
                RETURNING users.*, (SELECT slug FROM bookmarks WHERE item = users.id AND resource = 'user') as "bookmark!"
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

                let activity_repo =
                    crate::model::user_activities::UserActivityRepository::new(self.state.clone());
                let _ = activity_repo
                    .create_with_tx(
                        user.id,
                        crate::model::user_activities::ActivityType::UserJoined,
                        user.id,
                        "user",
                        None,
                        None,
                        &mut tx,
                    )
                    .await?;

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
            RETURNING users.*, (SELECT slug FROM bookmarks WHERE item = users.id AND resource = 'user') as "bookmark!"
            "#,
            user_id,
            object_key
        )
        .fetch_optional(&self.state.pool)
        .await?;

        Ok(result)
    }

    pub async fn update_banner(
        &self,
        requestor: &User,
        user_id: Uuid,
        object_key: &str,
    ) -> AppResult<Option<User>> {
        if requestor.id != user_id {
            return Err(crate::err::forbidden("cannot update another user's banner"));
        }

        ensure_verified(requestor)?;

        let result = sqlx::query_as!(
            User,
            r#"
            UPDATE users
            SET banner_object_id = $2,
                updated_at = CURRENT_TIMESTAMP
            WHERE id = $1
            RETURNING users.*, (SELECT slug FROM bookmarks WHERE item = users.id AND resource = 'user') as "bookmark!"
            "#,
            user_id,
            object_key
        )
        .fetch_optional(&self.state.pool)
        .await?;

        Ok(result)
    }

    pub async fn reset_password(
        &self,
        user_id: Uuid,
        token: PasswordResetToken,
        new_password: &str,
    ) -> AppResult<()> {
        let user = self.find_by_id(user_id).await?;
        validate_password(
            new_password,
            &[&user.username, &user.email, &user.display_name],
        )
        .map_err(|e| {
            let mut errors = ValidationErrors::new();
            errors.add("new_password", e);
            errors
        })?;
        let password_hash =
            crate::util::hash(new_password).map_err(|_| internal_error("password hash failed"))?;

        let mut tx = self.state.pool.begin().await?;

        // https://thecopenhagenbook.com/password-reset
        // You should even mark a user's email address as verified if they reset their password.
        let _result = sqlx::query!(
            r#"
            UPDATE users
            SET password_hash = $2, verified_at = NOW()
            WHERE id = $1
            "#,
            user_id,
            password_hash
        )
        .execute(&mut *tx)
        .await?;

        // invalidate token
        let tokens = PasswordResetTokenRepository::new(self.state.clone());
        tokens.invalidate_with_tx(token, &mut tx).await?;

        // Invalidate all existing sessions linked to the user when the user resets their password.
        let sessions = crate::model::sessions::SessionRepository::new(self.state.clone());
        sessions.invalidate_all_with_tx(user_id, &mut tx).await?;

        tx.commit().await?;

        Ok(())
    }

    pub async fn top_languages(&self, user_id: Uuid, limit: i64) -> AppResult<Vec<Language>> {
        let result = sqlx::query_as!(
            Language,
            r#"
            SELECT
                    languages.id,
                    languages.code,
                    languages.name,
                    languages.description,
                    languages.private,
                    languages.like_count,
                    languages.banner_object_id,
                    languages.created_at,
                    languages.updated_at,
                    languages.created_by,
                    languages.updated_by,
                    COALESCE(bookmarks.slug, '')::text as "bookmark!"
                FROM languages
                LEFT JOIN (
                    SELECT
                        CASE
                            WHEN entity_type = 'language' THEN entity_id
                            WHEN related_entity_type = 'language' THEN related_entity_id
                        END as lang_id,
                        MAX(timestamp) as last_activity
                    FROM user_activities
                    WHERE user_id = $1
                        AND (entity_type = 'language' OR related_entity_type = 'language')
                    GROUP BY lang_id
                ) as la ON la.lang_id = languages.id
                LEFT JOIN bookmarks ON bookmarks.item = languages.id AND bookmarks.resource = 'language'
                WHERE languages.created_by = $1
                ORDER BY COALESCE(la.last_activity, languages.created_at) DESC
                LIMIT $2
            "#,
            user_id,
            limit
        )
        .fetch_all(&self.state.pool)
        .await?;

        Ok(result)
    }

    pub fn as_json_ld(user: &User) -> Value {
        let mut base = json!({
            "@type": "Person",
            "name": user.name(),
            "alternateName": user.username,
            "url": format!("{}/bookmarks/{}", CONFIG.public_url_base, &user.bookmark),
            "image": user.get_profile_picture_url().unwrap_or_else(|| format!("{}/assets/default-pfp.webp", CONFIG.public_url_base)),
        });

        if !user.pronouns.is_empty() {
            base["pronouns"] = json!(&user.pronouns);
        }
        if !user.gender.is_empty() {
            base["gender"] = json!(&user.gender);
        }
        if !user.description.is_empty() {
            base["description"] = json!(&user.description);
        }

        base
    }

    pub fn as_embed(user: &User) -> GenericEmbed {
        let title = if user.display_name.is_empty() {
            format!("@{}", user.username)
        } else {
            format!("{} (@{})", user.display_name, user.username)
        };
        GenericEmbed {
            url: format!("{}/users/{}", &crate::CONFIG.public_url_base, user.username),
            title,
            description: truncate_description(&user.description),
            author: None,
            image: user.get_profile_picture_url(),
            color: if user.gender.is_empty() {
                None
            } else {
                Some(format!("#{}", user.gender))
            },
        }
    }
}

crate::util::repo_from_parts!(UserRepository);

#[async_trait::async_trait]
impl crate::model::bookmarks::ResolveBookmark for UserRepository {
    async fn resolve_bookmark(
        &self,
        item: Uuid,
        link_type: crate::model::bookmarks::LinkType,
    ) -> AppResult<String> {
        // api: /api/users/{username}
        // web: /users/{username}
        let user = self.find_by_id(item).await?;

        let slug = match link_type {
            crate::model::bookmarks::LinkType::Web => format!("/users/{}", user.username),
            crate::model::bookmarks::LinkType::Api => format!("/api/users/{}", user.username),
        };

        Ok(slug)
    }
}
