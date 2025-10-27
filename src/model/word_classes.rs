use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use validator::Validate;

use crate::{
    err::{AppResult, bad_request, not_found},
    model::{language_invites::PermissionLevel, users::User},
    pagination::{PaginatedRequest, PaginatedResponse},
    util::{AppState, ensure_verified},
};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct WordClass {
    pub id: Uuid,
    pub language: Uuid,
    pub name: String,
    pub abbreviation: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub created_by: Uuid,
    pub updated_by: Uuid,
    pub bookmark: String,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CreateWordClass {
    #[validate(length(min = 1, max = 50))]
    pub name: String,

    #[validate(length(min = 1, max = 10))]
    pub abbreviation: String,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct UpdateWordClass {
    #[validate(length(min = 1, max = 50))]
    pub name: Option<String>,

    #[validate(length(min = 1, max = 10))]
    pub abbreviation: Option<String>,
}

pub struct WordClassRepository {
    state: AppState,
}

impl WordClassRepository {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    pub async fn create(
        &self,
        requestor: &User,
        lang_code: &str,
        word_class: CreateWordClass,
    ) -> AppResult<WordClass> {
        word_class.validate()?;

        ensure_verified(requestor)?;

        let languages = crate::model::languages::LanguageRepository::new(self.state.clone());
        let language = languages.find_by_code(lang_code).await?;

        let permissions = crate::model::language_permissions::LanguagePermissionRepository::new(
            self.state.clone(),
        );
        let user_perm = permissions
            .find_by_user_and_language(requestor.id, language.id)
            .await?;

        let Some(perm) = user_perm else {
            return Err(bad_request(
                "you don't have permission to create word classes",
            ));
        };

        if perm.permission == PermissionLevel::Viewer {
            return Err(bad_request("viewers cannot create word classes"));
        }

        if &word_class.abbreviation == "search" {
            return Err(bad_request("cannot use 'search' as abbreviation"));
        }

        if self
            .name_exists_in_language(language.id, &word_class.name)
            .await?
        {
            return Err(bad_request(
                "word class name already exists in this language",
            ));
        }

        if self
            .abbreviation_exists_in_language(language.id, &word_class.abbreviation)
            .await?
        {
            return Err(bad_request(
                "word class abbreviation already exists in this language",
            ));
        }

        let mut tx = self.state.pool.begin().await?;

        let wc_result = sqlx::query!(
            r#"
                INSERT INTO word_classes (language, name, abbreviation, created_by, updated_by)
                VALUES ($1, $2, $3, $4, $4)
                RETURNING id, language, name, abbreviation, created_by, updated_by, created_at, updated_at
            "#,
            language.id,
            word_class.name,
            word_class.abbreviation,
            requestor.id
        )
        .fetch_one(&mut *tx)
        .await?;

        // Generate and insert bookmark
        let slug = crate::model::bookmarks::BookmarkRepository::generate_slug();
        sqlx::query!(
            "INSERT INTO bookmarks (slug, item, resource) VALUES ($1, $2, 'word_class')",
            slug,
            wc_result.id
        )
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        let result = WordClass {
            id: wc_result.id,
            language: wc_result.language,
            name: wc_result.name,
            abbreviation: wc_result.abbreviation,
            created_at: wc_result.created_at,
            updated_at: wc_result.updated_at,
            created_by: wc_result.created_by,
            updated_by: wc_result.updated_by,
            bookmark: slug,
        };

        Ok(result)
    }

    pub async fn find_by_id(&self, id: Uuid) -> AppResult<WordClass> {
        let result = sqlx::query_as!(
            WordClass,
            r#"
                SELECT
                    word_classes.id,
                    word_classes.language,
                    word_classes.name,
                    word_classes.abbreviation,
                    word_classes.created_at,
                    word_classes.updated_at,
                    word_classes.created_by,
                    word_classes.updated_by,
                    COALESCE(bookmarks.slug, '')::text as "bookmark!"
                FROM word_classes
                LEFT JOIN bookmarks ON bookmarks.item = word_classes.id AND bookmarks.resource = 'word_class'
                WHERE word_classes.id = $1
            "#,
            id
        )
        .fetch_optional(&self.state.pool)
        .await?;

        result.ok_or_else(|| not_found(format!("word class with id '{id}'")))
    }

    pub async fn find_by_abbreviation(
        &self,
        language: Uuid,
        abbreviation: &str,
    ) -> AppResult<WordClass> {
        let result = sqlx::query_as!(
            WordClass,
            r#"
                SELECT
                    word_classes.id,
                    word_classes.language,
                    word_classes.name,
                    word_classes.abbreviation,
                    word_classes.created_at,
                    word_classes.updated_at,
                    word_classes.created_by,
                    word_classes.updated_by,
                    COALESCE(bookmarks.slug, '')::text as "bookmark!"
                FROM word_classes
                LEFT JOIN bookmarks ON bookmarks.item = word_classes.id AND bookmarks.resource = 'word_class'
                WHERE word_classes.language = $1 AND word_classes.abbreviation = $2
            "#,
            language,
            abbreviation
        )
        .fetch_optional(&self.state.pool)
        .await?;

        result.ok_or_else(|| {
            not_found(format!(
                "word class with abbreviation '{}' in language '{}'",
                abbreviation, language
            ))
        })
    }

    pub async fn update(
        &self,
        requestor: &User,
        id: Uuid,
        updates: UpdateWordClass,
    ) -> AppResult<WordClass> {
        updates.validate()?;

        ensure_verified(requestor)?;

        // Get the current word class to check the language
        let current = self.find_by_id(id).await?;

        let permissions = crate::model::language_permissions::LanguagePermissionRepository::new(
            self.state.clone(),
        );
        let user_perm = permissions
            .find_by_user_and_language(requestor.id, current.language)
            .await?;

        let Some(perm) = user_perm else {
            return Err(bad_request(
                "you don't have permission to edit word classes",
            ));
        };

        if perm.permission == PermissionLevel::Viewer {
            return Err(bad_request("viewers cannot edit word classes"));
        }

        if let Some(name) = &updates.name {
            if self.name_exists_in_language(current.language, name).await? {
                return Err(bad_request(
                    "word class name already exists in this language",
                ));
            }
        }

        if let Some(abbreviation) = &updates.abbreviation {
            if self
                .abbreviation_exists_in_language(current.language, abbreviation)
                .await?
            {
                return Err(bad_request(
                    "word class abbreviation already exists in this language",
                ));
            }
        }

        let result = sqlx::query_as!(
            WordClass,
            r#"
                UPDATE word_classes
                SET name = COALESCE($2, name),
                    abbreviation = COALESCE($3, abbreviation),
                    updated_by = $4,
                    updated_at = CURRENT_TIMESTAMP
                WHERE id = $1
                RETURNING word_classes.*, (SELECT bookmarks.slug FROM bookmarks WHERE bookmarks.item = word_classes.id AND bookmarks.resource = 'word_class') as "bookmark!"
            "#,
            id,
            updates.name,
            updates.abbreviation,
            requestor.id
        )
        .fetch_optional(&self.state.pool)
        .await?;

        result.ok_or_else(|| not_found(format!("word class with id '{id}'")))
    }

    pub async fn delete(&self, requestor: &User, id: Uuid) -> AppResult<bool> {
        ensure_verified(requestor)?;

        // Get the current word class to check the language
        let current = self.find_by_id(id).await?;

        let permissions = crate::model::language_permissions::LanguagePermissionRepository::new(
            self.state.clone(),
        );
        let user_perm = permissions
            .find_by_user_and_language(requestor.id, current.language)
            .await?;

        let Some(perm) = user_perm else {
            return Err(bad_request(
                "you don't have permission to delete word classes",
            ));
        };

        if perm.permission == PermissionLevel::Viewer {
            return Err(bad_request("viewers cannot delete word classes"));
        }

        let result = sqlx::query!("DELETE FROM word_classes WHERE id = $1", id)
            .execute(&self.state.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn name_exists_in_language(&self, language: Uuid, name: &str) -> AppResult<bool> {
        let result = sqlx::query!(
            "SELECT 1 as exists FROM word_classes WHERE language = $1 AND name = $2",
            language,
            name
        )
        .fetch_optional(&self.state.pool)
        .await?;

        Ok(result.is_some())
    }

    async fn abbreviation_exists_in_language(
        &self,
        language: Uuid,
        abbreviation: &str,
    ) -> AppResult<bool> {
        let result = sqlx::query!(
            "SELECT 1 as exists FROM word_classes WHERE language = $1 AND abbreviation = $2",
            language,
            abbreviation
        )
        .fetch_optional(&self.state.pool)
        .await?;

        Ok(result.is_some())
    }

    pub async fn search(
        &self,
        language: Uuid,
        pagination: PaginatedRequest,
        search: WordClassSearch,
    ) -> AppResult<PaginatedResponse<WordClass>> {
        use sqlx::QueryBuilder;

        let users = crate::model::users::UserRepository::new(self.state.clone());
        let created_by = match &search.created_by {
            Some(username) => {
                let user = users.find_by_username(username).await?;
                Some(user.id)
            }
            None => None,
        };
        let updated_by = match &search.updated_by {
            Some(username) => {
                let user = users.find_by_username(username).await?;
                Some(user.id)
            }
            None => None,
        };

        let items_future = sqlx::query_as!(
            WordClass,
            r#"
                SELECT
                    word_classes.*,
                    COALESCE(bookmarks.slug, '')::text as "bookmark!"
                FROM word_classes
                LEFT JOIN bookmarks ON bookmarks.item = word_classes.id AND bookmarks.resource = 'word_class'
                WHERE
                word_classes.language = $1
                AND ($3::TIMESTAMPTZ IS NULL OR word_classes.created_at < $3)
                AND ($4::TIMESTAMPTZ IS NULL OR word_classes.created_at > $4)
                AND ($5::UUID IS NULL OR word_classes.created_by = $5)
                AND ($6::UUID IS NULL OR word_classes.updated_by = $6)
                ORDER BY (
                    CASE
                        WHEN $2::TEXT IS NOT NULL AND word_classes.name ILIKE '%' || $2 || '%' THEN 100.0
                        WHEN $2::TEXT IS NOT NULL AND word_classes.abbreviation ILIKE '%' || $2 || '%' THEN 90.0
                        ELSE 0.0
                    END +
                    CASE WHEN $2::TEXT IS NOT NULL THEN
                        similarity(word_classes.name, $2) * 3.0 +
                        COALESCE(similarity(word_classes.abbreviation, $2), 0.0) * 2.0
                    ELSE 0.0
                    END
                ) DESC, word_classes.id
                LIMIT $7
                OFFSET $8
            "#,
            language,
            search.text_query,
            search.created_before,
            search.created_after,
            created_by,
            updated_by,
            pagination.limit as i64,
            pagination.offset as i64,
        )
        .fetch_all(&self.state.pool);

        let count_future = sqlx::query_scalar!(
            r#"
                SELECT COUNT(*)
                FROM word_classes
                WHERE
                language = $1
                AND ($2::TIMESTAMPTZ IS NULL OR created_at < $2)
                AND ($3::TIMESTAMPTZ IS NULL OR created_at > $3)
                AND ($4::UUID IS NULL OR created_by = $4)
                AND ($5::UUID IS NULL OR updated_by = $5)
            "#,
            language,
            search.created_before,
            search.created_after,
            created_by,
            updated_by
        )
        .fetch_one(&self.state.pool);

        let (items, total_count) = tokio::try_join!(items_future, count_future)?;

        let total = total_count.unwrap_or(0);
        let has_more = (i64::from(pagination.offset) + items.len() as i64) < total;

        Ok(PaginatedResponse {
            items,
            total,
            offset: pagination.offset,
            limit: pagination.limit,
            has_more,
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct WordClassSearch {
    pub text_query: Option<String>,
    pub created_before: Option<DateTime<Utc>>,
    pub created_after: Option<DateTime<Utc>>,
    pub created_by: Option<String>,
    pub updated_by: Option<String>,
}

crate::util::repo_from_parts!(WordClassRepository);

#[async_trait::async_trait]
impl crate::model::bookmarks::ResolveBookmark for WordClassRepository {
    async fn resolve_bookmark(&self, item: Uuid, link_type: crate::model::bookmarks::LinkType) -> AppResult<String> {
        // api: /api/languages/{code}/word-classes/{abbreviation}
        // web: /languages/{code}/word-classes/{abbreviation}
        let word_class = self.find_by_id(item).await?;

        let languages = crate::model::languages::LanguageRepository::new(self.state.clone());
        let language = languages.find_by_id(word_class.language).await?;

        let slug = match link_type {
            crate::model::bookmarks::LinkType::Web => format!(
                "/languages/{}/word-classes/{}",
                language.code, word_class.abbreviation
            ),
            crate::model::bookmarks::LinkType::Api => format!(
                "/api/languages/{}/word-classes/{}",
                language.code, word_class.abbreviation
            ),
        };

        Ok(slug)
    }
}
