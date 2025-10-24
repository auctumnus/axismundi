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

        let result = sqlx::query_as!(
            WordClass,
            r#"
                INSERT INTO word_classes (language, name, abbreviation, created_by, updated_by)
                VALUES ($1, $2, $3, $4, $4)
                RETURNING *
            "#,
            language.id,
            word_class.name,
            word_class.abbreviation,
            requestor.id
        )
        .fetch_one(&self.state.pool)
        .await?;

        Ok(result)
    }

    pub async fn find_by_id(&self, id: Uuid) -> AppResult<WordClass> {
        let result = sqlx::query_as!(WordClass, "SELECT * FROM word_classes WHERE id = $1", id)
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
            "SELECT * FROM word_classes WHERE language = $1 AND abbreviation = $2",
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
                RETURNING *
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
        search: WordClassSearch,
    ) -> AppResult<PaginatedResponse<WordClass>> {
        use sqlx::QueryBuilder;

        // Build items query
        let mut items_query: QueryBuilder<sqlx::Postgres> =
            QueryBuilder::new("SELECT * FROM word_classes WHERE language = ");
        items_query.push_bind(language);

        if let Some(ref q) = search.text_query {
            items_query.push(" AND name % ");
            items_query.push_bind(q);
        }

        if let Some(created_before) = search.created_before {
            items_query.push(" AND created_at < ");
            items_query.push_bind(created_before);
        }

        if let Some(created_after) = search.created_after {
            items_query.push(" AND created_at > ");
            items_query.push_bind(created_after);
        }

        items_query.push(" ORDER BY name, id LIMIT ");
        items_query.push_bind(search.pagination.limit);
        items_query.push(" OFFSET ");
        items_query.push_bind(search.pagination.offset);

        // Build count query
        let mut count_query: QueryBuilder<sqlx::Postgres> =
            QueryBuilder::new("SELECT COUNT(*) FROM word_classes WHERE language = ");
        count_query.push_bind(language);

        if let Some(ref q) = search.text_query {
            count_query.push(" AND name % ");
            count_query.push_bind(q);
        }

        if let Some(created_before) = search.created_before {
            count_query.push(" AND created_at < ");
            count_query.push_bind(created_before);
        }

        if let Some(created_after) = search.created_after {
            count_query.push(" AND created_at > ");
            count_query.push_bind(created_after);
        }

        let items_future = items_query
            .build_query_as::<WordClass>()
            .fetch_all(&self.state.pool);

        let count_future = count_query
            .build_query_scalar::<i64>()
            .fetch_one(&self.state.pool);

        let (items, total) = tokio::try_join!(items_future, count_future)?;

        let has_more = (search.pagination.offset as i64 + items.len() as i64) < total;

        Ok(PaginatedResponse {
            items,
            total,
            offset: search.pagination.offset,
            limit: search.pagination.limit,
            has_more,
        })
    }
}

#[derive(Debug)]
pub struct WordClassSearch {
    pub pagination: PaginatedRequest,
    pub text_query: Option<String>,
    pub created_before: Option<DateTime<Utc>>,
    pub created_after: Option<DateTime<Utc>>,
}

crate::util::repo_from_parts!(WordClassRepository);
