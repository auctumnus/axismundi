use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool, Type};
use uuid::Uuid;
use validator::Validate;

use crate::err::{AppResult, not_found};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type, PartialEq, Eq)]
#[sqlx(type_name = "permission_level", rename_all = "lowercase")]
pub enum PermissionLevel {
    Viewer,
    Editor,
    Admin,
    Owner,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct LanguageInvite {
    pub id: Uuid,
    pub language: Uuid,
    pub sender: Uuid,
    pub recipient: Uuid,
    pub permissions: PermissionLevel,
    pub sent_at: DateTime<Utc>,
    pub accepted_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CreateLanguageInvite {
    pub language: Uuid,
    pub recipient: Uuid,
    pub permissions: PermissionLevel,
}

pub struct LanguageInviteRepository {
    pool: PgPool,
}

impl LanguageInviteRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, invite: CreateLanguageInvite, sender: Uuid) -> AppResult<LanguageInvite> {
        invite.validate()?;

        let result = sqlx::query_as!(
            LanguageInvite,
            r#"
                INSERT INTO language_invites (language, sender, recipient, permissions)
                VALUES ($1, $2, $3, $4)
                RETURNING id, language, sender, recipient, permissions as "permissions: PermissionLevel", sent_at, accepted_at
            "#,
            invite.language,
            sender,
            invite.recipient,
            invite.permissions as PermissionLevel
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(result)
    }

    pub async fn find_by_id(&self, id: Uuid) -> AppResult<LanguageInvite> {
        let result = sqlx::query_as!(
            LanguageInvite,
            r#"SELECT id, language, sender, recipient, permissions as "permissions: PermissionLevel", sent_at, accepted_at FROM language_invites WHERE id = $1"#,
            id
        )
        .fetch_optional(&self.pool)
        .await?;

        result.ok_or_else(|| not_found(format!("language invite with id '{id}'")))
    }

    pub async fn list_by_recipient(&self, recipient: Uuid) -> AppResult<Vec<LanguageInvite>> {
        let result = sqlx::query_as!(
            LanguageInvite,
            r#"SELECT id, language, sender, recipient, permissions as "permissions: PermissionLevel", sent_at, accepted_at FROM language_invites WHERE recipient = $1 AND accepted_at IS NULL ORDER BY sent_at DESC"#,
            recipient
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(result)
    }

    pub async fn list_by_language(&self, language: Uuid) -> AppResult<Vec<LanguageInvite>> {
        let result = sqlx::query_as!(
            LanguageInvite,
            r#"SELECT id, language, sender, recipient, permissions as "permissions: PermissionLevel", sent_at, accepted_at FROM language_invites WHERE language = $1 ORDER BY sent_at DESC"#,
            language
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(result)
    }

    pub async fn accept(&self, id: Uuid) -> AppResult<LanguageInvite> {
        let result = sqlx::query_as!(
            LanguageInvite,
            r#"
                UPDATE language_invites
                SET accepted_at = CURRENT_TIMESTAMP
                WHERE id = $1 AND accepted_at IS NULL
                RETURNING id, language, sender, recipient, permissions as "permissions: PermissionLevel", sent_at, accepted_at
            "#,
            id
        )
        .fetch_optional(&self.pool)
        .await?;

        result.ok_or_else(|| not_found(format!("pending language invite with id '{id}'")))
    }

    pub async fn delete(&self, id: Uuid) -> AppResult<bool> {
        let result = sqlx::query!("DELETE FROM language_invites WHERE id = $1", id)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }
}

crate::util::repo_from_parts!(LanguageInviteRepository);
