use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sqlx::FromRow;
use uuid::Uuid;
use validator::Validate;

use crate::{
    err::{AppResult, bad_request, internal_error, not_found},
    model::{
        definitions::{Definition, DefinitionRepository},
        language_invites::PermissionLevel,
        users::User,
    },
    pagination::{PaginatedRequest, PaginatedResponse},
    util::{AppState, ensure_verified, text_query},
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
    pub ipa: String,
    pub notes: String,
    pub extra: Option<JsonValue>,
    pub like_count: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing)]
    pub _created_by: Option<Uuid>,
    #[serde(skip_serializing)]
    pub _updated_by: Option<Uuid>,

    // materialized
    pub bookmark: String,
    pub language_code: Option<String>,
    pub word_class_abbreviation: Option<String>,
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

#[derive(Debug, Clone, Serialize)]
pub struct WordWithMeta {
    pub word: Word,
    pub first_definition: Option<Definition>,
    pub creator: User,
    pub is_liked: bool,
}

impl WordWithMeta {
    pub fn like_target(&self) -> String {
        format!(
            "/api/languages/{}/words/{}/{}",
            self.word.language_code.as_ref().unwrap(),
            self.word.slug,
            self.word.lemma
        )
    }
}

impl WordRepository {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    #[allow(dead_code)]
    pub fn make_slug(word: &str) -> String {
        nfkc(word)
    }

    pub async fn materialize(
        &self,
        word: Word,
        requestor: Option<&User>,
    ) -> AppResult<WordWithMeta> {
        let creator = self.find_creator(&word.id).await?;

        let first_definition = DefinitionRepository::new(self.state.clone())
            .get_first_by_word(&word.id)
            .await?;

        let is_liked = if let Some(user) = requestor {
            self.is_liked(&word.id, &user.id).await?
        } else {
            false
        };

        Ok(WordWithMeta {
            word,
            first_definition,
            creator,
            is_liked,
        })
    }

    pub async fn count_by_slug(&self, language: Uuid, slug: &str) -> AppResult<i64> {
        let count = sqlx::query_scalar!(
            r#"
                SELECT COUNT(*) FROM words
                WHERE language = $1 AND slug = $2
            "#,
            language,
            slug
        )
        .fetch_one(&self.state.pool)
        .await?;

        Ok(count.unwrap_or(0))
    }

