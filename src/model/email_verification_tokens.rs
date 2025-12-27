use crate::err::{AppResult, not_found};
use crate::util::AppState;
use chrono::{DateTime, Utc};
use uuid::Uuid;

#[allow(dead_code)]
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
    state: AppState,
}

const NON_CONFUSABLES: &[char] = &[
    '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', 'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'J',
    'K', 'M', 'N', 'P', 'Q', 'R', 'S', 'T', 'U', 'V', 'W', 'X', 'Y',
];

impl EmailVerificationTokenRepository {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    /// Creates a verification token within a transaction and returns the record with unhashed token
    pub async fn create(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        user_id: Uuid,
        email: String,
    ) -> AppResult<EmailVerificationToken> {
        let unhashed_token = nanoid::nanoid!(6, NON_CONFUSABLES);
        let expires_at = Utc::now() + chrono::Duration::days(1);
        // Tokens that require an extra level of security, **such as password reset tokens**,
        // should be hashed with SHA-256.
        // https://thecopenhagenbook.com/server-side-tokens#storing-tokens
        let hashed_token = crate::util::stable_hash(&unhashed_token);

        let mut record = sqlx::query_as!(
            EmailVerificationToken,
            r#"
                INSERT INTO email_verification_tokens (user_id, token, expires_at, email)
                VALUES ($1, $2, $3, $4)
                RETURNING *
            "#,
            user_id,
            hashed_token,
            expires_at,
            email
        )
        .fetch_one(&mut **tx)
        .await?;

        // Replace the hashed token with the unhashed one for sending
        record.token = unhashed_token;

        Ok(record)
    }

    /// Sends a verification email with the given token
    pub async fn send(&self, user_id: uuid::Uuid, email: &str, token: &str) -> AppResult<()> {
        self.state
            .email_service
            .send_verification_email(user_id, email, token)
            .await?;
        Ok(())
    }

    pub async fn find(
        &self,
        user_id: Uuid,
        email: &str,
        token: &str,
    ) -> AppResult<Option<EmailVerificationToken>> {
        let hashed_token = crate::util::stable_hash(token);
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
        .fetch_optional(&self.state.pool)
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
        .fetch_optional(&self.state.pool)
        .await?;

        if let Some(token) = result {
            Ok(token)
        } else {
            Err(not_found("Token not found or already invalidated"))
        }
    }

    pub async fn resend(&self, original_token_id: Uuid) -> AppResult<EmailVerificationToken> {
        let mut tx = self.state.pool.begin().await?;

        let original = sqlx::query_as!(
            EmailVerificationToken,
            r#"
                SELECT * FROM email_verification_tokens
                WHERE id = $1 AND invalidated_at IS NULL
            "#,
            original_token_id
        )
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| not_found("token not found or already invalidated"))?;

        sqlx::query!(
            r#"
                UPDATE email_verification_tokens
                SET invalidated_at = NOW()
                WHERE id = $1
            "#,
            original_token_id
        )
        .execute(&mut *tx)
        .await?;

        let new_token = self
            .create(&mut tx, original.user_id, original.email.clone())
            .await?;

        tx.commit().await?;

        self.send(original.user_id, &original.email, &new_token.token)
            .await?;

        Ok(new_token)
    }

    #[allow(dead_code)]
    pub async fn cleanup_expired(&self) -> AppResult<()> {
        sqlx::query!(
            r#"
                DELETE FROM email_verification_tokens
                WHERE expires_at < NOW()
            "#
        )
        .execute(&self.state.pool)
        .await?;

        Ok(())
    }
}

crate::util::repo_from_parts!(EmailVerificationTokenRepository);
