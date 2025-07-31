use std::{cell::LazyCell, sync::LazyLock};

use anyhow::{Result, anyhow, bail};
use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};
use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use thiserror::Error;
use validator::{Validate, ValidationError};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct User {
    pub id: i32,
    pub username: String,
    pub email: String,
    pub password_hash: String,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub pronouns: Option<String>,
    pub gender: Option<String>,
    pub profile_picture_object_id: Option<String>,
    pub verified_at: Option<DateTime<Utc>>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

static USERNAME_REGEX: LazyLock<Regex> = re!("^([a-z0-9](-|_)?)+[a-z0-9]");
static GENDER_REGEX: LazyLock<Regex> = re!("^([a-fA-F0-9]{3}){1,2}$"); // hex code

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CreateUser {
    #[validate(length(min = 2, max = 30), regex(path = USERNAME_REGEX))]
    pub username: String,

    #[validate(email)]
    pub email: String,

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

pub struct UserRepository {
    pool: PgPool,
}

use crate::re;

fn hash_password(password: &str) -> Result<String, argon2::password_hash::Error> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let password_hash = argon2.hash_password(password.as_bytes(), &salt)?;
    Ok(password_hash.to_string())
}

pub fn verify_password(password: &str, hash: &str) -> Result<bool, argon2::password_hash::Error> {
    let parsed_hash = PasswordHash::new(hash)?;
    let argon2 = Argon2::default();
    Ok(argon2
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok())
}

impl UserRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, user: CreateUser) -> Result<User> {
        if self.username_exists(&user.username).await? {
            bail!("Username is in use");
        }

        let password_hash =
            hash_password(&user.password).map_err(|_| anyhow!("Password hash failed"))?;

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
            user.email,
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

    pub async fn find_by_id(&self, id: i32) -> Result<Option<User>, sqlx::Error> {
        let result = sqlx::query_as!(
            User,
            "SELECT id, username, email, password_hash, display_name, description, pronouns, gender, profile_picture_object_id, verified_at, created_at, updated_at FROM users WHERE id = $1",
            id
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(result)
    }

    pub async fn find_by_username(&self, username: &str) -> Result<Option<User>, sqlx::Error> {
        let result = sqlx::query_as!(
            User,
            "SELECT id, username, email, password_hash, display_name, description, pronouns, gender, profile_picture_object_id, verified_at, created_at, updated_at FROM users WHERE username = $1",
            username
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(result)
    }

    pub async fn find_by_email(&self, email: &str) -> Result<Option<User>, sqlx::Error> {
        let result = sqlx::query_as!(
            User,
            "SELECT id, username, email, password_hash, display_name, description, pronouns, gender, profile_picture_object_id, verified_at, created_at, updated_at FROM users WHERE email = $1",
            email
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(result)
    }

    pub async fn update(&self, id: i32, updates: UpdateUser) -> Result<Option<User>> {
        if let Some(username) = &updates.username {
            if self.username_exists(username).await? {
                bail!("Username is in use");
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

        Ok(result)
    }

    pub async fn delete(&self, id: i32) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!("DELETE FROM users WHERE id = $1", id)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn list(&self, limit: i64, offset: i64) -> Result<Vec<User>, sqlx::Error> {
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

    pub async fn count(&self) -> Result<i64, sqlx::Error> {
        let result = sqlx::query!("SELECT COUNT(*) as count FROM users")
            .fetch_one(&self.pool)
            .await?;

        Ok(result.count.unwrap_or(0))
    }

    pub async fn username_exists(&self, username: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            "SELECT 1 as exists FROM users WHERE username = $1",
            username
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(result.is_some())
    }

    pub async fn email_exists(&self, email: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!("SELECT 1 as exists FROM users WHERE email = $1", email)
            .fetch_optional(&self.pool)
            .await?;

        Ok(result.is_some())
    }

    pub async fn verify_user(&self, id: i32) -> Result<Option<User>, sqlx::Error> {
        let result = sqlx::query_as!(
            User,
            r#"
            UPDATE users 
            SET verified_at = CURRENT_TIMESTAMP,
                updated_at = CURRENT_TIMESTAMP
            WHERE id = $1
            RETURNING id, username, email, password_hash, display_name, description, pronouns, gender, profile_picture_object_id, verified_at, created_at, updated_at
            "#,
            id
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(result)
    }
    pub async fn is_verified(&self, id: i32) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!("SELECT verified_at FROM users WHERE id = $1", id)
            .fetch_optional(&self.pool)
            .await?;

        Ok(result.map(|row| row.verified_at.is_some()).unwrap_or(false))
    }
}
