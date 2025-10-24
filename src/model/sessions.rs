use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

use crate::err::{AppError, AppResult};
use crate::util::{AppState, ensure_verified};

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct SessionObj {
    #[serde(skip_serializing)]
    pub user_id: Uuid,
    #[serde(skip_serializing)]
    pub session_token: String,

    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub invalidated_at: Option<DateTime<Utc>>,
}

pub struct SessionRepository {
    state: AppState,
}

const SESSION_LENGTH: Duration = Duration::days(30);

impl SessionRepository {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    pub async fn login(&self, email: &str, password: &str) -> AppResult<(String, SessionObj)> {
        // this is written kind of backwards, but it avoids a timing attack
        // we always check the password, no matter whether the user exists or not,
        // so you can't tell if the email has been used / password is correct
        let user_repo = super::users::UserRepository::new(self.state.clone());

        let result = user_repo.find_by_email(email).await;

        let password_hash = match result {
            Ok(ref user) => user.password_hash.clone(),
            Err(
                e @ AppError {
                    status_code: axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    ..
                },
            ) => return Err(e),
            _ => String::new(),
        };

        if !password_hash.is_empty() && crate::util::verify(password, &password_hash)? {
            if let Ok(user) = result {
                // TODO: should we actually be checking this?
                ensure_verified(&user)?;

                let token = nanoid::nanoid!();
                let hashed_token = crate::util::stable_hash(&token);
                let expires_at = Utc::now() + SESSION_LENGTH;

                let session = sqlx::query_as!(
                    SessionObj,
                    r#"
                        INSERT INTO user_sessions (user_id, session_token, expires_at)
                        VALUES ($1, $2, $3)
                        RETURNING *
                    "#,
                    user.id,
                    hashed_token,
                    expires_at
                )
                .fetch_one(&self.state.pool)
                .await?;

                Ok((token, session))
            } else {
                debug_assert!(
                    false,
                    "User not found after successful password verification"
                );
                Err(AppError::new(
                    "Invalid email or password".to_string(),
                    axum::http::StatusCode::UNAUTHORIZED,
                ))
            }
        } else {
            Err(AppError::new(
                "Invalid email or password".to_string(),
                axum::http::StatusCode::UNAUTHORIZED,
            ))
        }
    }

    pub async fn find(&self, token: &str) -> AppResult<Option<SessionObj>> {
        let hashed_token = crate::util::stable_hash(token);

        let result = sqlx::query_as!(
            SessionObj,
            r#"
                SELECT * FROM user_sessions
                WHERE session_token = $1 AND invalidated_at IS NULL AND expires_at > NOW()
            "#,
            hashed_token
        )
        .fetch_optional(&self.state.pool)
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
            .execute(&self.state.pool)
            .await?;

            Ok(Some(session))
        } else {
            Ok(None)
        }
    }

    pub async fn find_by_user_id(&self, user_id: Uuid) -> AppResult<Vec<SessionObj>> {
        let result = sqlx::query_as!(
            SessionObj,
            r#"
                SELECT * FROM user_sessions
                WHERE user_id = $1 AND invalidated_at IS NULL AND expires_at > NOW()
            "#,
            user_id
        )
        .fetch_all(&self.state.pool)
        .await?;

        Ok(result)
    }

    pub async fn invalidate(&self, session: SessionObj) -> AppResult<()> {
        sqlx::query!(
            r#"
                UPDATE user_sessions
                SET invalidated_at = NOW()
                WHERE id = $1 AND invalidated_at IS NULL
            "#,
            session.id
        )
        .execute(&self.state.pool)
        .await?;

        Ok(())
    }

    pub async fn invalidate_all(&self, user_id: Uuid) -> AppResult<()> {
        sqlx::query!(
            r#"
                UPDATE user_sessions
                SET invalidated_at = NOW()
                WHERE user_id = $1 AND invalidated_at IS NULL
            "#,
            user_id
        )
        .execute(&self.state.pool)
        .await?;

        Ok(())
    }

    pub async fn cleanup_expired(&self) -> AppResult<()> {
        sqlx::query!(
            r#"
                DELETE FROM user_sessions
                WHERE expires_at < NOW() OR invalidated_at IS NOT NULL
            "#
        )
        .execute(&self.state.pool)
        .await?;

        Ok(())
    }
}

crate::util::repo_from_parts!(SessionRepository);
