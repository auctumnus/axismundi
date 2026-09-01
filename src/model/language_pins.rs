use uuid::Uuid;

use crate::{err::AppResult, model::languages::Language, util::AppState};

pub struct LanguagePinRepository {
    state: AppState,
}

impl LanguagePinRepository {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    pub async fn pin(&self, user_id: Uuid, language_id: Uuid) -> AppResult<()> {
        sqlx::query!(
            r#"
                INSERT INTO user_language_pins (user_id, language_id)
                VALUES ($1, $2)
                ON CONFLICT (user_id, language_id) DO NOTHING
            "#,
            user_id,
            language_id
        )
        .execute(&self.state.pool)
        .await?;

        Ok(())
    }

    pub async fn unpin(&self, user_id: Uuid, language_id: Uuid) -> AppResult<()> {
        sqlx::query!(
            "DELETE FROM user_language_pins WHERE user_id = $1 AND language_id = $2",
            user_id,
            language_id
        )
        .execute(&self.state.pool)
        .await?;

        Ok(())
    }

    pub async fn list_by_user(&self, user_id: Uuid) -> AppResult<Vec<Language>> {
        let languages = sqlx::query_as!(
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
                FROM user_language_pins
                JOIN languages ON languages.id = user_language_pins.language_id
                JOIN language_permissions
                    ON language_permissions.language = languages.id
                    AND language_permissions."user" = user_language_pins.user_id
                    AND language_permissions.permission IN ('editor', 'admin', 'owner')
                LEFT JOIN bookmarks
                    ON bookmarks.item = languages.id AND bookmarks.resource = 'language'
                WHERE user_language_pins.user_id = $1
                ORDER BY user_language_pins.created_at DESC
            "#,
            user_id
        )
        .fetch_all(&self.state.pool)
        .await?;

        Ok(languages)
    }

    pub async fn is_pinned(&self, user_id: Uuid, language_id: Uuid) -> AppResult<bool> {
        let pinned = sqlx::query_scalar!(
            "SELECT EXISTS(SELECT 1 FROM user_language_pins WHERE user_id = $1 AND language_id = $2)",
            user_id,
            language_id
        )
        .fetch_one(&self.state.pool)
        .await?;

        Ok(pinned.unwrap_or(false))
    }
}

crate::util::repo_from_parts!(LanguagePinRepository);
