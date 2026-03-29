use chrono::{DateTime, Utc};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::fmt::Write as _;
use uuid::Uuid;
use validator::Validate;

use crate::{
    err::{AppError, AppResult, bad_request, not_found},
    model::{
        language_invites::PermissionLevel,
        languages::Language,
        translatable::{Translatable, TranslatableRepository},
        users::User,
    },
    pagination::{PaginatedRequest, PaginatedResponse},
    util::{AppState, ensure_verified},
};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Translation {
    pub id: Uuid,
    #[serde(skip_serializing)]
    pub translatable: Uuid,
    #[serde(skip_serializing)]
    pub language: Uuid,
    pub translated_text: String,
    pub translated_title: Option<String>,
    pub ipa: Option<String>,
    pub gloss: Option<String>,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub like_count: i64,

    #[serde(skip_serializing)]
    pub created_by: Uuid,
    #[serde(skip_serializing)]
    #[allow(dead_code)]
    pub updated_by: Uuid,

    pub translatable_slug: String,
    pub translatable_title: String,
    pub language_code: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TranslationWithLanguageAndContributor {
    pub translation: Translation,
    pub translatable: Translatable,
    pub language: Language,
    pub author: User,
    pub is_liked: bool,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CreateTranslation {
    #[validate(length(min = 1, max = 100_000))]
    pub translated_text: String,

    #[validate(length(min = 1, max = 40))]
    pub translated_title: Option<String>,

    #[validate(length(max = 100_000))]
    pub ipa: Option<String>,

    #[validate(length(max = 100_000))]
    pub gloss: Option<String>,

    #[validate(length(max = 100_000))]
    pub notes: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct UpdateTranslation {
    #[validate(length(min = 1, max = 100_000))]
    pub translated_text: Option<String>,

    #[validate(length(min = 1, max = 40))]
    pub translated_title: Option<String>,

    #[validate(length(max = 100_000))]
    pub ipa: Option<String>,

    #[validate(length(max = 100_000))]
    pub gloss: Option<String>,

    #[validate(length(max = 100_000))]
    pub notes: Option<String>,
}

pub struct TranslationRepository {
    state: AppState,
}

impl TranslationRepository {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    pub async fn materialize(
        &self,
        translation: Translation,
        requestor: Option<&User>,
    ) -> AppResult<TranslationWithLanguageAndContributor> {
        let translatable_repo = TranslatableRepository::new(self.state.clone());
        let language_repo = crate::model::languages::LanguageRepository::new(self.state.clone());
        let user_repo = crate::model::users::UserRepository::new(self.state.clone());

        let translatable = translatable_repo
            .find_by_id(translation.translatable)
            .await?;
        let language = language_repo.find_by_id(translation.language).await?;
        let author = user_repo.find_by_id(translation.created_by).await?;

        let is_liked = if let Some(requestor) = requestor {
            self.is_liked(&translation.id, &requestor.id).await?
        } else {
            false
        };

        Ok(TranslationWithLanguageAndContributor {
            translation,
            translatable,
            language,
            author,
            is_liked,
        })
    }

    pub async fn find_by_id(&self, id: Uuid) -> AppResult<Translation> {
        let result = sqlx::query_as!(
            Translation,
            r#"
                SELECT
                    t.id,
                    t.translatable,
                    t.language,
                    t.ipa,
                    t.gloss,
                    t.notes,
                    t.translated_text,
                    t.translated_title,
                    t.created_at,
                    t.updated_at,
                    t.like_count,
                    t.created_by,
                    t.updated_by,
                    tr.slug as translatable_slug,
                    tr.title as translatable_title,
                    l.code as language_code
                FROM translation t
                JOIN translatable tr ON t.translatable = tr.id
                JOIN languages l ON t.language = l.id
                WHERE t.id = $1
            "#,
            id
        )
        .fetch_optional(&self.state.pool)
        .await?;

        result.ok_or_else(|| not_found(format!("translation with id '{id}'")))
    }

    #[allow(dead_code)]
    pub async fn find_by_slug(&self, slug: &str) -> AppResult<Translation> {
        let result = sqlx::query_as!(
            Translation,
            r#"
                SELECT
                    t.id,
                    t.translatable,
                    t.language,
                    t.ipa,
                    t.gloss,
                    t.notes,
                    t.translated_text,
                    t.translated_title,
                    t.created_at,
                    t.updated_at,
                    t.like_count,
                    t.created_by,
                    t.updated_by,
                    tr.slug as translatable_slug,
                    tr.title as translatable_title,
                    l.code as language_code
                FROM translation t
                JOIN translatable tr ON t.translatable = tr.id
                JOIN languages l ON t.language = l.id
                WHERE tr.slug = $1
            "#,
            slug
        )
        .fetch_optional(&self.state.pool)
        .await?;

        result.ok_or_else(|| not_found(format!("translation with slug '{slug}'")))
    }

    pub async fn find_by_translatable_and_language(
        &self,
        translatable_id: Uuid,
        language_id: Uuid,
    ) -> AppResult<Translation> {
        let result = sqlx::query_as!(
            Translation,
            r#"
                SELECT
                    t.id,
                    t.translatable,
                    t.language,
                    t.ipa,
                    t.gloss,
                    t.notes,
                    t.translated_text,
                    t.translated_title,
                    t.created_at,
                    t.updated_at,
                    t.like_count,
                    t.created_by,
                    t.updated_by,
                    tr.slug as translatable_slug,
                    tr.title as translatable_title,
                    l.code as language_code
                FROM translation t
                JOIN translatable tr ON t.translatable = tr.id
                JOIN languages l ON t.language = l.id
                WHERE t.translatable = $1 AND t.language = $2
            "#,
            translatable_id,
            language_id
        )
        .fetch_one(&self.state.pool)
        .await?;

        Ok(result)
    }

    async fn verify_translation_unique(
        &self,
        translatable_id: Uuid,
        language_id: Uuid,
    ) -> AppResult<()> {
        match self
            .find_by_translatable_and_language(translatable_id, language_id)
            .await
        {
            Ok(_) => Err(bad_request(
                "a translation for this translatable in this language already exists",
            )),
            Err(AppError {
                status_code: StatusCode::NOT_FOUND,
                ..
            }) => Ok(()),
            e => e.map(|_| ()),
        }
    }

    async fn create_translation_activity(
        &self,
        requestor: &User,
        translation_id: Uuid,
        language_id: Uuid,
    ) -> AppResult<()> {
        let lang_repo = crate::model::languages::LanguageRepository::new(self.state.clone());
        let lang = lang_repo.find_by_id(language_id).await?;
        if !lang.private {
            let activity_repo =
                crate::model::user_activities::UserActivityRepository::new(self.state.clone());
            let _activity = activity_repo
                .create(
                    requestor.id,
                    crate::model::user_activities::ActivityType::CreateTranslation,
                    translation_id,
                    "translation",
                    Some(language_id),
                    Some("language"),
                )
                .await?;
        }
        Ok(())
    }

    pub async fn create(
        &self,
        requestor: &User,
        translatable_id: Uuid,
        language_id: Uuid,
        translation: CreateTranslation,
    ) -> AppResult<Translation> {
        translation.validate()?;
        ensure_verified(requestor)?;

        crate::model::user_bans::UserBanRepository::new(self.state.clone())
            .ensure_not_banned(requestor.id)
            .await?;

        // Verify the translatable exists
        crate::model::translatable::TranslatableRepository::new(self.state.clone())
            .find_by_id(translatable_id)
            .await?;

        self.verify_translation_unique(translatable_id, language_id)
            .await?;

        let mut tx = self.state.pool.begin().await?;

        let result = sqlx::query_as!(
            Translation,
            r#"
                WITH inserted AS (
                    INSERT INTO translation (translatable, language, translated_text, translated_title, created_by, updated_by, ipa, gloss, notes)
                    VALUES ($1, $2, $3, $4, $5, $5, $6, $7, $8)
                    RETURNING id, translatable, language, translated_text, translated_title, created_at, updated_at, created_by, updated_by, ipa, gloss, notes, like_count
                )
                SELECT
                    i.id,
                    i.translatable,
                    i.language,
                    i.ipa,
                    i.gloss,
                    i.notes,
                    i.translated_text,
                    i.translated_title,
                    i.created_at,
                    i.updated_at,
                    i.like_count,
                    i.created_by,
                    i.updated_by,
                    tr.slug as translatable_slug,
                    tr.title as translatable_title,
                    l.code as language_code
                FROM inserted i
                JOIN translatable tr ON i.translatable = tr.id
                JOIN languages l ON i.language = l.id
            "#,
            translatable_id,
            language_id,
            translation.translated_text,
            translation.translated_title,
            requestor.id,
            translation.ipa,
            translation.gloss,
            translation.notes
        )
        .fetch_one(&mut *tx)
        .await?;

        let permissions = crate::model::language_permissions::LanguagePermissionRepository::new(
            self.state.clone(),
        );
        let perm_check = permissions
            .check_permission_with_audit(
                crate::model::language_permissions::CheckPermissionReq {
                    user: requestor.id,
                    language: language_id,
                    required_level: PermissionLevel::Editor,
                    action_type: crate::model::audit_log::AuditActionType::Created,
                    resource_type: crate::model::audit_log::AuditableResource::Translation,
                    resource_id: result.id,
                    context: Some(serde_json::json!({
                        "language_id": language_id,
                        "translatable_id": translatable_id,
                        "translated_text": translation.translated_text
                    })),
                },
                &mut tx,
            )
            .await?;

        if perm_check == crate::model::audit_log::PermissionCheck::NoPermission {
            return Err(bad_request(
                "you don't have permission to create translations",
            ));
        }

        tx.commit().await?;

        self.create_translation_activity(requestor, result.id, language_id)
            .await?;

        Ok(result)
    }

    async fn create_update_activity(
        &self,
        requestor: &User,
        translation_id: Uuid,
        language_id: Uuid,
    ) -> AppResult<()> {
        let lang_repo = crate::model::languages::LanguageRepository::new(self.state.clone());
        let lang = lang_repo.find_by_id(language_id).await?;
        if !lang.private {
            let activity_repo =
                crate::model::user_activities::UserActivityRepository::new(self.state.clone());
            let _activity = activity_repo
                .create(
                    requestor.id,
                    crate::model::user_activities::ActivityType::UpdateTranslation,
                    translation_id,
                    "translation",
                    Some(language_id),
                    Some("language"),
                )
                .await?;
        }
        Ok(())
    }

    pub async fn update(
        &self,
        requestor: &User,
        id: Uuid,
        updates: UpdateTranslation,
    ) -> AppResult<Translation> {
        updates.validate()?;
        ensure_verified(requestor)?;

        crate::model::user_bans::UserBanRepository::new(self.state.clone())
            .ensure_not_banned(requestor.id)
            .await?;

        // Get the translation to find its language
        let existing = self.find_by_id(id).await?;

        let mut tx = self.state.pool.begin().await?;

        let permissions = crate::model::language_permissions::LanguagePermissionRepository::new(
            self.state.clone(),
        );
        let perm_check = permissions
            .check_permission_with_audit(
                crate::model::language_permissions::CheckPermissionReq {
                    user: requestor.id,
                    language: existing.language,
                    required_level: PermissionLevel::Editor,
                    action_type: crate::model::audit_log::AuditActionType::Updated,
                    resource_type: crate::model::audit_log::AuditableResource::Translation,
                    resource_id: id,
                    context: Some(serde_json::json!({
                        "language_id": existing.language,
                        "translatable_id": existing.translatable,
                        "updates": updates
                    })),
                },
                &mut tx,
            )
            .await?;

        if perm_check == crate::model::audit_log::PermissionCheck::NoPermission {
            return Err(bad_request(
                "you don't have permission to edit translations",
            ));
        }

        let result = sqlx::query_as!(
            Translation,
            r#"
                WITH updated AS (
                    UPDATE translation
                    SET translated_text = COALESCE($2, translated_text),
                        translated_title = COALESCE($3, translated_title),
                        ipa = COALESCE($4, ipa),
                        gloss = COALESCE($5, gloss),
                        notes = COALESCE($6, notes),
                        updated_by = $7,
                        updated_at = CURRENT_TIMESTAMP
                    WHERE id = $1
                    RETURNING id, translatable, language, translated_text, translated_title, created_at, updated_at, created_by, updated_by, ipa, gloss, notes, like_count
                )
                SELECT
                    u.id,
                    u.translatable,
                    u.language,
                    u.ipa,
                    u.gloss,
                    u.notes,
                    u.translated_text,
                    u.translated_title,
                    u.created_at,
                    u.updated_at,
                    u.like_count,
                    u.created_by,
                    u.updated_by,
                    tr.slug as translatable_slug,
                    tr.title as translatable_title,
                    l.code as language_code
                FROM updated u
                JOIN translatable tr ON u.translatable = tr.id
                JOIN languages l ON u.language = l.id
            "#,
            id,
            updates.translated_text,
            updates.translated_title,
            updates.ipa,
            updates.gloss,
            updates.notes,
            requestor.id
        )
        .fetch_optional(&mut *tx)
        .await?;

        let updated_translation =
            result.ok_or_else(|| not_found(format!("translation with id '{id}'")))?;

        tx.commit().await?;

        self.create_update_activity(requestor, updated_translation.id, existing.language)
            .await?;

        Ok(updated_translation)
    }

    pub async fn delete(&self, requestor: &User, id: Uuid) -> AppResult<bool> {
        ensure_verified(requestor)?;

        crate::model::user_bans::UserBanRepository::new(self.state.clone())
            .ensure_not_banned(requestor.id)
            .await?;

        // Get the translation to find its language
        let existing = self.find_by_id(id).await?;

        let mut tx = self.state.pool.begin().await?;

        let permissions = crate::model::language_permissions::LanguagePermissionRepository::new(
            self.state.clone(),
        );
        let perm_check = permissions
            .check_permission_with_audit(
                crate::model::language_permissions::CheckPermissionReq {
                    user: requestor.id,
                    language: existing.language,
                    required_level: PermissionLevel::Editor,
                    action_type: crate::model::audit_log::AuditActionType::Deleted,
                    resource_type: crate::model::audit_log::AuditableResource::Translation,
                    resource_id: id,
                    context: Some(serde_json::json!({
                        "language_id": existing.language,
                        "translatable_id": existing.translatable,
                        "translated_text": existing.translated_text
                    })),
                },
                &mut tx,
            )
            .await?;

        if perm_check == crate::model::audit_log::PermissionCheck::NoPermission {
            return Err(bad_request(
                "you don't have permission to delete translations",
            ));
        }

        let result = sqlx::query!("DELETE FROM translation WHERE id = $1", id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn list_by_translatable(
        &self,
        translatable_id: Uuid,
        pagination: PaginatedRequest,
    ) -> AppResult<PaginatedResponse<Translation>> {
        let items_future = sqlx::query_as!(
            Translation,
            r#"
                SELECT
                    t.id,
                    t.translatable,
                    t.language,
                    t.ipa,
                    t.gloss,
                    t.notes,
                    t.translated_text,
                    t.translated_title,
                    t.created_at,
                    t.updated_at,
                    t.like_count,
                    t.created_by,
                    t.updated_by,
                    tr.slug as translatable_slug,
                    tr.title as translatable_title,
                    l.code as language_code
                FROM translation t
                JOIN translatable tr ON t.translatable = tr.id
                JOIN languages l ON t.language = l.id
                WHERE t.translatable = $1
                ORDER BY t.created_at DESC
                LIMIT $2
                OFFSET $3
            "#,
            translatable_id,
            i64::from(pagination.limit),
            i64::from(pagination.offset)
        )
        .fetch_all(&self.state.pool);

        let count_future = sqlx::query_scalar!(
            r#"
                SELECT COUNT(*)
                FROM translation
                WHERE translatable = $1
            "#,
            translatable_id
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

    pub async fn list_by_language(
        &self,
        language_id: Uuid,
        pagination: PaginatedRequest,
    ) -> AppResult<PaginatedResponse<Translation>> {
        let items_future = sqlx::query_as!(
            Translation,
            r#"
                SELECT
                    t.id,
                    t.translatable,
                    t.language,
                    t.ipa,
                    t.gloss,
                    t.notes,
                    t.translated_text,
                    t.translated_title,
                    t.created_at,
                    t.updated_at,
                    t.like_count,
                    t.created_by,
                    t.updated_by,
                    tr.slug as translatable_slug,
                    tr.title as translatable_title,
                    l.code as language_code
                FROM translation t
                JOIN translatable tr ON t.translatable = tr.id
                JOIN languages l ON t.language = l.id
                WHERE t.language = $1
                ORDER BY t.created_at DESC
                LIMIT $2
                OFFSET $3
            "#,
            language_id,
            i64::from(pagination.limit),
            i64::from(pagination.offset)
        )
        .fetch_all(&self.state.pool);

        let count_future = sqlx::query_scalar!(
            r#"
                SELECT COUNT(*)
                FROM translation
                WHERE language = $1
            "#,
            language_id
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

    pub async fn is_liked(&self, translation_id: &Uuid, user_id: &Uuid) -> AppResult<bool> {
        let result = sqlx::query!(
            r#"
                SELECT 1 as exists FROM translation_likes
                WHERE translation_id = $1 AND user_id = $2
            "#,
            translation_id,
            user_id
        )
        .fetch_optional(&self.state.pool)
        .await?;

        Ok(result.is_some())
    }

    pub async fn like_translation(
        &self,
        translation_id: Uuid,
        user_id: Uuid,
    ) -> AppResult<Option<i64>> {
        crate::model::user_bans::UserBanRepository::new(self.state.clone())
            .ensure_not_banned(user_id)
            .await?;

        let mut tx = self.state.pool.begin().await?;
        let result = sqlx::query!(
            r#"
                INSERT INTO translation_likes (translation_id, user_id)
                VALUES ($1, $2)
                ON CONFLICT DO NOTHING
            "#,
            translation_id,
            user_id
        )
        .execute(&mut *tx)
        .await?;

        let likes = if result.rows_affected() > 0 {
            let likes = sqlx::query_scalar!(
                r#"
                    UPDATE translation
                    SET like_count = like_count + 1
                    WHERE id = $1
                    RETURNING like_count
                "#,
                translation_id
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

    pub async fn unlike_translation(
        &self,
        translation_id: Uuid,
        user_id: Uuid,
    ) -> AppResult<Option<i64>> {
        crate::model::user_bans::UserBanRepository::new(self.state.clone())
            .ensure_not_banned(user_id)
            .await?;

        let mut tx = self.state.pool.begin().await?;
        let result = sqlx::query!(
            r#"
                DELETE FROM translation_likes
                WHERE translation_id = $1 AND user_id = $2
            "#,
            translation_id,
            user_id
        )
        .execute(&mut *tx)
        .await?;

        let likes = if result.rows_affected() > 0 {
            let likes = sqlx::query_scalar!(
                r#"
                    UPDATE translation
                    SET like_count = GREATEST(like_count - 1, 0)
                    WHERE id = $1
                    RETURNING like_count
                "#,
                translation_id
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

    pub async fn as_json_ld(
        &self,
        translation: &Translation,
        translatable: &Translatable,
        language: &Language,
    ) -> AppResult<serde_json::Value> {
        let user_repo = crate::model::users::UserRepository::new(self.state.clone());
        let creator = user_repo.find_by_id(translation.created_by).await?;

        let translatable_repo = TranslatableRepository::new(self.state.clone());
        let translatable_ld = translatable_repo.as_json_ld(translatable).await?;

        let json_ld = serde_json::json!({
            "@context": "https://schema.org",
            "@type": "CreativeWork",
            "name": format!("{} ({} translation)", translatable.title, language.name),
            "text": translation.translated_text,
            "inLanguage": language.name,
            "translationOfWork": translatable_ld,
            "dateCreated": translation.created_at.to_rfc3339(),
            "dateModified": translation.updated_at.to_rfc3339(),
            "author": crate::model::users::UserRepository::as_json_ld(&creator),
            "url": format!("{}/translatable/{}/translation/{}", crate::config::CONFIG.public_url_base, translatable.slug, language.code),
        });

        Ok(json_ld)
    }
}

#[derive(Default, Debug, Deserialize, Clone, Serialize)]
pub struct TranslationSearch {
    pub q: Option<String>,
    pub created_before: Option<DateTime<Utc>>,
    pub created_after: Option<DateTime<Utc>>,
}

impl TranslationRepository {
    fn build_search_queries(search: &TranslationSearch) -> (String, String, usize) {
        let mut param_count = 1;

        let mut items_query = String::from(
            r"
                SELECT
                    t.id,
                    t.translatable,
                    t.language,
                    t.ipa,
                    t.gloss,
                    t.notes,
                    t.translated_text,
                    t.translated_title,
                    t.created_at,
                    t.updated_at,
                    t.like_count,
                    t.created_by,
                    t.updated_by,
                    tr.slug as translatable_slug,
                    tr.title as translatable_title,
                    l.code as language_code
                FROM translation t
                JOIN translatable tr ON t.translatable = tr.id
                JOIN languages l ON t.language = l.id
                WHERE t.language = $1
            ",
        );

        let mut count_query = String::from(
            "SELECT COUNT(*) FROM translation t JOIN translatable tr ON t.translatable = tr.id WHERE t.language = $1",
        );

        if search.q.is_some() {
            param_count += 1;
            let condition = format!(
                " AND (tr.title ILIKE ${} OR t.translated_text ILIKE ${})",
                param_count, param_count
            );
            items_query.push_str(&condition);
            count_query.push_str(&condition);
        }

        if search.created_before.is_some() {
            param_count += 1;
            let condition = format!(" AND t.created_at < ${}", param_count);
            items_query.push_str(&condition);
            count_query.push_str(&condition);
        }

        if search.created_after.is_some() {
            param_count += 1;
            let condition = format!(" AND t.created_at > ${}", param_count);
            items_query.push_str(&condition);
            count_query.push_str(&condition);
        }

        write!(
            &mut items_query,
            " ORDER BY t.created_at DESC LIMIT ${} OFFSET ${}",
            param_count + 1,
            param_count + 2
        )
        .unwrap();

        (items_query, count_query, param_count)
    }

    async fn execute_search_queries(
        &self,
        items_query: &str,
        count_query: &str,
        language_id: &Uuid,
        search: &TranslationSearch,
        pagination: &PaginatedRequest,
    ) -> AppResult<(Vec<Translation>, i64)> {
        let search_pattern = search.q.as_ref().map(|q| format!("%{q}%"));

        let mut items_q = sqlx::query_as::<_, Translation>(items_query).bind(language_id);
        let mut count_q = sqlx::query_scalar::<_, i64>(count_query).bind(language_id);

        if let Some(ref pattern) = search_pattern {
            items_q = items_q.bind(pattern);
            count_q = count_q.bind(pattern);
        }

        if let Some(ref created_before) = search.created_before {
            items_q = items_q.bind(created_before);
            count_q = count_q.bind(created_before);
        }

        if let Some(ref created_after) = search.created_after {
            items_q = items_q.bind(created_after);
            count_q = count_q.bind(created_after);
        }

        let items_future = items_q
            .bind(i64::from(pagination.limit))
            .bind(i64::from(pagination.offset))
            .fetch_all(&self.state.pool);

        let count_future = count_q.fetch_one(&self.state.pool);

        let (items, total) = tokio::try_join!(items_future, count_future)?;

        Ok((items, total))
    }

    pub async fn search(
        &self,
        language_id: &Uuid,
        pagination: PaginatedRequest,
        search: TranslationSearch,
    ) -> AppResult<PaginatedResponse<Translation>> {
        let (items_query, count_query, _param_count) =
            TranslationRepository::build_search_queries(&search);

        let (items, total) = self
            .execute_search_queries(
                &items_query,
                &count_query,
                language_id,
                &search,
                &pagination,
            )
            .await?;

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
}

crate::util::repo_from_parts!(TranslationRepository);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::audit_log::{
        AuditActionType, AuditLogFilter, AuditLogRepository, AuditableResource,
    };
    use crate::model::languages::{CreateLanguage, LanguageRepository};
    use crate::model::translatable::{CreateTranslatable, TranslatableRepository};
    use crate::model::users::UserRepository;
    use crate::pagination::PaginatedRequest;
    use crate::{config::CONFIG, create_router, email};
    use sqlx::PgPool;

    #[tokio::test]
    async fn test_create_translation_as_admin_creates_audit_log() {
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
        let _source_lang = lang_repo
            .create(
                &owner,
                CreateLanguage {
                    name: "Source Language".to_string(),
                    code: crate::tests::random_code(),
                    description: "Source language".to_string(),
                    private: false,
                },
            )
            .await
            .unwrap();

        let target_lang = lang_repo
            .create(
                &owner,
                CreateLanguage {
                    name: "Target Language".to_string(),
                    code: crate::tests::random_code(),
                    description: "Target language".to_string(),
                    private: false,
                },
            )
            .await
            .unwrap();

        // Create a translatable
        let translatable_repo = TranslatableRepository::new(app_state.clone());
        let translatable = translatable_repo
            .create(
                &owner,
                CreateTranslatable {
                    title: "hello world".to_string(),

                    english: "hello world".to_string(),

                    source_name: None,

                    source_url: None,

                    source_content: None,
                    source_language: None,
                    description: None,
                },
            )
            .await
            .unwrap();

        // Admin creates a translation (without permission)
        let trans_repo = TranslationRepository::new(app_state.clone());
        let translation = trans_repo
            .create(
                &admin,
                translatable.id,
                target_lang.id,
                CreateTranslation {
                    translated_text: "Bonjour le monde".to_string(),
                    translated_title: None,
                    ipa: None,
                    gloss: None,
                    notes: None,
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
                    resource_type: Some(AuditableResource::Translation),
                    resource_id: Some(translation.id),
                },
            )
            .await
            .unwrap();

        assert_eq!(logs.items.len(), 1);
        let log = &logs.items[0];
        assert_eq!(log.user_id, Some(admin.id));
        assert_eq!(log.action, AuditActionType::Created);
        assert_eq!(log.resource_type, AuditableResource::Translation);
        assert_eq!(log.resource_id, translation.id);
        assert_eq!(
            log.details["language_id"],
            serde_json::json!(target_lang.id)
        );
    }

    #[tokio::test]
    async fn test_update_translation_as_moderator_creates_audit_log() {
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
        let _source_lang = lang_repo
            .create(
                &owner,
                CreateLanguage {
                    name: "Source Language".to_string(),
                    code: crate::tests::random_code(),
                    description: "Source language".to_string(),
                    private: false,
                },
            )
            .await
            .unwrap();

        let target_lang = lang_repo
            .create(
                &owner,
                CreateLanguage {
                    name: "Target Language".to_string(),
                    code: crate::tests::random_code(),
                    description: "Target language".to_string(),
                    private: false,
                },
            )
            .await
            .unwrap();

        // Create a translatable
        let translatable_repo = TranslatableRepository::new(app_state.clone());
        let translatable = translatable_repo
            .create(
                &owner,
                CreateTranslatable {
                    title: "hello world".to_string(),

                    english: "hello world".to_string(),

                    source_name: None,

                    source_url: None,

                    source_content: None,
                    source_language: None,
                    description: None,
                },
            )
            .await
            .unwrap();

        let trans_repo = TranslationRepository::new(app_state.clone());
        let translation = trans_repo
            .create(
                &owner,
                translatable.id,
                target_lang.id,
                CreateTranslation {
                    translated_text: "Bonjour le monde".to_string(),
                    translated_title: None,
                    gloss: None,
                    ipa: None,
                    notes: None,
                },
            )
            .await
            .unwrap();

        // Moderator updates the translation (without permission)
        let updated = trans_repo
            .update(
                &moderator,
                translation.id,
                UpdateTranslation {
                    translated_text: Some("Salut le monde".to_string()),
                    translated_title: None,
                    gloss: None,
                    ipa: None,
                    notes: None,
                },
            )
            .await
            .unwrap();

        assert_eq!(updated.translated_text, "Salut le monde");

        // Check audit log was created
        let audit_repo = AuditLogRepository::new(app_state.clone());
        let logs = audit_repo
            .search(
                &moderator,
                PaginatedRequest::default(),
                AuditLogFilter {
                    user_id: Some(moderator.id),
                    action: Some(AuditActionType::Updated),
                    resource_type: Some(AuditableResource::Translation),
                    resource_id: Some(translation.id),
                },
            )
            .await
            .unwrap();

        assert_eq!(logs.items.len(), 1);
        let log = &logs.items[0];
        assert_eq!(log.user_id, Some(moderator.id));
        assert_eq!(log.action, AuditActionType::Updated);
        assert_eq!(log.resource_type, AuditableResource::Translation);
        assert_eq!(log.resource_id, translation.id);
        assert_eq!(
            log.details["language_id"],
            serde_json::json!(target_lang.id)
        );
    }

    #[tokio::test]
    async fn test_delete_translation_as_admin_creates_audit_log() {
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
        let _source_lang = lang_repo
            .create(
                &owner,
                CreateLanguage {
                    name: "Source Language".to_string(),
                    code: crate::tests::random_code(),
                    description: "Source language".to_string(),
                    private: false,
                },
            )
            .await
            .unwrap();

        let target_lang = lang_repo
            .create(
                &owner,
                CreateLanguage {
                    name: "Target Language".to_string(),
                    code: crate::tests::random_code(),
                    description: "Target language".to_string(),
                    private: false,
                },
            )
            .await
            .unwrap();

        // Create a translatable
        let translatable_repo = TranslatableRepository::new(app_state.clone());
        let translatable = translatable_repo
            .create(
                &owner,
                CreateTranslatable {
                    title: "hello world".to_string(),

                    english: "hello world".to_string(),

                    source_name: None,

                    source_url: None,

                    source_content: None,
                    source_language: None,
                    description: None,
                },
            )
            .await
            .unwrap();
        let trans_repo = TranslationRepository::new(app_state.clone());
        let translation = trans_repo
            .create(
                &owner,
                translatable.id,
                target_lang.id,
                CreateTranslation {
                    translated_text: "Bonjour le monde".to_string(),
                    translated_title: None,
                    gloss: None,
                    ipa: None,
                    notes: None,
                },
            )
            .await
            .unwrap();

        let trans_id = translation.id;

        // Admin deletes the translation (without permission)
        let deleted = trans_repo.delete(&admin, trans_id).await.unwrap();
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
                    resource_type: Some(AuditableResource::Translation),
                    resource_id: Some(trans_id),
                },
            )
            .await
            .unwrap();

        assert_eq!(logs.items.len(), 1);
        let log = &logs.items[0];
        assert_eq!(log.user_id, Some(admin.id));
        assert_eq!(log.action, AuditActionType::Deleted);
        assert_eq!(log.resource_type, AuditableResource::Translation);
        assert_eq!(log.resource_id, trans_id);
        assert_eq!(
            log.details["language_id"],
            serde_json::json!(target_lang.id)
        );
    }
}
