use crate::err::{AppResult, not_found};
use chrono::{DateTime, Utc};
use sqlx::PgPool;

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
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, user_id: i32) -> AppResult<String> {
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

    pub async fn find_by_token(&self, token: &str) -> AppResult<Option<PasswordResetToken>> {
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

    pub async fn invalidate(&self, token: PasswordResetToken) -> AppResult<PasswordResetToken> {
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
            Err(not_found("Token not found or already invalidated"))
        }
    }

    pub async fn cleanup_expired(&self) -> AppResult<()> {
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

crate::util::repo_from_parts!(PasswordResetTokenRepository);
