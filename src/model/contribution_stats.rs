use serde::Deserialize;
use uuid::Uuid;

use crate::{
    err::AppResult,
    model::{
        language_families::LanguageFamilyRepository, language_invites::PermissionLevel,
        languages::LanguageRepository, users::User,
    },
    pagination::{PaginatedRequest, PaginatedResponse},
    util::{AppState, repo_from_parts},
};

pub struct ContributionStats {
    #[allow(dead_code)]
    pub language_id: Uuid,
    #[allow(dead_code)]
    pub user_id: Uuid,
    pub word_count: i64,
    pub translation_count: i64,
}

pub struct ContributionStatsRepository {
    state: AppState,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ContributionsSearch {
    pub q: Option<String>,
    pub permission_level: Option<PermissionLevel>,
}

impl ContributionStatsRepository {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    #[allow(dead_code)]
    pub async fn increment_word_count(
        &self,
        language: &Uuid,
        user: &Uuid,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ) -> AppResult<ContributionStats> {
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

    #[allow(dead_code)]
    pub async fn decrement_word_count(
        &self,
        language: &Uuid,
        user: &Uuid,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ) -> AppResult<ContributionStats> {
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

    #[allow(dead_code)]
    pub async fn increment_translation_count(
        &self,
        language: &Uuid,
        user: &Uuid,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ) -> AppResult<ContributionStats> {
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

    #[allow(dead_code)]
    pub async fn decrement_translation_count(
        &self,
        language: &Uuid,
        user: &Uuid,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ) -> AppResult<ContributionStats> {
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
                    u.tags,
                    u.created_at,
                    u.updated_at,
                    COALESCE(bookmarks.slug, '')::text as "bookmark!"
                FROM users u
                LEFT JOIN contribution_stats cs ON u.id = cs.user_id AND cs.language_id = $1
                LEFT JOIN bookmarks ON bookmarks.item = u.id AND bookmarks.resource = 'user'
                LEFT JOIN language_permissions lp ON lp."user" = u.id AND lp.language = $1
                WHERE cs.language_id = $1 OR lp.permission >= 'editor'
                ORDER BY (COALESCE(cs.word_count, 0) + (10 * COALESCE(cs.translation_count, 0))) DESC
                LIMIT $2
            "#,
            language,
            limit
        )
        .fetch_all(&self.state.pool)
        .await?;

        let owner = LanguageRepository::new(self.state.clone())
            .find_owner(*language)
            .await?;

        let mut c = vec![owner];
        for user in &contributors {
            if !c.iter().any(|u| u.id == user.id) {
                c.push(user.clone());
            }
        }

        Ok(c)
    }

    pub async fn search_top_contributors(
        &self,
        language: &Uuid,
        query: &ContributionsSearch,
        paginated_request: &PaginatedRequest,
    ) -> AppResult<
        PaginatedResponse<(
            User,
            ContributionStats,
            PermissionLevel,
            Option<Uuid>,
            Option<chrono::DateTime<chrono::Utc>>,
        )>,
    > {
        let offset = paginated_request.offset;
        let limit = paginated_request.limit;

        let like_query = match &query.q {
            Some(q) => format!("%{}%", q),
            None => "%".to_string(),
        };

        let min_permission = query.permission_level.unwrap_or(PermissionLevel::Viewer);

        let items_future = sqlx::query!(
            r#"
                SELECT
                    u.id as u_id,
                    u.username as u_username,
                    u.email as u_email,
                    u.password_hash as u_password_hash,
                    u.display_name as u_display_name,
                    u.description as u_description,
                    u.pronouns as u_pronouns,
                    u.gender as u_gender,
                    u.profile_picture_object_id as u_profile_picture_object_id,
                    u.verified_at as u_verified_at,
                    u.tags as u_tags,
                    u.created_at as u_created_at,
                    u.updated_at as u_updated_at,
                    COALESCE(bookmarks.slug, '')::text as "bookmark!",
                    lp.id as "permission_id?",
                    COALESCE(lp.permission, 'viewer') as "permission!: PermissionLevel",
                    lp.accepted_at as "accepted_at?",
                    cs.language_id as "cs_language_id?",
                    cs.user_id as "cs_user_id?",
                    COALESCE(cs.word_count, 0) as "cs_word_count!",
                    COALESCE(cs.translation_count, 0) as "cs_translation_count!"
                FROM users u
                LEFT JOIN contribution_stats cs ON u.id = cs.user_id AND cs.language_id = $1
                LEFT JOIN bookmarks ON bookmarks.item = u.id AND bookmarks.resource = 'user'
                LEFT JOIN language_permissions lp ON lp."user" = u.id AND lp.language = $1 AND lp.accepted_at IS NOT NULL
                WHERE (u.username ILIKE $2 OR u.display_name ILIKE $2)
                AND (lp.permission IS NULL OR lp.permission >= $4)
                AND (cs.word_count IS NOT NULL OR cs.translation_count IS NOT NULL OR lp.permission IS NOT NULL)
                ORDER BY (COALESCE(cs.word_count, 0) + (10 * COALESCE(cs.translation_count, 0))) DESC
                LIMIT $3 OFFSET $5
            "#,
            language,
            like_query,
            i64::from(limit),
            min_permission as PermissionLevel,
            i64::from(offset),
        )
        .fetch_all(&self.state.pool);
        let count_future = sqlx::query_scalar!(
            r#"
                SELECT COUNT(*) as "count!"
                FROM users u
                LEFT JOIN contribution_stats cs ON u.id = cs.user_id AND cs.language_id = $1
                LEFT JOIN language_permissions lp ON lp.user = u.id AND lp.language = $1 AND lp.accepted_at IS NOT NULL
                WHERE (u.username ILIKE $2 OR u.display_name ILIKE $2)
                AND (lp.permission IS NULL OR lp.permission >= $3)

            "#,
            language,
            like_query,
            min_permission as PermissionLevel,
        )
        .fetch_one(&self.state.pool);

        let (items, total_count) = tokio::try_join!(items_future, count_future)?;

        let total = total_count;
        let has_more = (i64::from(paginated_request.offset)
            + i64::try_from(items.len()).unwrap_or(i64::MAX))
            < total;

        let mut results = Vec::new();
        for record in items {
            let user = User {
                id: record.u_id,
                username: record.u_username,
                email: record.u_email,
                password_hash: record.u_password_hash,
                display_name: record.u_display_name,
                description: record.u_description,
                pronouns: record.u_pronouns,
                gender: record.u_gender,
                profile_picture_object_id: record.u_profile_picture_object_id,
                verified_at: record.u_verified_at,
                tags: record.u_tags,
                created_at: record.u_created_at,
                updated_at: record.u_updated_at,
                bookmark: record.bookmark,
            };

            let stats = ContributionStats {
                language_id: record.cs_language_id.unwrap_or(*language),
                user_id: record.cs_user_id.unwrap_or(record.u_id),
                word_count: record.cs_word_count,
                translation_count: record.cs_translation_count,
            };
            results.push((
                user,
                stats,
                record.permission,
                record.permission_id,
                record.accepted_at,
            ));
        }

        Ok(PaginatedResponse {
            items: results,
            total,
            offset,
            limit,
            has_more,
        })
    }

    pub async fn search_top_contributors_for_family(
        &self,
        family_code: &str,
        query: &ContributionsSearch,
        paginated_request: &PaginatedRequest,
    ) -> AppResult<PaginatedResponse<User>> {
        let families = LanguageFamilyRepository::new(self.state.clone());
        let family = families.find_by_code(family_code).await?;

        let offset = paginated_request.offset;
        let limit = paginated_request.limit;

        let like_query = match &query.q {
            Some(q) => format!("%{}%", q),
            None => "%".to_string(),
        };

        let min_permission = query.permission_level.unwrap_or(PermissionLevel::Viewer);

        // get users who have either:
        // 1. contributed to any language in the family, OR
        // 2. have permissions on the family itself
        // filtered by family permission level
        // aggregating contribution stats across all languages in the family
        let items_future = sqlx::query!(
            r#"
                SELECT
                    u.id as u_id,
                    u.username as u_username,
                    u.email as u_email,
                    u.password_hash as u_password_hash,
                    u.display_name as u_display_name,
                    u.description as u_description,
                    u.pronouns as u_pronouns,
                    u.gender as u_gender,
                    u.profile_picture_object_id as u_profile_picture_object_id,
                    u.verified_at as u_verified_at,
                    u.tags as u_tags,
                    u.created_at as u_created_at,
                    u.updated_at as u_updated_at,
                    COALESCE(bookmarks.slug, '')::text as "bookmark!"
                FROM users u
                LEFT JOIN bookmarks ON bookmarks.item = u.id AND bookmarks.resource = 'user'
                LEFT JOIN language_family_permissions lfp ON lfp."user" = u.id AND lfp.family = $1
                LEFT JOIN LATERAL (
                    SELECT
                        SUM(cs.word_count) as total_words,
                        SUM(cs.translation_count) as total_translations
                    FROM contribution_stats cs
                    JOIN language_family_members lfm ON lfm.language_id = cs.language_id
                    WHERE lfm.family_id = $1 AND cs.user_id = u.id
                ) stats ON true
                WHERE (u.username ILIKE $2 OR u.display_name ILIKE $2)
                AND (lfp.permission IS NULL OR lfp.permission >= $4)
                AND (COALESCE(stats.total_words, 0) > 0 OR COALESCE(stats.total_translations, 0) > 0 OR lfp.permission IS NOT NULL)
                ORDER BY (COALESCE(stats.total_words, 0) + (10 * COALESCE(stats.total_translations, 0))) DESC
                LIMIT $3 OFFSET $5
            "#,
            family.id,
            like_query,
            i64::from(limit),
            min_permission as PermissionLevel,
            i64::from(offset),
        )
        .fetch_all(&self.state.pool);

        let count_future = sqlx::query_scalar!(
            r#"
                SELECT COUNT(*) as "count!"
                FROM users u
                LEFT JOIN language_family_permissions lfp ON lfp."user" = u.id AND lfp.family = $1
                LEFT JOIN LATERAL (
                    SELECT
                        SUM(cs.word_count) as total_words,
                        SUM(cs.translation_count) as total_translations
                    FROM contribution_stats cs
                    JOIN language_family_members lfm ON lfm.language_id = cs.language_id
                    WHERE lfm.family_id = $1 AND cs.user_id = u.id
                ) stats ON true
                WHERE (u.username ILIKE $2 OR u.display_name ILIKE $2)
                AND (lfp.permission IS NULL OR lfp.permission >= $3)
                AND (COALESCE(stats.total_words, 0) > 0 OR COALESCE(stats.total_translations, 0) > 0 OR lfp.permission IS NOT NULL)
            "#,
            family.id,
            like_query,
            min_permission as PermissionLevel,
        )
        .fetch_one(&self.state.pool);

        let (items, total_count) = tokio::try_join!(items_future, count_future)?;

        let total = total_count;
        let has_more = (i64::from(paginated_request.offset)
            + i64::try_from(items.len()).unwrap_or(i64::MAX))
            < total;

        let mut results = Vec::new();
        for record in items {
            let user = User {
                id: record.u_id,
                username: record.u_username,
                email: record.u_email,
                password_hash: record.u_password_hash,
                display_name: record.u_display_name,
                description: record.u_description,
                pronouns: record.u_pronouns,
                gender: record.u_gender,
                profile_picture_object_id: record.u_profile_picture_object_id,
                verified_at: record.u_verified_at,
                tags: record.u_tags,
                created_at: record.u_created_at,
                updated_at: record.u_updated_at,
                bookmark: record.bookmark,
            };
            results.push(user);
        }

        Ok(PaginatedResponse {
            items: results,
            total,
            offset,
            limit,
            has_more,
        })
    }
}

repo_from_parts!(ContributionStatsRepository);
