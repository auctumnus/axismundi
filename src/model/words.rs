use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sqlx::FromRow;
use uuid::Uuid;
use validator::Validate;

use crate::{
    err::{AppResult, bad_request, internal_error, not_found},
    model::{language_invites::PermissionLevel, users::User},
    pagination::{PaginatedRequest, PaginatedResponse},
    util::{AppState, ensure_verified},
};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[allow(clippy::struct_field_names)]
pub struct Word {
    #[serde(skip_serializing)]
    pub id: Uuid,
    #[serde(skip_serializing)]
    pub language: Uuid,
    #[serde(skip_serializing)]
    #[allow(dead_code)]
    pub word_class: Option<Uuid>,
    #[serde(skip_serializing)]
    pub cognacy: Option<Uuid>,
    pub word: String,
    pub slug: String,
    pub lemma: i32,
    pub ipa: Option<String>,
    pub notes: Option<String>,
    pub extra: Option<JsonValue>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing)]
    pub _created_by: Uuid,
    #[serde(skip_serializing)]
    pub _updated_by: Uuid,

    // materialized
    pub bookmark: String,
    pub language_code: Option<String>,
    pub created_by: Option<String>,
    pub updated_by: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CreateWord {
    #[validate(length(min = 1, max = 200))]
    pub word: String,

    #[validate(length(min = 1, max = 10))]
    pub word_class: String,

    #[validate(length(max = 200))]
    pub ipa: Option<String>,

    #[validate(length(max = 10000))]
    pub notes: Option<String>,

    pub extra: Option<JsonValue>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct UpdateWord {
    #[validate(length(min = 1, max = 200))]
    pub word: Option<String>,

    #[validate(length(min = 1, max = 10))]
    pub word_class: Option<String>,

    #[validate(length(max = 200))]
    pub ipa: Option<String>,

    #[validate(length(max = 10000))]
    pub notes: Option<String>,

    pub extra: Option<JsonValue>,
}

pub struct WordRepository {
    state: AppState,
}

fn nfkc(input: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    input.nfkc().collect()
}

impl WordRepository {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    #[allow(dead_code)]
    pub fn make_slug(word: &str) -> String {
        nfkc(word)
    }

    pub async fn make_slug_and_lemma(&self, language: Uuid, word: &str) -> AppResult<(String, i32)> {
        let slug = nfkc(word);
        let number_slugs = sqlx::query_scalar!(
            r#"
                SELECT COUNT(*) FROM words
                WHERE language = $1 AND slug = $2
            "#,
            language,
            slug
        )
        .fetch_one(&self.state.pool)
        .await?;

        let lemma = i32::try_from(number_slugs.unwrap_or(0))
            .map_err(|_| internal_error("lemma count overflow"))?
            .checked_add(1)
            .ok_or_else(|| internal_error("lemma count overflow"))?;
        Ok((slug, lemma))
    }

    pub async fn create(
        &self,
        requestor: &User,
        language: Uuid,
        word: CreateWord,
    ) -> AppResult<Word> {
        word.validate()?;

        ensure_verified(requestor)?;

        let permissions = crate::model::language_permissions::LanguagePermissionRepository::new(
            self.state.clone(),
        );
        let user_perm = permissions
            .find_by_user_and_language(requestor.id, language)
            .await?;

        let Some(perm) = user_perm else {
            return Err(bad_request("you don't have permission to create words"));
        };

        if perm.permission == PermissionLevel::Viewer {
            return Err(bad_request("viewers cannot create words"));
        }

        let word_classes = crate::model::word_classes::WordClassRepository::new(self.state.clone());
        let word_class = word_classes
            .find_by_abbreviation(language, &word.word_class)
            .await?;

        let (slug, lemma) = self.make_slug_and_lemma(language, &word.word).await?;

        let mut tx = self.state.pool.begin().await?;

        let word_result = sqlx::query!(
            r#"
                INSERT INTO words (language, word_class, word, slug, lemma, ipa, notes, extra, created_by, updated_by)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $9)
                RETURNING words.*
            "#,
            language,
            word_class.id,
            word.word,
            slug,
            lemma,
            word.ipa,
            word.notes,
            word.extra,
            requestor.id
        )
        .fetch_one(&mut *tx)
        .await?;

        // Generate and insert bookmark
        let bookmark_slug = crate::model::bookmarks::BookmarkRepository::generate_slug();
        sqlx::query!(
            "INSERT INTO bookmarks (slug, item, resource) VALUES ($1, $2, 'lemma')",
            bookmark_slug,
            word_result.id
        )
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        // fetch the created word to get materialized fields
        let created_word = self.find_by_id(word_result.id).await?;

        // Create activity if language is public
        let lang_repo = crate::model::languages::LanguageRepository::new(self.state.clone());
        let lang = lang_repo.find_by_id(language).await?;
        if !lang.private {
            let activity_repo = crate::model::user_activities::UserActivityRepository::new(self.state.clone());
            let _activity = activity_repo.create(
                requestor.id,
                crate::model::user_activities::ActivityType::CreateWord,
                created_word.id,
                "word",
                Some(language),
                Some("language"),
            ).await?;
        }

        Ok(created_word)
    }

    pub async fn find_by_id(&self, id: Uuid) -> AppResult<Word> {
        let result = sqlx::query_as!(
            Word,
            r#"
                SELECT
                    words.id,
                    words.language,
                    words.word_class,
                    words.cognacy,
                    words.word,
                    words.slug,
                    words.lemma,
                    words.ipa,
                    words.notes,
                    words.extra,
                    words.created_at,
                    words.updated_at,
                    words.created_by as "_created_by!",
                    words.updated_by as "_updated_by!",
                    COALESCE(bookmarks.slug, '')::text as "bookmark!",
                    languages.code as language_code,
                    created.username as created_by,
                    updated.username as updated_by
                FROM words
                LEFT JOIN bookmarks ON bookmarks.item = words.id AND bookmarks.resource = 'lemma'
                LEFT JOIN languages ON languages.id = words.language
                LEFT JOIN users AS created ON created.id = words.created_by
                LEFT JOIN users AS updated ON updated.id = words.updated_by
                WHERE words.id = $1
            "#,
            id
        )
        .fetch_optional(&self.state.pool)
        .await?;

        result.ok_or_else(|| not_found(format!("word with id '{id}'")))
    }

    pub async fn find_by_slug_and_lemma(
        &self,
        _requestor: Option<&User>,
        language: Uuid,
        slug: &str,
        lemma: i32,
    ) -> AppResult<Word> {
        let result = sqlx::query_as!(
            Word,
            r#"
                SELECT
                    words.id,
                    words.language,
                    words.word_class,
                    words.cognacy,
                    words.word,
                    words.slug,
                    words.lemma,
                    words.ipa,
                    words.notes,
                    words.extra,
                    words.created_at,
                    words.updated_at,
                    words.created_by as "_created_by!",
                    words.updated_by as "_updated_by!",
                    COALESCE(bookmarks.slug, '')::text as "bookmark!",
                    languages.code as language_code,
                    created.username as created_by,
                    updated.username as updated_by
                FROM words
                LEFT JOIN bookmarks ON bookmarks.item = words.id AND bookmarks.resource = 'lemma'
                LEFT JOIN languages ON languages.id = words.language
                LEFT JOIN users AS created ON created.id = words.created_by
                LEFT JOIN users AS updated ON updated.id = words.updated_by
                WHERE words.slug = $1 AND words.lemma = $2 AND words.language = $3
            "#,
            slug,
            lemma,
            language
        )
        .fetch_optional(&self.state.pool)
        .await?;

        result.ok_or_else(|| not_found(format!("word with slug '{slug}' and lemma '{lemma}'")))
    }

    pub async fn update_by_lemma(&self, requestor: &User, language: Uuid, slug: &str, lemma: i32, updates: UpdateWord) -> AppResult<Word> {
        updates.validate()?;

        ensure_verified(requestor)?;

        // get the word to find its language
        let word = self.find_by_slug_and_lemma(Some(requestor), language, slug, lemma).await?;

        let permissions = crate::model::language_permissions::LanguagePermissionRepository::new(
            self.state.clone(),
        );
        let user_perm = permissions
            .find_by_user_and_language(requestor.id, word.language)
            .await?;

        let Some(perm) = user_perm else {
            return Err(bad_request("you don't have permission to edit words"));
        };

        if perm.permission == PermissionLevel::Viewer {
            return Err(bad_request("viewers cannot edit words"));
        }

        let word_classes = crate::model::word_classes::WordClassRepository::new(self.state.clone());

        let word_class = if let Some(ref abbreviation) = updates.word_class {
            Some(
                word_classes
                    .find_by_abbreviation(word.language, abbreviation)
                    .await?
                    .id,
            )
        } else {
            None
        };

        let mut tx = self.state.pool.begin().await?;

        // if slug is the same, we don't need to update the lemma
        // buuut if the slug is changing, we need to change the lemmata of all words with the same slug
        let (slug, lemma) = if let Some(w) = updates.word.as_ref() {
            let original_lemma = word.lemma;
            let (new_slug, lemma) = self.make_slug_and_lemma(word.language, w).await?;
            sqlx::query!(
                r#"
                    UPDATE words
                    SET lemma = lemma - 1
                    WHERE language = $1 AND slug = $2 AND lemma > $3
                "#,
                word.language,
                word.slug,
                original_lemma
            )
            .execute(&mut *tx)
            .await?;

            (Some(new_slug), Some(lemma))
        } else {
            (None, None)
        };


        let result = sqlx::query_as!(
            Word,
            r#"
                UPDATE words
                SET word_class = COALESCE($2, word_class),
                    word = COALESCE($3, word),
                    slug = COALESCE($4, slug),
                    ipa = COALESCE($5, ipa),
                    notes = COALESCE($6, notes),
                    extra = COALESCE($7, extra),
                    updated_by = $8,
                    updated_at = CURRENT_TIMESTAMP,
                    lemma = COALESCE($9, lemma)
                WHERE id = $1
                RETURNING
                    words.id,
                    words.language,
                    words.word_class,
                    words.cognacy,
                    words.word,
                    words.slug,
                    words.lemma,
                    words.ipa,
                    words.notes,
                    words.extra,
                    words.created_at,
                    words.updated_at,
                    words.created_by as "_created_by!",
                    words.updated_by as "_updated_by!",
                    (SELECT slug FROM bookmarks WHERE item = words.id AND resource = 'lemma') as "bookmark!",
                    (SELECT code FROM languages WHERE id = words.language) as language_code,
                    (SELECT username FROM users WHERE id = words.created_by) as created_by,
                    (SELECT username FROM users WHERE id = words.updated_by) as updated_by
            "#,
            word.id,
            word_class,
            updates.word,
            slug,
            updates.ipa,
            updates.notes,
            updates.extra,
            requestor.id,
            lemma,
        )
        .fetch_optional(&mut *tx)
        .await?;

        tx.commit().await?;

        let updated_word = result.ok_or_else(|| not_found(format!("word with id '{}'", word.id)))?;

        // Create activity if language is public
        let lang_repo = crate::model::languages::LanguageRepository::new(self.state.clone());
        let lang = lang_repo.find_by_id(word.language).await?;
        if !lang.private {
            let activity_repo = crate::model::user_activities::UserActivityRepository::new(self.state.clone());
            let _activity = activity_repo.create(
                requestor.id,
                crate::model::user_activities::ActivityType::UpdateWord,
                updated_word.id,
                "word",
                Some(word.language),
                Some("language"),
            ).await?;
        }

        Ok(updated_word)
    }

    pub async fn delete_by_lemma(&self, requestor: &User, language: Uuid, slug: &str, lemma: i32) -> AppResult<bool> {
        ensure_verified(requestor)?;

        // get the word to find its language
        let word = self.find_by_slug_and_lemma(Some(requestor), language, slug, lemma).await?;

        let permissions = crate::model::language_permissions::LanguagePermissionRepository::new(
            self.state.clone(),
        );
        let user_perm = permissions
            .find_by_user_and_language(requestor.id, word.language)
            .await?;

        let Some(perm) = user_perm else {
            return Err(bad_request("you don't have permission to delete words"));
        };

        if perm.permission == PermissionLevel::Viewer {
            return Err(bad_request("viewers cannot delete words"));
        }

        let result = sqlx::query!("DELETE FROM words WHERE id = $1", word.id)
            .execute(&self.state.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    #[allow(clippy::too_many_lines)]
    pub async fn search(
        &self,
        language: Uuid,
        pagination: PaginatedRequest,
        search: WordSearch,
    ) -> AppResult<PaginatedResponse<Word>> {
        // search strategy:
        // - exact matches in word, then definition, then notes
        // - trigram similarity

        let word_class = if let Some(ref abbreviation) = search.word_class {
            Some(
                crate::model::word_classes::WordClassRepository::new(self.state.clone())
                    .find_by_abbreviation(language, abbreviation)
                    .await?
                    .id,
            )
        } else {
            None
        };

        let items_future = sqlx::query_as!(
            Word,
            r#"
                SELECT
                    words.id,
                    words.language,
                    words.word_class,
                    words.cognacy,
                    words.word,
                    words.slug,
                    words.lemma,
                    words.ipa,
                    words.notes,
                    words.extra,
                    words.created_at,
                    words.updated_at,
                    words.created_by as "_created_by!",
                    words.updated_by as "_updated_by!",
                    COALESCE(bookmarks.slug, '')::text as "bookmark!",
                    languages.code as language_code,
                    created.username as created_by,
                    updated.username as updated_by
                FROM words
                LEFT JOIN bookmarks ON bookmarks.item = words.id AND bookmarks.resource = 'lemma'
                LEFT JOIN languages ON languages.id = words.language
                LEFT JOIN users AS created ON created.id = words.created_by
                LEFT JOIN users AS updated ON updated.id = words.updated_by
                WHERE
                words.language = $1
                AND ($2::UUID IS NULL OR words.word_class = $2)
                AND ($3::TIMESTAMPTZ IS NULL OR words.created_at < $3)
                AND ($4::TIMESTAMPTZ IS NULL OR words.created_at > $4)
                AND ($5::TEXT IS NULL OR words.slug = $5)
                ORDER BY (
                    CASE
                        WHEN $6::TEXT IS NOT NULL AND words.word ILIKE '%' || $6 || '%' THEN 100.0
                        WHEN $6::TEXT IS NOT NULL AND words.notes ILIKE '%' || $6 || '%' THEN 80.0
                        ELSE 0.0
                    END +
                    CASE WHEN $6::TEXT IS NOT NULL THEN
                        similarity(words.word, $6) * 3.0 +
                        COALESCE(similarity(words.notes, $6), 0.0) * 1.0
                    ELSE 0.0
                    END
                ) DESC, words.id
                LIMIT $7
                OFFSET $8
            "#,
            language,
            word_class,
            search.created_before,
            search.created_after,
            search.exact_slug,
            search.q,
            i64::from(pagination.limit),
            i64::from(pagination.offset)
        )
        .fetch_all(&self.state.pool);

        let count_future = sqlx::query_scalar!(
            r#"
                SELECT COUNT(*)
                FROM words
                WHERE
                language = $1
                AND ($2::UUID IS NULL OR word_class = $2)
                AND ($3::TIMESTAMPTZ IS NULL OR created_at < $3)
                AND ($4::TIMESTAMPTZ IS NULL OR created_at > $4)
                AND ($5::TEXT IS NULL OR slug = $5)
            "#,
            language,
            word_class,
            search.created_before,
            search.created_after,
            search.exact_slug,
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
}

#[derive(Default, Debug, Deserialize)]
pub struct WordSearch {
    pub q: Option<String>,
    pub exact_slug: Option<String>,
    pub word_class: Option<String>,
    pub created_before: Option<DateTime<Utc>>,
    pub created_after: Option<DateTime<Utc>>,
}

crate::util::repo_from_parts!(WordRepository);

#[async_trait::async_trait]
impl crate::model::bookmarks::ResolveBookmark for WordRepository {
    async fn resolve_bookmark(&self, item: Uuid, link_type: crate::model::bookmarks::LinkType) -> AppResult<String> {
        // api: /api/languages/{code}/words/{slug}
        // web: /languages/{code}/words/{slug}
        let word = self.find_by_id(item).await?;

        let languages = crate::model::languages::LanguageRepository::new(self.state.clone());
        let language = languages.find_by_id(word.language).await?;

        let slug = match link_type {
            crate::model::bookmarks::LinkType::Web => format!(
                "/languages/{}/words/{}/{}",
                language.code, word.slug, word.lemma
            ),
            crate::model::bookmarks::LinkType::Api => format!(
                "/api/languages/{}/words/{}/{}",
                language.code, word.slug, word.lemma
            ),
        };

        Ok(slug)
    }
}
