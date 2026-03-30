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
    pub notes: String,
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

    #[validate(length(max = 10000))]
    pub notes: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct UpdateWordClass {
    #[validate(length(min = 1, max = 50))]
    pub name: Option<String>,

    #[validate(length(min = 1, max = 10))]
    pub abbreviation: Option<String>,

    #[validate(length(max = 10000))]
    pub notes: Option<String>,
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
        use crate::model::audit_log::{AuditActionType, AuditableResource, PermissionCheck};
        use crate::model::language_permissions::{
            CheckPermissionReq, LanguagePermissionRepository,
        };

        word_class.validate()?;

        ensure_verified(requestor)?;

        let languages = crate::model::languages::LanguageRepository::new(self.state.clone());
        let language = languages.find_by_code(lang_code).await?;

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
                INSERT INTO word_classes (language, name, abbreviation, notes, created_by, updated_by)
                VALUES ($1, $2, $3, $4, $5, $5)
                RETURNING id, language, name, abbreviation, notes, created_by, updated_by, created_at, updated_at
            "#,
            language.id,
            word_class.name,
            word_class.abbreviation,
            word_class.notes.unwrap_or_default(),
            requestor.id
        )
        .fetch_one(&mut *tx)
        .await?;

        // Check permission with audit
        let permissions = LanguagePermissionRepository::new(self.state.clone());
        let perm_check = permissions
            .check_permission_with_audit(
                CheckPermissionReq {
                    user: requestor.id,
                    language: language.id,
                    required_level: PermissionLevel::Editor,
                    action_type: AuditActionType::Created,
                    resource_type: AuditableResource::WordClass,
                    resource_id: wc_result.id,
                    context: Some(serde_json::json!({
                        "name": &word_class.name,
                        "abbreviation": &word_class.abbreviation,
                    })),
                },
                &mut tx,
            )
            .await?;

        if perm_check == PermissionCheck::NoPermission {
            return Err(bad_request(
                "you don't have permission to create word classes",
            ));
        }

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
            notes: wc_result.notes,
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
                    word_classes.notes,
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

    pub async fn list_all(&self, language: Uuid) -> AppResult<Vec<WordClass>> {
        let result = sqlx::query_as!(
            WordClass,
            r#"
                SELECT
                    word_classes.id,
                    word_classes.language,
                    word_classes.name,
                    word_classes.abbreviation,
                    word_classes.notes,
                    word_classes.created_at,
                    word_classes.updated_at,
                    word_classes.created_by,
                    word_classes.updated_by,
                    COALESCE(bookmarks.slug, '')::text as "bookmark!"
                FROM word_classes
                LEFT JOIN bookmarks ON bookmarks.item = word_classes.id AND bookmarks.resource = 'word_class'
                WHERE word_classes.language = $1
                ORDER BY word_classes.name
            "#,
            language
        )
        .fetch_all(&self.state.pool)
        .await?;

        Ok(result)
    }

    pub async fn find_by_abbreviation(
        &self,
        language: &Uuid,
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
                    word_classes.notes,
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
        use crate::model::audit_log::{AuditActionType, AuditableResource, PermissionCheck};
        use crate::model::language_permissions::{
            CheckPermissionReq, LanguagePermissionRepository,
        };

        updates.validate()?;

        ensure_verified(requestor)?;

        // Get the current word class to check the language
        let current = self.find_by_id(id).await?;

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

        let mut tx = self.state.pool.begin().await?;

        // Check permission with audit
        let permissions = LanguagePermissionRepository::new(self.state.clone());
        let perm_check = permissions
            .check_permission_with_audit(
                CheckPermissionReq {
                    user: requestor.id,
                    language: current.language,
                    required_level: PermissionLevel::Editor,
                    action_type: AuditActionType::Updated,
                    resource_type: AuditableResource::WordClass,
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
                "you don't have permission to edit word classes",
            ));
        }

        let result = sqlx::query_as!(
            WordClass,
            r#"
                UPDATE word_classes
                SET name = COALESCE($2, name),
                    abbreviation = COALESCE($3, abbreviation),
                    notes = COALESCE($4, notes),
                    updated_by = $5,
                    updated_at = CURRENT_TIMESTAMP
                WHERE id = $1
                RETURNING word_classes.*, (SELECT bookmarks.slug FROM bookmarks WHERE bookmarks.item = word_classes.id AND bookmarks.resource = 'word_class') as "bookmark!"
            "#,
            id,
            updates.name,
            updates.abbreviation,
            updates.notes,
            requestor.id
        )
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| not_found(format!("word class with id '{id}'")))?;

        tx.commit().await?;

        Ok(result)
    }

    pub fn render_notes(word_class: &WordClass) -> AppResult<String> {
        if word_class.notes.is_empty() {
            Ok(String::new())
        } else {
            Ok(crate::md::render_md(&word_class.notes)?)
        }
    }

    pub async fn delete(&self, requestor: &User, id: Uuid) -> AppResult<bool> {
        use crate::model::audit_log::{AuditActionType, AuditableResource, PermissionCheck};
        use crate::model::language_permissions::{
            CheckPermissionReq, LanguagePermissionRepository,
        };

        ensure_verified(requestor)?;

        // Get the current word class to check the language
        let current = self.find_by_id(id).await?;

        let mut tx = self.state.pool.begin().await?;

        // Check permission with audit
        let permissions = LanguagePermissionRepository::new(self.state.clone());
        let perm_check = permissions
            .check_permission_with_audit(
                CheckPermissionReq {
                    user: requestor.id,
                    language: current.language,
                    required_level: PermissionLevel::Editor,
                    action_type: AuditActionType::Deleted,
                    resource_type: AuditableResource::WordClass,
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
                "you don't have permission to delete word classes",
            ));
        }

        let result = sqlx::query!("DELETE FROM word_classes WHERE id = $1", id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;

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
            i64::from(pagination.limit),
            i64::from(pagination.offset),
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
        word_class: &WordClass,
        language: &crate::model::languages::Language,
    ) -> AppResult<serde_json::Value> {
        let user_repo = crate::model::users::UserRepository::new(self.state.clone());
        let creator = user_repo.find_by_id(word_class.created_by).await?;

        let language_repo = crate::model::languages::LanguageRepository::new(self.state.clone());
        let language_ld = language_repo.as_json_ld(language).await?;

        let json_ld = serde_json::json!({
            "@context": "https://schema.org",
            "@type": "DefinedTerm",
            "name": word_class.name,
            "alternateName": word_class.abbreviation,
            "inDefinedTermSet": language_ld,
            "dateCreated": word_class.created_at.to_rfc3339(),
            "dateModified": word_class.updated_at.to_rfc3339(),
            "author": crate::model::users::UserRepository::as_json_ld(&creator),
            "url": format!("{}/languages/{}/word-classes/{}", crate::config::CONFIG.public_url_base, language.code, word_class.abbreviation),
        });

        Ok(json_ld)
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
    async fn resolve_bookmark(
        &self,
        item: Uuid,
        link_type: crate::model::bookmarks::LinkType,
    ) -> AppResult<String> {
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
