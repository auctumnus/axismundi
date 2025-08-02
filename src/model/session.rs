use anyhow::{bail, Result};
use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use sqlx::{FromRow, PgPool};

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Session {
    #[serde(skip_serializing)]
    pub user_id: i32,
    #[serde(skip_serializing)]
    pub session_token: String,

    pub id: i32,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub invalidated_at: Option<DateTime<Utc>>,
}

pub struct SessionRepository {
    pool: PgPool,
}

const SESSION_LENGTH: Duration = Duration::days(30);

impl SessionRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn login(&self, email: &str, password: &str) -> Result<Option<(String, Session)>> {
        let user_repo = super::user::UserRepository::new(self.pool.clone());
        
        if let Some(user) = user_repo.find_by_email(email).await? {
            if crate::util::verify(password, &user.password_hash)? {
                let token = nanoid::nanoid!();
                let hashed_token = crate::util::stable_hash(&token);
                let expires_at = Utc::now() + SESSION_LENGTH;

                let session = sqlx::query_as!(
                    Session,
                    r#"
                        INSERT INTO user_sessions (user_id, session_token, expires_at)
                        VALUES ($1, $2, $3)
                        RETURNING *
                    "#,
                    user.id,
                    hashed_token,
                    expires_at
                )
                .fetch_one(&self.pool)
                .await?;

                Ok(Some((token, session)))
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    pub async fn find(&self, token: &str) -> Result<Option<Session>> {
        let hashed_token = crate::util::stable_hash(token);
        
        let result = sqlx::query_as!(
            Session,
            r#"
                SELECT * FROM user_sessions
                WHERE session_token = $1 AND invalidated_at IS NULL AND expires_at > NOW()
            "#,
            hashed_token
        )
        .fetch_optional(&self.pool)
        .await?;

        if let Some(session) = result {
            // bump expiration
            let new_expires_at = Utc::now() + SESSION_LENGTH;
            sqlx::query!(
                r#"
                    UPDATE user_sessions
                    SET expires_at = $1
                    WHERE id = $2
                "#,
                new_expires_at,
                session.id
            )
            .execute(&self.pool)
            .await?;

            Ok(Some(session))
        } else {
            Ok(None)
        }
    }

    pub async fn find_by_user_id(&self, user_id: i32) -> Result<Vec<Session>> {
        let result = sqlx::query_as!(
            Session,
            r#"
                SELECT * FROM user_sessions
                WHERE user_id = $1 AND invalidated_at IS NULL AND expires_at > NOW()
            "#,
            user_id
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(result)
    }

    pub async fn invalidate(&self, session: Session) -> Result<()> {
        sqlx::query!(
            r#"
                UPDATE user_sessions
                SET invalidated_at = NOW()
                WHERE id = $1 AND invalidated_at IS NULL
            "#,
            session.id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn invalidate_all(&self, user_id: i32) -> Result<()> {
        sqlx::query!(
            r#"
                UPDATE user_sessions
                SET invalidated_at = NOW()
                WHERE user_id = $1 AND invalidated_at IS NULL
            "#,
            user_id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn cleanup_expired(&self) -> Result<()> {
        sqlx::query!(
            r#"
                DELETE FROM user_sessions
                WHERE expires_at < NOW() OR invalidated_at IS NOT NULL
            "#
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}