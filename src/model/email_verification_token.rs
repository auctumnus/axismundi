use crate::err::{AppResult, not_found};
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

pub struct EmailVerificationToken {
    pub id: Uuid,
    pub user_id: Uuid,
    pub email: String,
    pub token: String,
    pub invalidated_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

pub struct EmailVerificationTokenRepository {
    pool: PgPool,
}

const NON_CONFUSABLES: &[char] = &[
    '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', 'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'J',
    'K', 'M', 'N', 'P', 'Q', 'R', 'S', 'T', 'U', 'V', 'W', 'X', 'Y',
];

impl EmailVerificationTokenRepository {
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, user_id: Uuid, email: String) -> AppResult<String> {
        let token = nanoid::nanoid!(6, NON_CONFUSABLES);
        let expires_at = Utc::now() + chrono::Duration::days(1);
        // Tokens that require an extra level of security, **such as password reset tokens**,
        // should be hashed with SHA-256.
        // https://thecopenhagenbook.com/server-side-tokens#storing-tokens
        let hashed_token = crate::util::hash(&token)
            .map_err(|_| crate::err::internal_error("token hash failed"))?;

        sqlx::query!(
            r#"
                INSERT INTO email_verification_tokens (user_id, token, expires_at, email)
                VALUES ($1, $2, $3, $4)
            "#,
            user_id,
            hashed_token,
            expires_at,
            email
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(token)
    }

    pub async fn find(
        &self,
        user_id: Uuid,
        email: &str,
        token: &str,
    ) -> AppResult<Option<EmailVerificationToken>> {
        let hashed_token = crate::util::hash(token)
            .map_err(|_| crate::err::internal_error("token hash failed"))?;
        let result = sqlx::query_as!(
            EmailVerificationToken,
            r#"
                SELECT * FROM email_verification_tokens
                WHERE token = $1 AND user_id = $2 AND email = $3 AND invalidated_at IS NULL AND expires_at > NOW()
            "#,
            hashed_token,
            user_id,
            email
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(result)
    }

    pub async fn invalidate(&self, id: Uuid) -> AppResult<EmailVerificationToken> {
        let result = sqlx::query_as!(
            EmailVerificationToken,
            r#"
                UPDATE email_verification_tokens
                SET invalidated_at = NOW()
                WHERE id = $1 AND invalidated_at IS NULL
                RETURNING *
            "#,
            id
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
                DELETE FROM email_verification_tokens
                WHERE expires_at < NOW()
            "#
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}

crate::util::repo_from_parts!(EmailVerificationTokenRepository);
