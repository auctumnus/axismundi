use std::sync::LazyLock;

use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use validator::Validate;

use crate::{
    err::{AppResult, bad_request, not_found},
    model::{language_invites::PermissionLevel, users::User},
    pagination::{PaginatedRequest, PaginatedResponse},
    util::{AppState, ensure_verified, re},
};

/// Allowed in word category and word class abbreviations. Permissive enough
/// for glossing conventions (ADJ, 1SG, v.t., DEF.ART) but strict enough to
/// avoid characters that break URL parsing (/, ?, #, %, \) or are otherwise
/// unsafe in path segments.
pub static ABBREVIATION_REGEX: LazyLock<Regex> = re!(r"^[A-Za-z0-9][A-Za-z0-9._-]*$");

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct WordCategory {
    pub id: Uuid,
    pub language: Uuid,
    pub name: String,
    pub abbreviation: String,
    pub notes: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub created_by: Uuid,
    pub updated_by: Uuid,
    pub bookmark: String,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CreateWordCategory {
    #[validate(length(min = 1, max = 50))]
    pub name: String,

    #[validate(length(min = 1, max = 10), regex(path = ABBREVIATION_REGEX))]
    pub abbreviation: String,

    #[validate(length(max = 10000))]
    pub notes: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct UpdateWordCategory {
    #[validate(length(min = 1, max = 50))]
    pub name: Option<String>,

    #[validate(length(min = 1, max = 10), regex(path = ABBREVIATION_REGEX))]
    pub abbreviation: Option<String>,

    #[validate(length(max = 10000))]
    pub notes: Option<String>,
}

pub struct WordCategoryRepository {
    state: AppState,
}

impl WordCategoryRepository {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    pub async fn create(
        &self,
        requestor: &User,
        lang_code: &str,
        word_category: CreateWordCategory,
    ) -> AppResult<WordCategory> {
        self.create_inner(requestor, lang_code, word_category, false)
            .await
    }

    /// Like `create`, but does not write a per-row audit log entry. Intended
    /// for bulk operations like dictionary import where the caller emits a
    /// single summary audit log afterwards.
    pub async fn create_silent(
        &self,
        requestor: &User,
        lang_code: &str,
        word_category: CreateWordCategory,
    ) -> AppResult<WordCategory> {
        self.create_inner(requestor, lang_code, word_category, true)
            .await
    }

    async fn create_inner(
        &self,
        requestor: &User,
        lang_code: &str,
        word_category: CreateWordCategory,
        silent: bool,
    ) -> AppResult<WordCategory> {
        use crate::model::audit_log::{AuditActionType, AuditableResource, PermissionCheck};
        use crate::model::language_permissions::{
            CheckPermissionReq, LanguagePermissionRepository,
        };

        word_category.validate()?;

        ensure_verified(requestor)?;

        let languages = crate::model::languages::LanguageRepository::new(self.state.clone());
        let language = languages.find_by_code(lang_code).await?;

        if self
            .name_exists_in_language(language.id, &word_category.name)
            .await?
        {
            return Err(bad_request(
                "word category name already exists in this language",
            ));
        }

        if self
            .abbreviation_exists_in_language(language.id, &word_category.abbreviation)
            .await?
        {
            return Err(bad_request(
                "word category abbreviation already exists in this language",
            ));
        }

        let mut tx = self.state.pool.begin().await?;

        let wc_result = sqlx::query!(
            r#"
                INSERT INTO word_categories (language, name, abbreviation, notes, created_by, updated_by)
                VALUES ($1, $2, $3, $4, $5, $5)
                RETURNING id, language, name, abbreviation, notes, created_by, updated_by, created_at, updated_at
            "#,
            language.id,
            word_category.name,
            word_category.abbreviation,
            word_category.notes.unwrap_or_default(),
            requestor.id
        )
        .fetch_one(&mut *tx)
        .await?;

        let permissions = LanguagePermissionRepository::new(self.state.clone());
        if silent {
            let has_perm = permissions
                .has_permission(requestor.id, language.id, PermissionLevel::Editor)
                .await?;
            if !has_perm
                && !crate::util::is_admin_or_mod(&self.state, requestor.id).await?
            {
                return Err(bad_request(
                    "you don't have permission to create word categories",
                ));
            }
        } else {
            let perm_check = permissions
                .check_permission_with_audit(
                    CheckPermissionReq {
                        user: requestor.id,
                        language: language.id,
                        required_level: PermissionLevel::Editor,
                        action_type: AuditActionType::Created,
                        resource_type: AuditableResource::WordCategory,
                        resource_id: wc_result.id,
                        context: Some(serde_json::json!({
                            "name": &word_category.name,
                            "abbreviation": &word_category.abbreviation,
                        })),
                    },
                    &mut tx,
                )
                .await?;

            if perm_check == PermissionCheck::NoPermission {
                return Err(bad_request(
                    "you don't have permission to create word categories",
                ));
            }
        }

        let slug = crate::model::bookmarks::BookmarkRepository::generate_slug();
        sqlx::query!(
            "INSERT INTO bookmarks (slug, item, resource) VALUES ($1, $2, 'word_category')",
            slug,
            wc_result.id
        )
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        let result = WordCategory {
            id: wc_result.id,
            language: wc_result.language,
            name: wc_result.name,
            abbreviation: wc_result.abbreviation,
            notes: wc_result.notes,
            created_at: wc_result.created_at,
            updated_at: wc_result.updated_at,
            created_by: wc_result.created_by,
            updated_by: wc_result.updated_by,
            bookmark: slug,
        };

        Ok(result)
    }

    pub async fn find_by_id(&self, id: Uuid) -> AppResult<WordCategory> {
        let result = sqlx::query_as!(
            WordCategory,
            r#"
                SELECT
                    word_categories.id,
                    word_categories.language,
                    word_categories.name,
                    word_categories.abbreviation,
                    word_categories.notes,
                    word_categories.created_at,
                    word_categories.updated_at,
                    word_categories.created_by,
                    word_categories.updated_by,
                    COALESCE(bookmarks.slug, '')::text as "bookmark!"
                FROM word_categories
                LEFT JOIN bookmarks ON bookmarks.item = word_categories.id AND bookmarks.resource = 'word_category'
                WHERE word_categories.id = $1
            "#,
            id
        )
        .fetch_optional(&self.state.pool)
        .await?;

        result.ok_or_else(|| not_found(format!("word category with id '{id}'")))
    }

    pub async fn list_all(&self, language: Uuid) -> AppResult<Vec<WordCategory>> {
        let result = sqlx::query_as!(
            WordCategory,
            r#"
                SELECT
                    word_categories.id,
                    word_categories.language,
                    word_categories.name,
                    word_categories.abbreviation,
                    word_categories.notes,
                    word_categories.created_at,
                    word_categories.updated_at,
                    word_categories.created_by,
                    word_categories.updated_by,
                    COALESCE(bookmarks.slug, '')::text as "bookmark!"
                FROM word_categories
                LEFT JOIN bookmarks ON bookmarks.item = word_categories.id AND bookmarks.resource = 'word_category'
                WHERE word_categories.language = $1
                ORDER BY word_categories.name
            "#,
            language
        )
        .fetch_all(&self.state.pool)
        .await?;

        Ok(result)
    }

    pub async fn list_by_word(
        &self,
        word_id: Uuid,
        limit: Option<i64>,
    ) -> AppResult<Vec<WordCategory>> {
        let result = sqlx::query_as!(
            WordCategory,
            r#"
                SELECT
                    word_categories.id,
                    word_categories.language,
                    word_categories.name,
                    word_categories.abbreviation,
                    word_categories.notes,
                    word_categories.created_at,
                    word_categories.updated_at,
                    word_categories.created_by,
                    word_categories.updated_by,
                    COALESCE(bookmarks.slug, '')::text as "bookmark!"
                FROM word_categories
                JOIN word_word_categories ON word_word_categories.category = word_categories.id
                LEFT JOIN bookmarks ON bookmarks.item = word_categories.id AND bookmarks.resource = 'word_category'
                WHERE word_word_categories.word = $1
                ORDER BY word_categories.name
                LIMIT $2
            "#,
            word_id,
            limit
        )
        .fetch_all(&self.state.pool)
        .await?;

        Ok(result)
    }

    /// Look up a category by abbreviation; if missing, look up by name (treating the abbreviation
    /// as a candidate name); if still missing, create one with `name = abbreviation = abbreviation`.
    /// Used by the csv importer's auto-create path.
    pub async fn find_or_create_by_abbreviation(
        &self,
        requestor: &User,
        lang_code: &str,
        abbreviation: &str,
    ) -> AppResult<WordCategory> {
        use axum::http::StatusCode;
        let languages = crate::model::languages::LanguageRepository::new(self.state.clone());
        let language = languages.find_by_code(lang_code).await?;

        match self.find_by_abbreviation(&language.id, abbreviation).await {
            Ok(existing) => return Ok(existing),
            Err(e) if e.status_code == StatusCode::NOT_FOUND => {}
            Err(e) => return Err(e),
        }

        if let Some(existing) = self.find_by_name(language.id, abbreviation).await? {
            return Ok(existing);
        }

        self.create_silent(
            requestor,
            lang_code,
            CreateWordCategory {
                name: abbreviation.to_string(),
                abbreviation: abbreviation.to_string(),
                notes: None,
            },
        )
        .await
    }

    async fn find_by_name(&self, language: Uuid, name: &str) -> AppResult<Option<WordCategory>> {
        let result = sqlx::query_as!(
            WordCategory,
            r#"
                SELECT
                    word_categories.id,
                    word_categories.language,
                    word_categories.name,
                    word_categories.abbreviation,
                    word_categories.notes,
                    word_categories.created_at,
                    word_categories.updated_at,
                    word_categories.created_by,
                    word_categories.updated_by,
                    COALESCE(bookmarks.slug, '')::text as "bookmark!"
                FROM word_categories
                LEFT JOIN bookmarks ON bookmarks.item = word_categories.id AND bookmarks.resource = 'word_category'
                WHERE word_categories.language = $1 AND word_categories.name = $2
            "#,
            language,
            name
        )
        .fetch_optional(&self.state.pool)
        .await?;

        Ok(result)
    }

    pub async fn find_by_abbreviation(
        &self,
        language: &Uuid,
        abbreviation: &str,
    ) -> AppResult<WordCategory> {
        let result = sqlx::query_as!(
            WordCategory,
            r#"
                SELECT
                    word_categories.id,
                    word_categories.language,
                    word_categories.name,
                    word_categories.abbreviation,
                    word_categories.notes,
                    word_categories.created_at,
                    word_categories.updated_at,
                    word_categories.created_by,
                    word_categories.updated_by,
                    COALESCE(bookmarks.slug, '')::text as "bookmark!"
                FROM word_categories
                LEFT JOIN bookmarks ON bookmarks.item = word_categories.id AND bookmarks.resource = 'word_category'
                WHERE word_categories.language = $1 AND word_categories.abbreviation = $2
            "#,
            language,
            abbreviation
        )
        .fetch_optional(&self.state.pool)
        .await?;

        result.ok_or_else(|| {
            not_found(format!(
                "word category with abbreviation '{}' in language '{}'",
                abbreviation, language
            ))
        })
    }

    pub async fn update(
        &self,
        requestor: &User,
        id: Uuid,
        updates: UpdateWordCategory,
    ) -> AppResult<WordCategory> {
        use crate::model::audit_log::{AuditActionType, AuditableResource, PermissionCheck};
        use crate::model::language_permissions::{
            CheckPermissionReq, LanguagePermissionRepository,
        };

        updates.validate()?;

        ensure_verified(requestor)?;

        let current = self.find_by_id(id).await?;

        if let Some(name) = &updates.name {
            if name != &current.name && self.name_exists_in_language(current.language, name).await?
            {
                return Err(bad_request(
                    "word category name already exists in this language",
                ));
            }
        }

        if let Some(abbreviation) = &updates.abbreviation {
            if abbreviation != &current.abbreviation
                && self
                    .abbreviation_exists_in_language(current.language, abbreviation)
                    .await?
            {
                return Err(bad_request(
                    "word category abbreviation already exists in this language",
                ));
            }
        }

        let mut tx = self.state.pool.begin().await?;

        let permissions = LanguagePermissionRepository::new(self.state.clone());
        let perm_check = permissions
            .check_permission_with_audit(
                CheckPermissionReq {
                    user: requestor.id,
                    language: current.language,
                    required_level: PermissionLevel::Editor,
                    action_type: AuditActionType::Updated,
                    resource_type: AuditableResource::WordCategory,
                    resource_id: id,
                    context: Some(serde_json::json!({
                        "name": updates.name,
                        "abbreviation": updates.abbreviation,
                    })),
                },
                &mut tx,
            )
            .await?;

        if perm_check == PermissionCheck::NoPermission {
            return Err(bad_request(
                "you don't have permission to edit word categories",
            ));
        }

        let result = sqlx::query_as!(
            WordCategory,
            r#"
                UPDATE word_categories
                SET name = COALESCE($2, name),
                    abbreviation = COALESCE($3, abbreviation),
                    notes = COALESCE($4, notes),
                    updated_by = $5,
                    updated_at = CURRENT_TIMESTAMP
                WHERE id = $1
                RETURNING word_categories.*, (SELECT bookmarks.slug FROM bookmarks WHERE bookmarks.item = word_categories.id AND bookmarks.resource = 'word_category') as "bookmark!"
            "#,
            id,
            updates.name,
            updates.abbreviation,
            updates.notes,
            requestor.id
        )
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| not_found(format!("word category with id '{id}'")))?;

        tx.commit().await?;

        Ok(result)
    }

    pub fn render_notes(word_category: &WordCategory) -> AppResult<String> {
        if word_category.notes.is_empty() {
            Ok(String::new())
        } else {
            Ok(crate::md::render_md(&word_category.notes)?)
        }
    }

    pub async fn delete(&self, requestor: &User, id: Uuid) -> AppResult<bool> {
        use crate::model::audit_log::{AuditActionType, AuditableResource, PermissionCheck};
        use crate::model::language_permissions::{
            CheckPermissionReq, LanguagePermissionRepository,
        };

        ensure_verified(requestor)?;

        let current = self.find_by_id(id).await?;

        let mut tx = self.state.pool.begin().await?;

        let permissions = LanguagePermissionRepository::new(self.state.clone());
        let perm_check = permissions
            .check_permission_with_audit(
                CheckPermissionReq {
                    user: requestor.id,
                    language: current.language,
                    required_level: PermissionLevel::Editor,
                    action_type: AuditActionType::Deleted,
                    resource_type: AuditableResource::WordCategory,
                    resource_id: id,
                    context: Some(serde_json::json!({
                        "name": &current.name,
                        "abbreviation": &current.abbreviation,
                    })),
                },
                &mut tx,
            )
            .await?;

        if perm_check == PermissionCheck::NoPermission {
            return Err(bad_request(
                "you don't have permission to delete word categories",
            ));
        }

        let result = sqlx::query!("DELETE FROM word_categories WHERE id = $1", id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;

        Ok(result.rows_affected() > 0)
    }

    /// Replace the set of categories linked to a word. The caller must already have verified
    /// permission for the language. Runs in the provided transaction.
    pub async fn set_categories_for_word_tx(
        &self,
        word_id: Uuid,
        category_ids: &[Uuid],
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ) -> AppResult<()> {
        sqlx::query!("DELETE FROM word_word_categories WHERE word = $1", word_id)
            .execute(&mut **tx)
            .await?;

        for category_id in category_ids {
            sqlx::query!(
                "INSERT INTO word_word_categories (word, category) VALUES ($1, $2)",
                word_id,
                category_id
            )
            .execute(&mut **tx)
            .await?;
        }

        Ok(())
    }

    /// Resolve abbreviations to category ids in a given language. Returns NOT_FOUND for any
    /// abbreviation that doesn't exist.
    pub async fn resolve_abbreviations(
        &self,
        language: Uuid,
        abbreviations: &[String],
    ) -> AppResult<Vec<Uuid>> {
        let mut ids = Vec::with_capacity(abbreviations.len());
        for abbr in abbreviations {
            let cat = self.find_by_abbreviation(&language, abbr).await?;
            ids.push(cat.id);
        }
        Ok(ids)
    }

    async fn name_exists_in_language(&self, language: Uuid, name: &str) -> AppResult<bool> {
        let result = sqlx::query!(
            "SELECT 1 as exists FROM word_categories WHERE language = $1 AND name = $2",
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
            "SELECT 1 as exists FROM word_categories WHERE language = $1 AND abbreviation = $2",
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
        search: WordCategorySearch,
    ) -> AppResult<PaginatedResponse<WordCategory>> {
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
            WordCategory,
            r#"
                SELECT
                    word_categories.*,
                    COALESCE(bookmarks.slug, '')::text as "bookmark!"
                FROM word_categories
                LEFT JOIN bookmarks ON bookmarks.item = word_categories.id AND bookmarks.resource = 'word_category'
                WHERE
                word_categories.language = $1
                AND ($3::TIMESTAMPTZ IS NULL OR word_categories.created_at < $3)
                AND ($4::TIMESTAMPTZ IS NULL OR word_categories.created_at > $4)
                AND ($5::UUID IS NULL OR word_categories.created_by = $5)
                AND ($6::UUID IS NULL OR word_categories.updated_by = $6)
                ORDER BY (
                    CASE
                        WHEN $2::TEXT IS NOT NULL AND word_categories.name ILIKE '%' || $2 || '%' THEN 100.0
                        WHEN $2::TEXT IS NOT NULL AND word_categories.abbreviation ILIKE '%' || $2 || '%' THEN 90.0
                        ELSE 0.0
                    END +
                    CASE WHEN $2::TEXT IS NOT NULL THEN
                        similarity(word_categories.name, $2) * 3.0 +
                        COALESCE(similarity(word_categories.abbreviation, $2), 0.0) * 2.0
                    ELSE 0.0
                    END
                ) DESC, word_categories.id
                LIMIT $7
                OFFSET $8
            "#,
            language,
            search.text_query,
            search.created_before,
            search.created_after,
            created_by,
            updated_by,
            i64::from(pagination.limit),
            i64::from(pagination.offset),
        )
        .fetch_all(&self.state.pool);

        let count_future = sqlx::query_scalar!(
            r#"
                SELECT COUNT(*)
                FROM word_categories
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
        let has_more =
            (i64::from(pagination.offset) + i64::try_from(items.len()).unwrap_or(i64::MAX)) < total;

        Ok(PaginatedResponse {
            items,
            total,
            offset: pagination.offset,
            limit: pagination.limit,
            has_more,
        })
    }

    pub async fn as_json_ld(
        &self,
        word_category: &WordCategory,
        language: &crate::model::languages::Language,
    ) -> AppResult<serde_json::Value> {
        let user_repo = crate::model::users::UserRepository::new(self.state.clone());
        let creator = user_repo.find_by_id(word_category.created_by).await?;

        let language_repo = crate::model::languages::LanguageRepository::new(self.state.clone());
        let language_ld = language_repo.as_json_ld(language).await?;

        let json_ld = serde_json::json!({
            "@context": "https://schema.org",
            "@type": "DefinedTerm",
            "name": word_category.name,
            "alternateName": word_category.abbreviation,
            "inDefinedTermSet": language_ld,
            "dateCreated": word_category.created_at.to_rfc3339(),
            "dateModified": word_category.updated_at.to_rfc3339(),
            "author": crate::model::users::UserRepository::as_json_ld(&creator),
            "url": format!("{}/languages/{}/word-categories/{}", crate::config::CONFIG.public_url_base, language.code, word_category.abbreviation),
        });

        Ok(json_ld)
    }
}

#[derive(Debug, Deserialize)]
pub struct WordCategorySearch {
    pub text_query: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::util::deserialize_optional_form_datetime"
    )]
    pub created_before: Option<DateTime<Utc>>,
    #[serde(
        default,
        deserialize_with = "crate::util::deserialize_optional_form_datetime"
    )]
    pub created_after: Option<DateTime<Utc>>,
    pub created_by: Option<String>,
    pub updated_by: Option<String>,
}

crate::util::repo_from_parts!(WordCategoryRepository);

#[async_trait::async_trait]
impl crate::model::bookmarks::ResolveBookmark for WordCategoryRepository {
    async fn resolve_bookmark(
        &self,
        item: Uuid,
        link_type: crate::model::bookmarks::LinkType,
    ) -> AppResult<String> {
        let word_category = self.find_by_id(item).await?;

        let languages = crate::model::languages::LanguageRepository::new(self.state.clone());
        let language = languages.find_by_id(word_category.language).await?;

        let slug = match link_type {
            crate::model::bookmarks::LinkType::Web => format!(
                "/languages/{}/word-categories/{}",
                language.code, word_category.abbreviation
            ),
            crate::model::bookmarks::LinkType::Api => format!(
                "/api/languages/{}/word-categories/{}",
                language.code, word_category.abbreviation
            ),
        };

        Ok(slug)
    }
}