    pub async fn make_slug_and_lemma(
        &self,
        language: Uuid,
        word: &str,
    ) -> AppResult<(String, i32)> {
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

    pub fn render_notes(word: &Word) -> AppResult<String> {
        if word.notes.is_empty() {
            Ok(String::new())
        } else {
            Ok(crate::md::render_md(&word.notes)?)
        }
    }

    pub async fn create(
        &self,
        requestor: &User,
        language: Uuid,
        word: CreateWord,
    ) -> AppResult<Word> {
        word.validate()?;

        ensure_verified(requestor)?;

        crate::model::user_bans::UserBanRepository::new(self.state.clone())
            .ensure_not_banned(requestor.id)
            .await?;

        let word_classes = crate::model::word_classes::WordClassRepository::new(self.state.clone());
        let word_class = word_classes
            .find_by_abbreviation(&language, &word.word_class)
            .await?;

        let (slug, lemma) = self.make_slug_and_lemma(language, &word.word).await?;

        let mut tx = self.state.pool.begin().await?;

        let permissions = crate::model::language_permissions::LanguagePermissionRepository::new(
            self.state.clone(),
        );
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
            word.ipa.unwrap_or_default(),
            word.notes.unwrap_or_default(),
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

        let perm_check = permissions
            .check_permission_with_audit(
                crate::model::language_permissions::CheckPermissionReq {
                    user: requestor.id,
                    language,
                    required_level: PermissionLevel::Editor,
                    action_type: crate::model::audit_log::AuditActionType::Created,
                    resource_type: crate::model::audit_log::AuditableResource::Word,
                    resource_id: word_result.id,
                    context: Some(serde_json::json!({
                        "language_id": language,
                        "word": word.word
                    })),
                },
                &mut tx,
            )
            .await?;

        if perm_check == crate::model::audit_log::PermissionCheck::NoPermission {
            return Err(bad_request("you don't have permission to create words"));
        }

        tx.commit().await?;

        // fetch the created word to get materialized fields
        let created_word = self.find_by_id(word_result.id).await?;

        // Create activity if language is public
        let lang_repo = crate::model::languages::LanguageRepository::new(self.state.clone());
        let lang = lang_repo.find_by_id(language).await?;
        if !lang.private {
            let activity_repo =
                crate::model::user_activities::UserActivityRepository::new(self.state.clone());
            let _activity = activity_repo
                .create(
                    requestor.id,
                    crate::model::user_activities::ActivityType::CreateWord,
                    created_word.id,
                    "word",
                    Some(language),
                    Some("language"),
                )
                .await?;
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
                    words.like_count,
                    words.created_at,
                    words.updated_at,
                    words.created_by as "_created_by!",
                    words.updated_by as "_updated_by!",
                    COALESCE(bookmarks.slug, '')::text as "bookmark!",
                    languages.code as "language_code: Option<String>",
                    word_classes.abbreviation as "word_class_abbreviation: Option<String>",
                    created.username as "created_by: Option<String>",
                    updated.username as "updated_by: Option<String>"
                FROM words
                LEFT JOIN bookmarks ON bookmarks.item = words.id AND bookmarks.resource = 'lemma'
                LEFT JOIN languages ON languages.id = words.language
                LEFT JOIN word_classes ON word_classes.id = words.word_class
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
                    words.like_count,
                    words.created_at,
                    words.updated_at,
                    words.created_by as "_created_by!",
                    words.updated_by as "_updated_by!",
                    COALESCE(bookmarks.slug, '')::text as "bookmark!",
                    languages.code as "language_code: Option<String>",
                    word_classes.abbreviation as "word_class_abbreviation: Option<String>",
                    created.username as "created_by: Option<String>",
                    updated.username as "updated_by: Option<String>"
                FROM words
                LEFT JOIN bookmarks ON bookmarks.item = words.id AND bookmarks.resource = 'lemma'
                LEFT JOIN languages ON languages.id = words.language
                LEFT JOIN word_classes ON word_classes.id = words.word_class
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

    pub async fn update_by_lemma(
        &self,
        requestor: &User,
        language: Uuid,
        slug: &str,
        lemma: i32,
        updates: UpdateWord,
    ) -> AppResult<Word> {
        updates.validate()?;

        ensure_verified(requestor)?;

        crate::model::user_bans::UserBanRepository::new(self.state.clone())
            .ensure_not_banned(requestor.id)
            .await?;

        // get the word to find its language
        let word = self
            .find_by_slug_and_lemma(Some(requestor), language, slug, lemma)
            .await?;

        let word_classes = crate::model::word_classes::WordClassRepository::new(self.state.clone());

        let word_class = if let Some(ref abbreviation) = updates.word_class {
            Some(
                word_classes
                    .find_by_abbreviation(&word.language, abbreviation)
                    .await?
                    .id,
            )
        } else {
            None
        };

        let mut tx = self.state.pool.begin().await?;

        let permissions = crate::model::language_permissions::LanguagePermissionRepository::new(
            self.state.clone(),
        );
        let perm_check = permissions
            .check_permission_with_audit(
                crate::model::language_permissions::CheckPermissionReq {
                    user: requestor.id,
                    language: word.language,
                    required_level: PermissionLevel::Editor,
                    action_type: crate::model::audit_log::AuditActionType::Updated,
                    resource_type: crate::model::audit_log::AuditableResource::Word,
                    resource_id: word.id,
                    context: Some(serde_json::json!({
                        "language_id": word.language,
                        "word": word.word,
                        "updates": updates
                    })),
                },
                &mut tx,
            )
            .await?;

        if perm_check == crate::model::audit_log::PermissionCheck::NoPermission {
            return Err(bad_request("you don't have permission to edit words"));
        }

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
                    words.like_count,
                    words.created_at,
                    words.updated_at,
                    words.created_by as "_created_by!",
                    words.updated_by as "_updated_by!",
                    COALESCE((SELECT slug FROM bookmarks WHERE item = words.id AND resource = 'lemma'), '') as "bookmark!",
                    (SELECT code FROM languages WHERE id = words.language) as language_code,
                    (SELECT abbreviation FROM word_classes WHERE id = words.word_class) as word_class_abbreviation,
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

        let updated_word =
            result.ok_or_else(|| not_found(format!("word with id '{}'", word.id)))?;

        // Create activity if language is public
        let lang_repo = crate::model::languages::LanguageRepository::new(self.state.clone());
        let lang = lang_repo.find_by_id(word.language).await?;
        if !lang.private {
            let activity_repo =
                crate::model::user_activities::UserActivityRepository::new(self.state.clone());
            let _activity = activity_repo
                .create(
                    requestor.id,
                    crate::model::user_activities::ActivityType::UpdateWord,
                    updated_word.id,
                    "word",
                    Some(word.language),
                    Some("language"),
                )
                .await?;
        }

        Ok(updated_word)
    }

    pub async fn delete_by_lemma(
        &self,
        requestor: &User,
        language: Uuid,
        slug: &str,
        lemma: i32,
    ) -> AppResult<bool> {
        ensure_verified(requestor)?;

        crate::model::user_bans::UserBanRepository::new(self.state.clone())
            .ensure_not_banned(requestor.id)
            .await?;

        // get the word to find its language
        let word = self
            .find_by_slug_and_lemma(Some(requestor), language, slug, lemma)
            .await?;

        let mut tx = self.state.pool.begin().await?;

        let permissions = crate::model::language_permissions::LanguagePermissionRepository::new(
            self.state.clone(),
        );
        let perm_check = permissions
            .check_permission_with_audit(
                crate::model::language_permissions::CheckPermissionReq {
                    user: requestor.id,
                    language: word.language,
                    required_level: PermissionLevel::Editor,
                    action_type: crate::model::audit_log::AuditActionType::Deleted,
                    resource_type: crate::model::audit_log::AuditableResource::Word,
                    resource_id: word.id,
                    context: Some(serde_json::json!({
                        "language_id": word.language,
                        "word": word.word
                    })),
                },
                &mut tx,
            )
            .await?;

        if perm_check == crate::model::audit_log::PermissionCheck::NoPermission {
            return Err(bad_request("you don't have permission to delete words"));
        }

        let result = sqlx::query!("DELETE FROM words WHERE id = $1", word.id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;

        Ok(result.rows_affected() > 0)
    }

    #[allow(clippy::too_many_lines)]
    pub async fn search(
        &self,
        language: &Uuid,
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
                    words.like_count,
                    words.created_at,
                    words.updated_at,
                    words.created_by as "_created_by!",
                    words.updated_by as "_updated_by!",
                    COALESCE(bookmarks.slug, '')::text as "bookmark!",
                    languages.code as "language_code: Option<String>",
                    word_classes.abbreviation as "word_class_abbreviation: Option<String>",
                    created.username as "created_by: Option<String>",
                    updated.username as "updated_by: Option<String>"
                FROM words
                LEFT JOIN bookmarks ON bookmarks.item = words.id AND bookmarks.resource = 'lemma'
                LEFT JOIN languages ON languages.id = words.language
                LEFT JOIN word_classes ON word_classes.id = words.word_class
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
                ) DESC, words.id DESC
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

    pub async fn find_creator(&self, word: &Uuid) -> AppResult<User> {
        let result = sqlx::query_as!(
            User,
            r#"
                SELECT users.*,
                    COALESCE(bookmarks.slug, '')::text as "bookmark!"
                FROM users
                JOIN words ON words.created_by = users.id
                LEFT JOIN bookmarks ON bookmarks.item = users.id AND bookmarks.resource = 'user'
                WHERE words.id = $1
            "#,
            word
        )
        .fetch_optional(&self.state.pool)
        .await?;

        result.ok_or_else(|| not_found(format!("creator for word with id '{}'", word)))
    }

    pub async fn count_contributors(&self, word_id: Uuid) -> AppResult<i64> {
        let result = sqlx::query_scalar!(
            r#"
                SELECT COUNT(DISTINCT user_id) FROM (
                    SELECT created_by as user_id FROM words WHERE id = $1
                    UNION
                    SELECT updated_by as user_id FROM words WHERE id = $1
                    UNION
                    SELECT created_by as user_id FROM definitions WHERE word = $1
                    UNION
                    SELECT updated_by as user_id FROM definitions WHERE word = $1
                ) contributors
                WHERE user_id != (SELECT created_by FROM words WHERE id = $1)
            "#,
            word_id
        )
        .fetch_one(&self.state.pool)
        .await?;

        Ok(result.unwrap_or(0))
    }

    pub async fn like_word(&self, word_id: Uuid, user_id: Uuid) -> AppResult<Option<i64>> {
        crate::model::user_bans::UserBanRepository::new(self.state.clone())
            .ensure_not_banned(user_id)
            .await?;

        let mut tx = self.state.pool.begin().await?;
        let result = sqlx::query!(
            r#"
                INSERT INTO word_likes (word_id, user_id)
                VALUES ($1, $2)
                ON CONFLICT DO NOTHING
            "#,
            word_id,
            user_id
        )
        .execute(&mut *tx)
        .await?;

        let likes = if result.rows_affected() > 0 {
            let likes = sqlx::query_scalar!(
                r#"
                    UPDATE words
                    SET like_count = like_count + 1
                    WHERE id = $1
                    RETURNING like_count
                "#,
                word_id
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

    pub async fn unlike_word(&self, word_id: Uuid, user_id: Uuid) -> AppResult<Option<i64>> {
        crate::model::user_bans::UserBanRepository::new(self.state.clone())
            .ensure_not_banned(user_id)
            .await?;

        let mut tx = self.state.pool.begin().await?;
        let result = sqlx::query!(
            r#"
                DELETE FROM word_likes
                WHERE word_id = $1 AND user_id = $2
            "#,
            word_id,
            user_id
        )
        .execute(&mut *tx)
        .await?;

        let likes = if result.rows_affected() > 0 {
            let likes = sqlx::query_scalar!(
                r#"
                    UPDATE words
                    SET like_count = GREATEST(like_count - 1, 0)
                    WHERE id = $1
                    RETURNING like_count
                "#,
                word_id
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

    pub async fn is_liked(&self, word_id: &Uuid, user_id: &Uuid) -> AppResult<bool> {
        let result = sqlx::query!(
            r#"
                SELECT 1 as exists FROM word_likes
                WHERE word_id = $1 AND user_id = $2
            "#,
            word_id,
            user_id
        )
        .fetch_optional(&self.state.pool)
        .await?;

        Ok(result.is_some())
    }

    pub async fn search_across_languages(
        &self,
        user: &User,
        query: &str,
        exclude_id: Option<Uuid>,
        limit_per_language: i64,
    ) -> AppResult<CrossLanguageSearchResponse> {
        // Get languages where user has Editor+ permissions
        let editable_languages = sqlx::query!(
            r#"
                SELECT language, languages.code as "language_code!", languages.name as "language_name!"
                FROM language_permissions
                JOIN languages ON languages.id = language_permissions.language
                WHERE language_permissions."user" = $1
                  AND language_permissions.permission IN ('editor', 'admin', 'owner')
            "#,
            user.id
        )
        .fetch_all(&self.state.pool)
        .await?;

        if editable_languages.is_empty() {
            return Ok(CrossLanguageSearchResponse { languages: vec![] });
        }

        // Search words in each language
        let mut language_groups = Vec::new();

        for lang in editable_languages {
            let words = sqlx::query!(
                r#"
                    SELECT
                        words.id,
                        words.word,
                        words.slug,
                        words.lemma,
                        COALESCE(bookmarks.slug, '')::text as "bookmark!",
                        languages.code as "language_code!",
                        word_classes.abbreviation as "word_class_abbreviation?",
                        words.ipa as "ipa?"
                    FROM words
                    LEFT JOIN bookmarks ON bookmarks.item = words.id AND bookmarks.resource = 'lemma'
                    LEFT JOIN languages ON languages.id = words.language
                    LEFT JOIN word_classes ON word_classes.id = words.word_class
                    WHERE words.language = $1
                      AND ($2::UUID IS NULL OR words.id != $2)
                      AND (
                          words.word ILIKE '%' || $3 || '%'
                          OR similarity(words.word, $3) > 0.3
                      )
                    ORDER BY
                        CASE WHEN words.word ILIKE $3 || '%' THEN 0 ELSE 1 END,
                        similarity(words.word, $3) DESC,
                        words.word
                    LIMIT $4
                "#,
                lang.language,
                exclude_id,
                query,
                limit_per_language
            )
            .fetch_all(&self.state.pool)
            .await?;

            if !words.is_empty() {
                let word_results = words
                    .into_iter()
                    .map(|w| WordSearchResult {
                        id: w.id,
                        word: w.word,
                        slug: w.slug,
                        lemma: w.lemma,
                        bookmark: w.bookmark,
                        language_code: w.language_code,
                        word_class_abbreviation: w.word_class_abbreviation,
                        ipa: w.ipa,
                    })
                    .collect();

                language_groups.push(LanguageWordsGroup {
                    language_id: lang.language,
                    language_code: lang.language_code,
                    language_name: lang.language_name,
                    words: word_results,
                });
            }
        }

        // Sort language groups by number of matches (descending)
        language_groups.sort_by(|a, b| b.words.len().cmp(&a.words.len()));

        Ok(CrossLanguageSearchResponse {
            languages: language_groups,
        })
    }
}

#[derive(Default, Debug, Deserialize, Clone, Serialize)]
pub struct WordSearch {
    pub q: Option<String>,
    pub exact_slug: Option<String>,
    pub word_class: Option<String>,
    pub created_before: Option<DateTime<Utc>>,
    pub created_after: Option<DateTime<Utc>>,
}

text_query!(WordSearch);

#[derive(Debug, Serialize, Deserialize)]
pub struct WordSearchResult {
    pub id: Uuid,
    pub word: String,
    pub slug: String,
    pub lemma: i32,
    pub bookmark: String,
    pub language_code: String,
    pub word_class_abbreviation: Option<String>,
    pub ipa: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LanguageWordsGroup {
    pub language_id: Uuid,
    pub language_code: String,
    pub language_name: String,
    pub words: Vec<WordSearchResult>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CrossLanguageSearchResponse {
    pub languages: Vec<LanguageWordsGroup>,
}

crate::util::repo_from_parts!(WordRepository);

#[async_trait::async_trait]
impl crate::model::bookmarks::ResolveBookmark for WordRepository {
    async fn resolve_bookmark(
        &self,
        item: Uuid,
        link_type: crate::model::bookmarks::LinkType,
    ) -> AppResult<String> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::audit_log::{
        AuditActionType, AuditLogFilter, AuditLogRepository, AuditableResource,
    };
    use crate::model::languages::{CreateLanguage, LanguageRepository};
    use crate::model::users::UserRepository;
    use crate::pagination::PaginatedRequest;
    use crate::{config::CONFIG, create_router, email};
    use sqlx::PgPool;

    #[tokio::test]
    async fn test_create_word_as_admin_creates_audit_log() {
        let pool = PgPool::connect(&CONFIG.database_url).await.unwrap();
        let email_service = std::sync::Arc::new(email::MockEmailService::new());
        let app_state = crate::util::AppState {
            pool: pool.clone(),
            email_service: email_service.clone(),
        };
        let app = create_router(app_state.clone()).into_service();

        // Create an admin user
        let admin_username = crate::tests::random_name();
        let _admin_token =
            crate::tests::make_authed_user(&admin_username, &app, email_service.clone()).await;
        let admin_id =
            sqlx::query_scalar!("select id from users where username = $1", admin_username)
                .fetch_one(&pool)
                .await
                .unwrap();
        sqlx::query!(
            "insert into user_tags (user_id, tag, hidden) values ($1, 'admin', false)",
            admin_id
        )
        .execute(&pool)
        .await
        .unwrap();

        let user_repo = UserRepository::new(app_state.clone());
        let admin = user_repo.find_by_id(admin_id).await.unwrap();

        // Create a language by another user
        let owner_username = crate::tests::random_name();
        let _owner_token =
            crate::tests::make_authed_user(&owner_username, &app, email_service.clone()).await;
        let owner_id =
            sqlx::query_scalar!("select id from users where username = $1", owner_username)
                .fetch_one(&pool)
                .await
                .unwrap();
        let owner = user_repo.find_by_id(owner_id).await.unwrap();

        let lang_repo = LanguageRepository::new(app_state.clone());
        let lang = lang_repo
            .create(
                &owner,
                CreateLanguage {
                    name: "Test Language".to_string(),
                    code: crate::tests::random_code(),
                    description: "A test language".to_string(),
                    private: false,
                },
            )
            .await
            .unwrap();

        // Admin creates a word (without permission)
        let word_repo = WordRepository::new(app_state.clone());
        let word = word_repo
            .create(
                &admin,
                lang.id,
                CreateWord {
                    word: "testword".to_string(),
                    word_class: "n".to_string(),
                    ipa: None,
                    notes: Some("test notes".to_string()),
                    extra: None,
                },
            )
            .await
            .unwrap();

        // Check audit log was created
        let audit_repo = AuditLogRepository::new(app_state.clone());
        let logs = audit_repo
            .search(
                &admin,
                PaginatedRequest::default(),
                AuditLogFilter {
                    user_id: Some(admin.id),
                    action: Some(AuditActionType::Created),
                    resource_type: Some(AuditableResource::Word),
                    resource_id: Some(word.id),
                },
            )
            .await
            .unwrap();

        assert_eq!(logs.items.len(), 1);
        let log = &logs.items[0];
        assert_eq!(log.user_id, Some(admin.id));
        assert_eq!(log.action, AuditActionType::Created);
        assert_eq!(log.resource_type, AuditableResource::Word);
        assert_eq!(log.resource_id, word.id);
        assert_eq!(log.details["language_id"], serde_json::json!(lang.id));
        assert_eq!(log.details["word"], serde_json::json!("testword"));
    }

    #[tokio::test]
    async fn test_update_word_as_moderator_creates_audit_log() {
        let pool = PgPool::connect(&CONFIG.database_url).await.unwrap();
        let email_service = std::sync::Arc::new(email::MockEmailService::new());
        let app_state = crate::util::AppState {
            pool: pool.clone(),
            email_service: email_service.clone(),
        };
        let app = create_router(app_state.clone()).into_service();

        // Create a moderator user
        let mod_username = crate::tests::random_name();
        let _mod_token =
            crate::tests::make_authed_user(&mod_username, &app, email_service.clone()).await;
        let mod_id = sqlx::query_scalar!("select id from users where username = $1", mod_username)
            .fetch_one(&pool)
            .await
            .unwrap();
        sqlx::query!(
            "insert into user_tags (user_id, tag, hidden) values ($1, 'moderator', false)",
            mod_id
        )
        .execute(&pool)
        .await
        .unwrap();

        let user_repo = UserRepository::new(app_state.clone());
        let moderator = user_repo.find_by_id(mod_id).await.unwrap();

        // Create a language and word by another user
        let owner_username = crate::tests::random_name();
        let _owner_token =
            crate::tests::make_authed_user(&owner_username, &app, email_service.clone()).await;
        let owner_id =
            sqlx::query_scalar!("select id from users where username = $1", owner_username)
                .fetch_one(&pool)
                .await
                .unwrap();
        let owner = user_repo.find_by_id(owner_id).await.unwrap();

        let lang_repo = LanguageRepository::new(app_state.clone());
        let lang = lang_repo
            .create(
                &owner,
                CreateLanguage {
                    name: "Test Language".to_string(),
                    code: crate::tests::random_code(),
                    description: "A test language".to_string(),
                    private: false,
                },
            )
            .await
            .unwrap();

        let word_repo = WordRepository::new(app_state.clone());
        let word = word_repo
            .create(
                &owner,
                lang.id,
                CreateWord {
                    word: "testword".to_string(),
                    word_class: "n".to_string(),
                    ipa: None,
                    notes: Some("test notes".to_string()),
                    extra: None,
                },
            )
            .await
            .unwrap();

        // Moderator updates the word (without permission)
        let updated = word_repo
            .update_by_lemma(
                &moderator,
                lang.id,
                &word.slug,
                word.lemma,
                UpdateWord {
                    word: None,
                    word_class: None,
                    ipa: None,
                    notes: Some("updated notes".to_string()),
                    extra: None,
                },
            )
            .await
            .unwrap();

        assert_eq!(updated.notes, "updated notes".to_string());

        // Check audit log was created
        let audit_repo = AuditLogRepository::new(app_state.clone());
        let logs = audit_repo
            .search(
                &moderator,
                PaginatedRequest::default(),
                AuditLogFilter {
                    user_id: Some(moderator.id),
                    action: Some(AuditActionType::Updated),
                    resource_type: Some(AuditableResource::Word),
                    resource_id: Some(word.id),
                },
            )
            .await
            .unwrap();

        assert_eq!(logs.items.len(), 1);
        let log = &logs.items[0];
        assert_eq!(log.user_id, Some(moderator.id));
        assert_eq!(log.action, AuditActionType::Updated);
        assert_eq!(log.resource_type, AuditableResource::Word);
        assert_eq!(log.resource_id, word.id);
        assert_eq!(log.details["language_id"], serde_json::json!(lang.id));
        assert_eq!(log.details["word"], serde_json::json!("testword"));
    }

    #[tokio::test]
    async fn test_delete_word_as_admin_creates_audit_log() {
        let pool = PgPool::connect(&CONFIG.database_url).await.unwrap();
        let email_service = std::sync::Arc::new(email::MockEmailService::new());
        let app_state = crate::util::AppState {
            pool: pool.clone(),
            email_service: email_service.clone(),
        };
        let app = create_router(app_state.clone()).into_service();

        // Create an admin user
        let admin_username = crate::tests::random_name();
        let _admin_token =
            crate::tests::make_authed_user(&admin_username, &app, email_service.clone()).await;
        let admin_id =
            sqlx::query_scalar!("select id from users where username = $1", admin_username)
                .fetch_one(&pool)
                .await
                .unwrap();
        sqlx::query!(
            "insert into user_tags (user_id, tag, hidden) values ($1, 'admin', false)",
            admin_id
        )
        .execute(&pool)
        .await
        .unwrap();

        let user_repo = UserRepository::new(app_state.clone());
        let admin = user_repo.find_by_id(admin_id).await.unwrap();

        // Create a language and word by another user
        let owner_username = crate::tests::random_name();
        let _owner_token =
            crate::tests::make_authed_user(&owner_username, &app, email_service.clone()).await;
        let owner_id =
            sqlx::query_scalar!("select id from users where username = $1", owner_username)
                .fetch_one(&pool)
                .await
                .unwrap();
        let owner = user_repo.find_by_id(owner_id).await.unwrap();

        let lang_repo = LanguageRepository::new(app_state.clone());
        let lang = lang_repo
            .create(
                &owner,
                CreateLanguage {
                    name: "Test Language".to_string(),
                    code: crate::tests::random_code(),
                    description: "A test language".to_string(),
                    private: false,
                },
            )
            .await
            .unwrap();

        let word_repo = WordRepository::new(app_state.clone());
        let word = word_repo
            .create(
                &owner,
                lang.id,
                CreateWord {
                    word: "testword".to_string(),
                    word_class: "n".to_string(),
                    ipa: None,
                    notes: Some("test notes".to_string()),
                    extra: None,
                },
            )
            .await
            .unwrap();

        let word_id = word.id;

        // Admin deletes the word (without permission)
        let deleted = word_repo
            .delete_by_lemma(&admin, lang.id, &word.slug, word.lemma)
            .await
            .unwrap();
        assert!(deleted);

        // Check audit log was created
        let audit_repo = AuditLogRepository::new(app_state.clone());
        let logs = audit_repo
            .search(
                &admin,
                PaginatedRequest::default(),
                AuditLogFilter {
                    user_id: Some(admin.id),
                    action: Some(AuditActionType::Deleted),
                    resource_type: Some(AuditableResource::Word),
                    resource_id: Some(word_id),
                },
            )
            .await
            .unwrap();

        assert_eq!(logs.items.len(), 1);
        let log = &logs.items[0];
        assert_eq!(log.user_id, Some(admin.id));
        assert_eq!(log.action, AuditActionType::Deleted);
        assert_eq!(log.resource_type, AuditableResource::Word);
        assert_eq!(log.resource_id, word_id);
        assert_eq!(log.details["language_id"], serde_json::json!(lang.id));
        assert_eq!(log.details["word"], serde_json::json!("testword"));
    }
}
