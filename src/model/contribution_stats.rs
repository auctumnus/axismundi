use uuid::Uuid;

use crate::{err::AppResult, model::{languages::LanguageRepository, users::User}, util::{AppState, repo_from_parts}};

pub struct ContributionStats {
    language_id: Uuid,
    user_id: Uuid,
    word_count: i64,
    translation_count: i64
}

pub struct ContributionStatsRepository {
    state: AppState,
}

impl ContributionStatsRepository {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    pub async fn increment_word_count(&self, language: &Uuid, user: &Uuid,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,) -> AppResult<ContributionStats> {
        sqlx::query_as!(
            ContributionStats,
            r#"
                INSERT INTO contribution_stats (language_id, user_id, word_count, translation_count)
                VALUES ($1, $2, 1, 0)
                ON CONFLICT (language_id, user_id)
                DO UPDATE SET
                word_count = contribution_stats.word_count + EXCLUDED.word_count
                RETURNING language_id, user_id, word_count, translation_count
            "#,
            language,
            user
        )
        .fetch_one(&mut **tx)
        .await
        .map_err(Into::into)
    }

    pub async fn decrement_word_count(&self, language: &Uuid, user: &Uuid,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,) -> AppResult<ContributionStats> {
        sqlx::query_as!(
            ContributionStats,
            r#"
                INSERT INTO contribution_stats (language_id, user_id, word_count, translation_count)
                VALUES ($1, $2, 0, 0)
                ON CONFLICT (language_id, user_id)
                DO UPDATE SET
                word_count = GREATEST(contribution_stats.word_count - 1, 0)
                RETURNING language_id, user_id, word_count, translation_count
            "#,
            language,
            user
        )
        .fetch_one(&mut **tx)
        .await
        .map_err(Into::into)
    }

    pub async fn increment_translation_count(&self, language: &Uuid, user: &Uuid,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,) -> AppResult<ContributionStats> {
        sqlx::query_as!(
            ContributionStats,
            r#"
                INSERT INTO contribution_stats (language_id, user_id, word_count, translation_count)
                VALUES ($1, $2, 1, 0)
                ON CONFLICT (language_id, user_id)
                DO UPDATE SET
                translation_count = contribution_stats.translation_count + EXCLUDED.translation_count
                RETURNING language_id, user_id, word_count, translation_count
            "#,
            language,
            user
        )
        .fetch_one(&mut **tx)
        .await
        .map_err(Into::into)
    }
    
    pub async fn decrement_translation_count(&self, language: &Uuid, user: &Uuid, 
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,) -> AppResult<ContributionStats> {
        sqlx::query_as!(
            ContributionStats,
            r#"
                INSERT INTO contribution_stats (language_id, user_id, word_count, translation_count)
                VALUES ($1, $2, 0, 0)
                ON CONFLICT (language_id, user_id)
                DO UPDATE SET
                translation_count = GREATEST(contribution_stats.translation_count - 1, 0)
                RETURNING language_id, user_id, word_count, translation_count
            "#,
            language,
            user
        )
        .fetch_one(&mut **tx)
        .await
        .map_err(Into::into)
    }

    pub async fn get_top_contributors(&self, language: &Uuid, limit: i64) -> AppResult<Vec<User>> {
        let contributors = sqlx::query_as!(
            User,
            r#"
                SELECT 
                    u.id,
                    u.username,
                    u.email,
                    u.password_hash,
                    u.display_name,
                    u.description,
                    u.pronouns,
                    u.gender,
                    u.profile_picture_object_id,
                    u.verified_at,
                    u.created_at,
                    u.updated_at,
                    COALESCE(bookmarks.slug, '')::text as "bookmark!"
                FROM users u
                JOIN contribution_stats cs ON u.id = cs.user_id
                LEFT JOIN bookmarks ON bookmarks.item = u.id AND bookmarks.resource = 'user'
                WHERE cs.language_id = $1
                ORDER BY (cs.word_count + (10 * cs.translation_count)) DESC
                LIMIT $2
            "#,
            language,
            limit
        )
        .fetch_all(&self.state.pool)
        .await?;

        let owner = LanguageRepository::new(self.state.clone()).find_owner(*language).await?;
        
        let mut c = vec![owner];
        for user in &contributors {
            if(!c.iter().any(|u| u.id == user.id)) {
                c.push(user.clone());
            }
        }

        Ok(c)

        
    }
}

repo_from_parts!(ContributionStatsRepository);