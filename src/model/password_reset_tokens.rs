use crate::err::{AppResult, not_found};
use crate::util::{AppState, stable_hash};
use chrono::{DateTime, Utc};
use uuid::Uuid;

#[allow(dead_code)]
pub struct PasswordResetToken {
    pub id: Uuid,
    pub user_id: Uuid,
    pub token: String,
    pub invalidated_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

pub struct PasswordResetTokenRepository {
    state: AppState,
}

impl PasswordResetTokenRepository {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    pub async fn create(&self, user_id: Uuid) -> AppResult<String> {
        let token = nanoid::nanoid!(20);
        let expires_at = Utc::now() + chrono::Duration::days(1);
        // Tokens that require an extra level of security, **such as password reset tokens**,
        // should be hashed with SHA-256.
        // https://thecopenhagenbook.com/server-side-tokens#storing-tokens
        let hashed_token = crate::util::stable_hash(&token);

        sqlx::query_as!(
            PasswordResetToken,
            r#"
                INSERT INTO password_reset_tokens (user_id, token, expires_at)
                VALUES ($1, $2, $3)
                RETURNING *
            "#,
            user_id,
            hashed_token,
            expires_at
        )
        .fetch_one(&self.state.pool)
        .await?;

        Ok(token)
    }

    pub async fn send(&self, user_id: Uuid, email: &str, token: &str) -> AppResult<()> {
        self.state
            .email_service
            .send_password_reset_email(user_id, email, token)
            .await?;
        Ok(())
    }

    pub async fn find_by_token(
        &self,
        user: Uuid,
        token: &str,
    ) -> AppResult<Option<PasswordResetToken>> {
        let hashed_token = stable_hash(token);
        let result = sqlx::query_as!(
            PasswordResetToken,
            r#"
                SELECT * FROM password_reset_tokens
                WHERE token = $1 AND user_id = $2 AND invalidated_at IS NULL AND expires_at > NOW()
            "#,
            hashed_token,
            user
        )
        .fetch_optional(&self.state.pool)
        .await?;

        Ok(result)
    }

    #[allow(dead_code)]
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
        .fetch_optional(&self.state.pool)
        .await?;

        if let Some(token) = result {
            Ok(token)
        } else {
            Err(not_found("Token not found or already invalidated"))
        }
    }

    pub async fn invalidate_with_tx(
        &self,
        token: PasswordResetToken,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ) -> AppResult<PasswordResetToken> {
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
        .fetch_optional(&mut **tx)
        .await?;

        if let Some(token) = result {
            Ok(token)
        } else {
            Err(not_found("Token not found or already invalidated"))
        }
    }

    #[allow(dead_code)]
    pub async fn cleanup_expired(&self) -> AppResult<()> {
        sqlx::query!(
            r#"
                DELETE FROM password_reset_tokens
                WHERE expires_at < NOW()
            "#
        )
        .execute(&self.state.pool)
        .await?;

        Ok(())
    }
}

crate::util::repo_from_parts!(PasswordResetTokenRepository);
