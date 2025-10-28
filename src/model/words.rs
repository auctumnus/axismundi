use axum::http::request;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
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
#[allow(clippy::struct_field_names)]
pub struct Word {
    pub id: Uuid,
    pub language: Uuid,
    pub word_class: Option<Uuid>,
    pub word: String,
    pub slug: String,
    pub lemma: i32,
    pub definition: String,
    pub ipa: Option<String>,
    pub notes: Option<String>,
    pub extra: Option<JsonValue>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub created_by: Uuid,
    pub updated_by: Uuid,
    pub bookmark: String,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CreateWord {
    #[validate(length(min = 1, max = 200))]
    pub word: String,

    #[validate(length(min = 1, max = 10))]
    pub word_class: String,

    #[validate(length(min = 1, max = 5000))]
    pub definition: String,

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

    #[validate(length(min = 1, max = 5000))]
    pub definition: Option<String>,

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

    pub fn make_slug(&self, word: &str) -> String {
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

        let lemma = (number_slugs.unwrap_or(1) as i32) + 1;
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
                INSERT INTO words (language, word_class, word, slug, lemma, definition, ipa, notes, extra, created_by, updated_by)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $10)
                RETURNING id, language, word_class, word, slug, lemma, definition, ipa, notes, extra, created_by, updated_by, created_at, updated_at
            "#,
            language,
            word_class.id,
            word.word,
            slug,
            lemma,
            word.definition,
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

        let result = Word {
            id: word_result.id,
            language: word_result.language,
            word_class: word_result.word_class,
            word: word_result.word,
            slug: word_result.slug,
            lemma: word_result.lemma,
            definition: word_result.definition,
            ipa: word_result.ipa,
            notes: word_result.notes,
            extra: word_result.extra,
            created_at: word_result.created_at,
            updated_at: word_result.updated_at,
            created_by: word_result.created_by,
            updated_by: word_result.updated_by,
            bookmark: bookmark_slug,
        };

        Ok(result)
    }

    pub async fn find_by_id(&self, id: Uuid) -> AppResult<Word> {
        let result = sqlx::query_as!(
            Word,
            r#"
                SELECT
                    words.id,
                    words.language,
                    words.word_class,
                    words.word,
                    words.slug,
                    words.lemma,
                    words.definition,
                    words.ipa,
                    words.notes,
                    words.extra,
                    words.created_at,
                    words.updated_at,
                    words.created_by,
                    words.updated_by,
                    COALESCE(bookmarks.slug, '')::text as "bookmark!"
                FROM words
                LEFT JOIN bookmarks ON bookmarks.item = words.id AND bookmarks.resource = 'lemma'
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
        requestor: Option<&User>,
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
                    words.word,
                    words.slug,
                    words.lemma,
                    words.definition,
                    words.ipa,
                    words.notes,
                    words.extra,
                    words.created_at,
                    words.updated_at,
                    words.created_by,
                    words.updated_by,
                    COALESCE(bookmarks.slug, '')::text as "bookmark!"
                FROM words
                LEFT JOIN bookmarks ON bookmarks.item = words.id AND bookmarks.resource = 'lemma'
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
                    definition = COALESCE($5, definition),
                    ipa = COALESCE($6, ipa),
                    notes = COALESCE($7, notes),
                    extra = COALESCE($8, extra),
                    updated_by = $9,
                    updated_at = CURRENT_TIMESTAMP,
                    lemma = COALESCE($10, lemma)
                WHERE id = $1
                RETURNING words.*, (SELECT slug FROM bookmarks WHERE item = words.id AND resource = 'lemma') as "bookmark!"
            "#,
            word.id,
            word_class,
            updates.word,
            slug,
            updates.definition,
            updates.ipa,
            updates.notes,
            updates.extra,
            requestor.id,
            lemma,
        )
        .fetch_optional(&mut *tx)
        .await?;

        tx.commit().await?;

        result.ok_or_else(|| not_found(format!("word with id '{}'", word.id)))
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
                    words.*,
                    COALESCE(bookmarks.slug, '')::text as "bookmark!"
                FROM words
                LEFT JOIN bookmarks ON bookmarks.item = words.id AND bookmarks.resource = 'lemma'
                WHERE
                words.language = $1
                AND ($2::UUID IS NULL OR words.word_class = $2)
                AND ($3::TIMESTAMPTZ IS NULL OR words.created_at < $3)
                AND ($4::TIMESTAMPTZ IS NULL OR words.created_at > $4)
                AND ($5::TEXT IS NULL OR words.slug = $5)
                ORDER BY (
                    CASE
                        WHEN $6::TEXT IS NOT NULL AND words.word ILIKE '%' || $6 || '%' THEN 100.0
                        WHEN $6::TEXT IS NOT NULL AND words.definition ILIKE '%' || $6 || '%' THEN 90.0
                        WHEN $6::TEXT IS NOT NULL AND words.notes ILIKE '%' || $6 || '%' THEN 80.0
                        ELSE 0.0
                    END +
                    CASE WHEN $6::TEXT IS NOT NULL THEN
                        similarity(words.word, $6) * 3.0 +
                        COALESCE(similarity(words.definition, $6), 0.0) * 2.0 +
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
