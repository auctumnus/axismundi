use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use validator::Validate;

use crate::{
    err::{AppResult, bad_request, forbidden, not_found},
    model::{
        language_invites::PermissionLevel,
        users::{User, UserSearch},
    },
    pagination::{PaginatedRequest, PaginatedResponse},
    util::{AppState, ensure_verified},
};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Language {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    pub description: String,
    pub private: bool,
    pub like_count: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub created_by: Uuid,
    pub updated_by: Uuid,
    pub bookmark: String,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CreateLanguage {
    #[validate(length(min = 2, max = 10))]
    pub code: String,

    #[validate(length(min = 2, max = 100))]
    pub name: String,

    #[serde(default)] // i.e. false
    pub private: bool,

    #[serde(default)]
    #[validate(length(max = 5000))]
    pub description: String,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct UpdateLanguage {
    #[validate(length(min = 2, max = 10))]
    pub code: Option<String>,

    #[serde(default)]
    pub private: Option<bool>,

    #[validate(length(min = 2, max = 100))]
    pub name: Option<String>,

    #[validate(length(max = 5000))]
    pub description: Option<String>,
}

pub struct LanguageRepository {
    state: AppState,
}

impl LanguageRepository {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    pub async fn render_description(&self, language: &Language) -> AppResult<String> {
        let rendered = crate::md::render_md(&language.description)?;
        Ok(rendered)
    }

    pub async fn find_by_id(&self, id: Uuid) -> AppResult<Language> {
        let result = sqlx::query_as!(
            Language,
            r#"
                SELECT
                    languages.id,
                    languages.code,
                    languages.name,
                    languages.description,
                    languages.private,
                    languages.like_count,
                    languages.created_at,
                    languages.updated_at,
                    languages.created_by,
                    languages.updated_by,
                    COALESCE(bookmarks.slug, '')::text as "bookmark!"
                FROM languages
                LEFT JOIN bookmarks ON bookmarks.item = languages.id AND bookmarks.resource = 'language'
                WHERE languages.id = $1
            "#,
            id
        )
        .fetch_optional(&self.state.pool)
        .await?;

        result.ok_or_else(|| not_found(format!("language with id '{id}'")))
    }

    pub async fn create(&self, requestor: &User, language: CreateLanguage) -> AppResult<Language> {
        language.validate()?;

        ensure_verified(requestor)?;

        if self.code_exists(&language.code).await? {
            return Err(bad_request("language code is already in use"));
        }

        if &language.code == "search" {
            return Err(bad_request("cannot use 'search' as language code"));
        }

        let mut tx = self.state.pool.begin().await?;

        let lang_result = sqlx::query!(
            r#"
                INSERT INTO languages (code, name, private, description, created_by, updated_by)
                VALUES ($1, $2, $3, $4, $5, $5)
                RETURNING id, code, name, private, description, created_by, updated_by, created_at, updated_at
            "#,
            language.code,
            language.name,
            language.private,
            language.description,
            requestor.id
        )
        .fetch_one(&mut *tx)
        .await?;

        // Generate and insert bookmark
        let slug = crate::model::bookmarks::BookmarkRepository::generate_slug();
        sqlx::query!(
            "INSERT INTO bookmarks (slug, item, resource) VALUES ($1, $2, 'language')",
            slug,
            lang_result.id
        )
        .execute(&mut *tx)
        .await?;

        let result = Language {
            id: lang_result.id,
            code: lang_result.code,
            name: lang_result.name,
            like_count: 0,
            description: lang_result.description,
            private: lang_result.private,
            created_at: lang_result.created_at,
            updated_at: lang_result.updated_at,
            created_by: lang_result.created_by,
            updated_by: lang_result.updated_by,
            bookmark: slug,
        };

        crate::model::language_permissions::LanguagePermissionRepository::new(self.state.clone())
            .create_by_tx(
                &mut tx,
                crate::model::language_permissions::CreateLanguagePermission {
                    language: result.id,
                    user: requestor.id,
                    permission: PermissionLevel::Owner,
                    via: None,
                },
                requestor.id,
            )
            .await?;

        tx.commit().await?;

        // Create activity if language is public
        if !result.private {
            let activity_repo = crate::model::user_activities::UserActivityRepository::new(self.state.clone());
            let _activity = activity_repo.create(
                requestor.id,
                crate::model::user_activities::ActivityType::CreateLanguage,
                result.id,
                "language",
                None,
                None,
            ).await?;
        }

        Ok(result)
    }

    pub async fn find_by_code(&self, code: &str) -> AppResult<Language> {
        let result = sqlx::query_as!(
            Language,
            r#"
                SELECT
                    languages.id,
                    languages.code,
                    languages.name,
                    languages.description,
                    languages.private,
                    languages.like_count,
                    languages.created_at,
                    languages.updated_at,
                    languages.created_by,
                    languages.updated_by,
                    COALESCE(bookmarks.slug, '')::text as "bookmark!"
                FROM languages
                LEFT JOIN bookmarks ON bookmarks.item = languages.id AND bookmarks.resource = 'language'
                WHERE languages.code = $1
            "#,
            code
        )
        .fetch_optional(&self.state.pool)
        .await?;

        result.ok_or_else(|| not_found(format!("language with code '{code}'")))
    }

    pub async fn update(
        &self,
        requestor: &User,
        id: Uuid,
        updates: UpdateLanguage,
    ) -> AppResult<Language> {
        updates.validate()?;

        ensure_verified(requestor)?;

        let permissions = crate::model::language_permissions::LanguagePermissionRepository::new(
            self.state.clone(),
        );
        let user_perm = permissions
            .find_by_user_and_language(requestor.id, id)
            .await?;
        let Some(perm) = user_perm else {
            return Err(forbidden("you don't have permission to edit this language"));
        };

        if perm.permission == PermissionLevel::Viewer {
            return Err(forbidden("viewers cannot edit language"));
        }

        if let Some(code) = &updates.code {
            if code == "search" {
                return Err(bad_request("cannot use 'search' as language code"));
            }
        }

        if updates.private.is_some() && perm.permission != PermissionLevel::Owner {
            return Err(forbidden("only owners can change language privacy"));
        }

        if let Some(code) = &updates.code {
            if self.code_exists(code).await? {
                return Err(bad_request("language code is already in use"));
            }
        }

        let result = sqlx::query_as!(
            Language,
            r#"
                UPDATE languages
                SET code = COALESCE($2, code),
                    name = COALESCE($3, name),
                    description = COALESCE($4, description),
                    updated_by = $5,
                    updated_at = CURRENT_TIMESTAMP
                WHERE id = $1
                RETURNING languages.*, (SELECT slug FROM bookmarks WHERE item = languages.id AND resource = 'language') as "bookmark!"
            "#,
            id,
            updates.code,
            updates.name,
            updates.description,
            requestor.id
        )
        .fetch_optional(&self.state.pool)
        .await?;

        let updated_lang = result.ok_or_else(|| not_found(format!("language with id '{id}'")))?;

        // Create activity if language is public
        if !updated_lang.private {
            let activity_repo = crate::model::user_activities::UserActivityRepository::new(self.state.clone());
            let _activity = activity_repo.create(
                requestor.id,
                crate::model::user_activities::ActivityType::UpdateLanguage,
                updated_lang.id,
                "language",
                None,
                None,
            ).await?;
        }

        Ok(updated_lang)
    }

    pub async fn delete(&self, requestor: &User, id: Uuid) -> AppResult<bool> {
        tracing::debug!("Deleting language {id}");
        ensure_verified(requestor)?;

        let permissions = crate::model::language_permissions::LanguagePermissionRepository::new(
            self.state.clone(),
        );
        let user_perm = permissions
            .find_by_user_and_language(requestor.id, id)
            .await?;
        let Some(perm) = user_perm else {
            return Err(forbidden(
                "you don't have permission to delete this language",
            ));
        };
        if perm.permission != PermissionLevel::Owner {
            return Err(forbidden("only owners can delete languages"));
        }

        let result = sqlx::query!("DELETE FROM languages WHERE id = $1", id)
            .execute(&self.state.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn code_exists(&self, code: &str) -> AppResult<bool> {
        let result = sqlx::query!("SELECT 1 as exists FROM languages WHERE code = $1", code)
            .fetch_optional(&self.state.pool)
            .await?;

        Ok(result.is_some())
    }

    pub async fn find_all_by_user(&self, user_id: Uuid) -> AppResult<Vec<Language>> {
        let result = sqlx::query_as!(
            Language,
            r#"
                SELECT
                    languages.id,
                    languages.code,
                    languages.name,
                    languages.description,
                    languages.private,
                    languages.like_count,
                    languages.created_at,
                    languages.updated_at,
                    languages.created_by,
                    languages.updated_by,
                    COALESCE(bookmarks.slug, '')::text as "bookmark!"
                FROM languages
                LEFT JOIN bookmarks ON bookmarks.item = languages.id AND bookmarks.resource = 'language'
                JOIN language_permissions ON language_permissions.language = languages.id
                WHERE language_permissions.user = $1
            "#,
            user_id
        )
        .fetch_all(&self.state.pool)
        .await?;

        Ok(result)
    }

    pub async fn search(
        &self,
        pagination: PaginatedRequest,
        search: LanguageSearch,
    ) -> AppResult<PaginatedResponse<Language>> {
        // search strategy:
        // - exact matches in name, code, owner are weighted highly
        // - otherwise, we use similarity on code, name, description
        // - TODO: deal with private languages and edited_by filter

        let items_future = sqlx::query_as!(
            Language,
            r#"
                SELECT
                    languages.id,
                    languages.code,
                    languages.name,
                    languages.description,
                    languages.private,
                    languages.like_count,
                    languages.created_at,
                    languages.updated_at,
                    languages.created_by,
                    languages.updated_by,
                    COALESCE(bookmarks.slug, '')::text as "bookmark!"
                FROM languages
                JOIN users ON users.id = languages.created_by
                LEFT JOIN bookmarks ON bookmarks.item = languages.id AND bookmarks.resource = 'language'
                WHERE
                ($1::TEXT IS NULL OR users.username = $1)
                AND ($2::TIMESTAMPTZ IS NULL OR languages.created_at < $2)
                AND ($3::TIMESTAMPTZ IS NULL OR languages.created_at > $3)
                ORDER BY (
                    CASE
                        WHEN $4::TEXT IS NOT NULL AND languages.name ILIKE '%' || $4 || '%' THEN 100.0
                        WHEN $4::TEXT IS NOT NULL AND languages.code ILIKE '%' || $4 || '%' THEN 90.0
                        WHEN $4::TEXT IS NOT NULL AND languages.description ILIKE '%' || $4 || '%' THEN 80.0
                        WHEN $4::TEXT IS NOT NULL AND users.username ILIKE '%' || $4 || '%' THEN 70.0
                        ELSE 0.0
                    END +
                    CASE WHEN $4::TEXT IS NOT NULL THEN
                        similarity(languages.name, $4) * 3.0 +
                        similarity(languages.code, $4) * 2.0 +
                        similarity(languages.description, $4) * 1.0
                    ELSE 0.0
                    END
                ) DESC, languages.id
                LIMIT $5
                OFFSET $6
            "#,
            search.owned_by,
            search.created_before,
            search.created_after,
            search.text_query,
            i64::from(pagination.limit),
            i64::from(pagination.offset)
        )
        .fetch_all(&self.state.pool);

        let count_future = sqlx::query_scalar!(
            r#"
                SELECT COUNT(*)
                FROM languages
                JOIN users ON users.id = languages.created_by
                WHERE
                ($1::TEXT IS NULL OR users.username = $1)
                AND ($2::TIMESTAMPTZ IS NULL OR languages.created_at < $2)
                AND ($3::TIMESTAMPTZ IS NULL OR languages.created_at > $3)
            "#,
            search.owned_by,
            search.created_before,
            search.created_after
        )
        .fetch_one(&self.state.pool);

        let (items, total_count) = tokio::try_join!(items_future, count_future)?;

        let total = total_count.unwrap_or(0);
        let has_more = (i64::from(pagination.offset) + i64::try_from(items.len()).unwrap_or(i64::MAX)) < total;

        Ok(PaginatedResponse {
            items,
            total,
            offset: pagination.offset,
            limit: pagination.limit,
            has_more,
        })
    }

    pub async fn search_editors_of_language(
        &self,
        language_id: Uuid,
        pagination: PaginatedRequest,
        search: UserSearch,
    ) -> AppResult<PaginatedResponse<User>> {
        // search strategy:
        // - exact matches in username, display_name are weighted highly
        // - otherwise, we use similarity on username, display_name, description
        // - filter by verified status if specified
        // - must have at least editor permissions (not viewer)

        let items_future = sqlx::query_as!(
            User,
            r#"
                SELECT
                    users.id,
                    users.username,
                    users.email,
                    users.password_hash,
                    users.display_name,
                    users.description,
                    users.pronouns,
                    users.gender,
                    users.profile_picture_object_id,
                    users.verified_at,
                    users.created_at,
                    users.updated_at,
                    COALESCE(bookmarks.slug, '')::text as "bookmark!"
                FROM users
                LEFT JOIN bookmarks ON bookmarks.item = users.id AND bookmarks.resource = 'user'
                JOIN language_permissions ON language_permissions.user = users.id
                WHERE
                language_permissions.language = $1
                AND language_permissions.permission IN ('editor', 'admin', 'owner')
                AND ($2::BOOL IS NULL OR (users.verified_at IS NOT NULL) = $2)
                AND ($3::TIMESTAMPTZ IS NULL OR users.created_at < $3)
                AND ($4::TIMESTAMPTZ IS NULL OR users.created_at > $4)
                ORDER BY (
                    CASE
                        WHEN $5::TEXT IS NOT NULL AND users.username ILIKE '%' || $5 || '%' THEN 100.0
                        WHEN $5::TEXT IS NOT NULL AND users.display_name ILIKE '%' || $5 || '%' THEN 90.0
                        WHEN $5::TEXT IS NOT NULL AND users.description ILIKE '%' || $5 || '%' THEN 80.0
                        ELSE 0.0
                    END +
                    CASE WHEN $5::TEXT IS NOT NULL THEN
                        similarity(users.username, $5) * 3.0 +
                        COALESCE(similarity(users.display_name, $5), 0.0) * 2.0 +
                        COALESCE(similarity(users.description, $5), 0.0) * 1.0
                    ELSE 0.0
                    END
                ) DESC, users.id
                LIMIT $6
                OFFSET $7
            "#,
            language_id,
            search.verified,
            search.created_before,
            search.created_after,
            search.text_query,
            i64::from(pagination.limit),
            i64::from(pagination.offset)
        )
        .fetch_all(&self.state.pool);

        let count_future = sqlx::query_scalar!(
            r#"
                SELECT COUNT(*)
                FROM users
                JOIN language_permissions ON language_permissions.user = users.id
                WHERE
                language_permissions.language = $1
                AND language_permissions.permission IN ('editor', 'owner')
                AND ($2::BOOL IS NULL OR (users.verified_at IS NOT NULL) = $2)
                AND ($3::TIMESTAMPTZ IS NULL OR users.created_at < $3)
                AND ($4::TIMESTAMPTZ IS NULL OR users.created_at > $4)
            "#,
            language_id,
            search.verified,
            search.created_before,
            search.created_after
        )
        .fetch_one(&self.state.pool);

        let (items, total_count) = tokio::try_join!(items_future, count_future)?;

        let total = total_count.unwrap_or(0);
        let has_more = (i64::from(pagination.offset) + i64::try_from(items.len()).unwrap_or(i64::MAX)) < total;

        Ok(PaginatedResponse {
            items,
            total,
            offset: pagination.offset,
            limit: pagination.limit,
            has_more,
        })
    }

    pub async fn find_owner(&self, language_id: Uuid) -> AppResult<User> {
        let result = sqlx::query_as!(
            User,
            r#"
                SELECT
                    users.id,
                    users.username,
                    users.email,
                    users.password_hash,
                    users.display_name,
                    users.description,
                    users.pronouns,
                    users.gender,
                    users.profile_picture_object_id,
                    users.verified_at,
                    users.created_at,
                    users.updated_at,
                    COALESCE(bookmarks.slug, '')::text as "bookmark!"
                FROM users
                LEFT JOIN bookmarks ON bookmarks.item = users.id AND bookmarks.resource = 'user'
                JOIN language_permissions ON language_permissions.user = users.id
                WHERE language_permissions.language = $1
                AND language_permissions.permission = 'owner'
            "#,
            language_id
        )
        .fetch_one(&self.state.pool);

        result.await.map_err(Into::into)
    }

    pub async fn count_contributors(&self, language_id: Uuid) -> AppResult<i64> {
        let result = sqlx::query_scalar!(
            r#"
                SELECT COUNT(DISTINCT user) FROM language_permissions
                WHERE language = $1 AND permission IN ('editor', 'admin')
            "#,
            language_id
        )
        .fetch_one(&self.state.pool).await?;

        Ok(result.unwrap_or(0))
    }

    pub async fn is_liked(&self, language_id: &Uuid, user_id: &Uuid) -> AppResult<bool> {
        let result = sqlx::query!(
            r#"
                SELECT 1 as exists FROM language_likes
                WHERE language_id = $1 AND user_id = $2
            "#,
            language_id,
            user_id
        )
        .fetch_optional(&self.state.pool)
        .await?;

        Ok(result.is_some())
    }

    pub async fn like_language(&self, language_id: Uuid, user_id: Uuid) -> AppResult<Option<i64>> {
        let mut tx = self.state.pool.begin().await?;
        let result = sqlx::query!(
            r#"
                INSERT INTO language_likes (language_id, user_id)
                VALUES ($1, $2)
                ON CONFLICT DO NOTHING
            "#,
            language_id,
            user_id
        )
        .execute(&mut *tx)
        .await?;

        let likes = if result.rows_affected() > 0 {
            let likes = sqlx::query_scalar!(
                r#"
                    UPDATE languages
                    SET like_count = like_count + 1
                    WHERE id = $1
                    RETURNING like_count
                "#,
                language_id
            )
            .fetch_one(&mut *tx)
            .await?;

            Some(likes)
        } else {
            None
        };
        tx.commit().await?;

        Ok(likes)
    }

    pub async fn unlike_language(&self, language_id: Uuid, user_id: Uuid) -> AppResult<Option<i64>> {
        let mut tx = self.state.pool.begin().await?;
        let result = sqlx::query!(
            r#"
                DELETE FROM language_likes
                WHERE language_id = $1 AND user_id = $2
            "#,
            language_id,
            user_id
        )
        .execute(&mut *tx)
        .await?;

        let likes = if result.rows_affected() > 0 {
            let likes = sqlx::query_scalar!(
                r#"
                    UPDATE languages
                    SET like_count = GREATEST(like_count - 1, 0)
                    WHERE id = $1
                    RETURNING like_count
                "#,
                language_id
            )
            .fetch_one(&mut *tx)
            .await?;

            Some(likes)
        } else {
            None
        };

        tx.commit().await?;

        Ok(likes)
    }
}

#[derive(Debug, Default)]
pub struct LanguageSearch {
    pub text_query: Option<String>,
    pub owned_by: Option<String>,
    #[allow(dead_code)]
    pub edited_by: Option<Vec<String>>,
    pub created_before: Option<DateTime<Utc>>,
    pub created_after: Option<DateTime<Utc>>,
}

crate::util::repo_from_parts!(LanguageRepository);

#[async_trait::async_trait]
impl crate::model::bookmarks::ResolveBookmark for LanguageRepository {
    async fn resolve_bookmark(&self, item: Uuid, link_type: crate::model::bookmarks::LinkType) -> AppResult<String> {
        // api: /api/languages/{code}
        // web: /languages/{code}
        let language = self.find_by_id(item).await?;

        let slug = match link_type {
            crate::model::bookmarks::LinkType::Web => format!("/languages/{}", language.code),
            crate::model::bookmarks::LinkType::Api => format!("/api/languages/{}", language.code),
        };

        Ok(slug)
    }
}
