use base64::prelude::BASE64_STANDARD;
use chrono::{DateTime, Utc};
use sha2::Sha256;
use sqlx::PgPool;
use anyhow::{bail, Result};
use sha2::{Digest};
use base64::prelude::*;

use crate::util::stable_hash;

pub struct PasswordResetToken {
    pub id: i32,
    pub user_id: i32,
    pub token: String,
    pub invalidated_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

pub struct PasswordResetTokenRepository {
    pool: PgPool,
}

impl PasswordResetTokenRepository {
    pub fn new(pool: PgPool) -> Self {
        PasswordResetTokenRepository { pool }
    }

    pub async fn create(&self, user_id: i32) -> Result<String> {
        let token = nanoid::nanoid!(20);
        let expires_at = Utc::now() + chrono::Duration::days(1);
        // Tokens that require an extra level of security, **such as password reset tokens**,
        // should be hashed with SHA-256. 
        // https://thecopenhagenbook.com/server-side-tokens#storing-tokens
        let hashed_token = crate::util::stable_hash(&token);

        sqlx::query!(
            r#"
                INSERT INTO password_reset_tokens (user_id, token, expires_at)
                VALUES ($1, $2, $3)
            "#,
            user_id,
            hashed_token,
            expires_at
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(token)
    }

    pub async fn find_by_token(&self, token: &str) -> Result<Option<PasswordResetToken>> {
        let hashed_token = stable_hash(token);
        let result = sqlx::query_as!(
            PasswordResetToken,
            r#"
                SELECT * FROM password_reset_tokens
                WHERE token = $1 AND invalidated_at IS NULL AND expires_at > NOW()
            "#,
            hashed_token
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(result)
    }

    pub async fn invalidate(&self, token: PasswordResetToken) -> Result<PasswordResetToken> {
        let result = sqlx::query_as!(
            PasswordResetToken,
            r#"
                UPDATE password_reset_tokens
                SET invalidated_at = NOW()
                WHERE id = $1 AND invalidated_at IS NULL
                RETURNING *
            "#,
            token.id
        )
        .fetch_optional(&self.pool)
        .await?;

        if let Some(token) = result {
            Ok(token)
        } else {
            bail!("Token not found or already invalidated")
        }
    }

    pub async fn cleanup_expired(&self) -> Result<()> {
        sqlx::query!(
            r#"
                DELETE FROM password_reset_tokens
                WHERE expires_at < NOW()
            "#
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}