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
pub struct Quotation {
    pub id: Uuid,
    #[serde(skip_serializing)]
    pub translation: Uuid,
    #[allow(dead_code)]
    #[serde(skip_serializing)]
    pub definition: Uuid,
    pub span_start: i32,
    pub span_end: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[allow(dead_code)]
    #[serde(skip_serializing)]
    pub created_by: Uuid,
    #[allow(dead_code)]
    #[serde(skip_serializing)]
    pub updated_by: Uuid,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CreateQuotation {
    pub definition: Uuid,

    #[validate(range(min = 0))]
    pub span_start: i32,
    #[validate(range(min = 0))]
    pub span_end: i32,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct UpdateQuotation {
    #[validate(range(min = 0))]
    pub span_start: Option<i32>,
    #[validate(range(min = 0))]
    pub span_end: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct QuotationWithWordInfo {
    pub id: Uuid,
    pub span_start: i32,
    pub span_end: i32,
    pub definition_id: Uuid,
    pub definition_text: String,
    pub word_slug: String,
    pub word_lemma: i32,
    pub word: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct QuotationRepository {
    state: AppState,
}

impl QuotationRepository {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    pub async fn find_by_id(&self, id: Uuid) -> AppResult<Quotation> {
        let result = sqlx::query_as!(
            Quotation,
            r#"
                SELECT
                    id,
                    translation,
                    definition,
                    span_start,
                    span_end,
                    created_at,
                    updated_at,
                    created_by,
                    updated_by
                FROM quotation
                WHERE id = $1
            "#,
            id
        )
        .fetch_optional(&self.state.pool)
        .await?;

        result.ok_or_else(|| not_found(format!("quotation with id '{id}'")))
    }

    pub async fn create(
        &self,
        requestor: &User,
        translation_id: Uuid,
        definition_id: Uuid,
        quotation: CreateQuotation,
    ) -> AppResult<Quotation> {
        quotation.validate()?;

        ensure_verified(requestor)?;

        crate::model::user_bans::UserBanRepository::new(self.state.clone())
            .ensure_not_banned(requestor.id)
            .await?;

        // Get the translation to find its language
        let translation =
            crate::model::translations::TranslationRepository::new(self.state.clone())
                .find_by_id(translation_id)
                .await?;

        // Get the definition to find its word and language
        let definition = crate::model::definitions::DefinitionRepository::new(self.state.clone())
            .find_by_id(definition_id)
            .await?;
        let word = crate::model::words::WordRepository::new(self.state.clone())
            .find_by_id(definition.word)
            .await?;

        // Both the translation and definition must be for the same language
        if translation.language != word.language {
            return Err(bad_request(
                "translation and definition must be for the same language",
            ));
        }

        let mut tx = self.state.pool.begin().await?;

        // check for overlapping quotations
        let overlapping = sqlx::query!(
            r#"
                SELECT EXISTS (
                    SELECT 1 FROM quotation
                    WHERE span_start < $1 AND span_end > $2
                    AND translation = $3
                )
            "#,
            quotation.span_end,
            quotation.span_start,
            translation_id
        )
        .fetch_one(&mut *tx)
        .await?;

        if overlapping.exists.unwrap_or(false) {
            return Err(bad_request("quotation overlaps with an existing quotation"));
        }

        let result = sqlx::query_as!(
            Quotation,
            r#"
                INSERT INTO quotation (translation, definition, span_start, span_end, created_by, updated_by)
                VALUES ($1, $2, $3, $4, $5, $5)
                RETURNING id, translation, definition, span_start, span_end, created_at, updated_at, created_by, updated_by
            "#,
            translation_id,
            definition_id,
            quotation.span_start,
            quotation.span_end,
            requestor.id
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
                    language: translation.language,
                    required_level: PermissionLevel::Editor,
                    action_type: crate::model::audit_log::AuditActionType::Created,
                    resource_type: crate::model::audit_log::AuditableResource::Quotation,
                    resource_id: result.id,
                    context: Some(serde_json::json!({
                        "language_id": translation.language,
                        "translation_id": translation_id,
                        "definition_id": definition_id,
                        "span_start": quotation.span_start,
                        "span_end": quotation.span_end
                    })),
                },
                &mut tx,
            )
            .await?;

        if perm_check == crate::model::audit_log::PermissionCheck::NoPermission {
            return Err(bad_request(
                "you don't have permission to create quotations",
            ));
        }

        tx.commit().await?;

        Ok(result)
    }

    pub async fn update(
        &self,
        requestor: &User,
        id: Uuid,
        updates: UpdateQuotation,
    ) -> AppResult<Quotation> {
        updates.validate()?;

        ensure_verified(requestor)?;

        crate::model::user_bans::UserBanRepository::new(self.state.clone())
            .ensure_not_banned(requestor.id)
            .await?;

        // Get the quotation and check language permissions
        let existing = self.find_by_id(id).await?;
        let translation =
            crate::model::translations::TranslationRepository::new(self.state.clone())
                .find_by_id(existing.translation)
                .await?;

        let span_end = updates.span_end.unwrap_or(existing.span_end);
        let span_start = updates.span_start.unwrap_or(existing.span_start);

        if span_start >= span_end {
            return Err(bad_request("span_start must be less than span_end"));
        }

        if span_start < 0 || span_end < 0 {
            return Err(bad_request("span_start and span_end must be non-negative"));
        }

        if span_end
            > translation
                .translated_text
                .chars()
                .count()
                .try_into()
                .unwrap_or(i32::MAX)
        {
            return Err(bad_request("span_end exceeds length of translated text"));
        }

        let mut tx = self.state.pool.begin().await?;

        let permissions = crate::model::language_permissions::LanguagePermissionRepository::new(
            self.state.clone(),
        );
        let perm_check = permissions
            .check_permission_with_audit(
                crate::model::language_permissions::CheckPermissionReq {
                    user: requestor.id,
                    language: translation.language,
                    required_level: PermissionLevel::Editor,
                    action_type: crate::model::audit_log::AuditActionType::Updated,
                    resource_type: crate::model::audit_log::AuditableResource::Quotation,
                    resource_id: id,
                    context: Some(serde_json::json!({
                        "language_id": translation.language,
                        "translation_id": existing.translation,
                        "updates": updates
                    })),
                },
                &mut tx,
            )
            .await?;

        if perm_check == crate::model::audit_log::PermissionCheck::NoPermission {
            return Err(bad_request("you don't have permission to edit quotations"));
        }

        // check for overlapping quotations
        let overlapping = sqlx::query!(
            r#"
                SELECT EXISTS (
                    SELECT 1 FROM quotation
                    WHERE span_start < $1 AND span_end > $2
                    AND translation = $3
                    AND id <> $4
                )
            "#,
            span_end,
            span_start,
            existing.translation,
            id
        )
        .fetch_one(&mut *tx)
        .await?;

        if overlapping.exists.unwrap_or(false) {
            return Err(bad_request("quotation overlaps with an existing quotation"));
        }

        let result = sqlx::query_as!(
            Quotation,
            r#"
                UPDATE quotation
                SET span_start = COALESCE($2, span_start),
                    span_end = COALESCE($3, span_end),
                    updated_by = $4,
                    updated_at = CURRENT_TIMESTAMP
                WHERE id = $1
                RETURNING id, translation, definition, span_start, span_end, created_at, updated_at, created_by, updated_by
            "#,
            id,
            updates.span_start,
            updates.span_end,
            requestor.id
        )
        .fetch_optional(&mut *tx)
        .await?;

        let quotation_result =
            result.ok_or_else(|| not_found(format!("quotation with id '{id}'")))?;

        tx.commit().await?;

        Ok(quotation_result)
    }

    pub async fn delete(&self, requestor: &User, id: Uuid) -> AppResult<bool> {
        ensure_verified(requestor)?;

        crate::model::user_bans::UserBanRepository::new(self.state.clone())
            .ensure_not_banned(requestor.id)
            .await?;

        // Get the quotation and check language permissions
        let existing = self.find_by_id(id).await?;
        let translation =
            crate::model::translations::TranslationRepository::new(self.state.clone())
                .find_by_id(existing.translation)
                .await?;

        let mut tx = self.state.pool.begin().await?;

        let permissions = crate::model::language_permissions::LanguagePermissionRepository::new(
            self.state.clone(),
        );
        let perm_check = permissions
            .check_permission_with_audit(
                crate::model::language_permissions::CheckPermissionReq {
                    user: requestor.id,
                    language: translation.language,
                    required_level: PermissionLevel::Editor,
                    action_type: crate::model::audit_log::AuditActionType::Deleted,
                    resource_type: crate::model::audit_log::AuditableResource::Quotation,
                    resource_id: id,
                    context: Some(serde_json::json!({
                        "language_id": translation.language,
                        "translation_id": existing.translation,
                        "span_start": existing.span_start,
                        "span_end": existing.span_end
                    })),
                },
                &mut tx,
            )
            .await?;

        if perm_check == crate::model::audit_log::PermissionCheck::NoPermission {
            return Err(bad_request(
                "you don't have permission to delete quotations",
            ));
        }

        let result = sqlx::query!("DELETE FROM quotation WHERE id = $1", id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;

        Ok(result.rows_affected() > 0)
    }

    #[allow(dead_code)]
    pub async fn list_by_translation(
        &self,
        translation_id: Uuid,
        pagination: PaginatedRequest,
    ) -> AppResult<PaginatedResponse<Quotation>> {
        let items_future = sqlx::query_as!(
            Quotation,
            r#"
                SELECT
                    id,
                    translation,
                    definition,
                    span_start,
                    span_end,
                    created_at,
                    updated_at,
                    created_by,
                    updated_by
                FROM quotation
                WHERE translation = $1
                ORDER BY span_start ASC
                LIMIT $2
                OFFSET $3
            "#,
            translation_id,
            i64::from(pagination.limit),
            i64::from(pagination.offset)
        )
        .fetch_all(&self.state.pool);

        let count_future = sqlx::query_scalar!(
            r#"
                SELECT COUNT(*)
                FROM quotation
                WHERE translation = $1
            "#,
            translation_id
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

    pub async fn list_by_translation_with_word_info(
        &self,
        translation_id: Uuid,
        pagination: PaginatedRequest,
    ) -> AppResult<PaginatedResponse<QuotationWithWordInfo>> {
        let items_future = sqlx::query_as!(
            QuotationWithWordInfo,
            r#"
                SELECT
                    q.id,
                    q.span_start,
                    q.span_end,
                    d.id         AS definition_id,
                    d.definition AS definition_text,
                    w.slug       AS word_slug,
                    w.lemma      AS word_lemma,
                    w.word       AS word,
                    q.created_at,
                    q.updated_at
                FROM quotation q
                JOIN definitions d ON d.id = q.definition
                JOIN words w       ON w.id = d.word
                WHERE q.translation = $1
                ORDER BY q.span_start ASC
                LIMIT $2
                OFFSET $3
            "#,
            translation_id,
            i64::from(pagination.limit),
            i64::from(pagination.offset)
        )
        .fetch_all(&self.state.pool);

        let count_future = sqlx::query_scalar!(
            r#"
                SELECT COUNT(*)
                FROM quotation
                WHERE translation = $1
            "#,
            translation_id
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

    pub async fn list_by_definition(
        &self,
        definition_id: Uuid,
        pagination: PaginatedRequest,
    ) -> AppResult<PaginatedResponse<Quotation>> {
        let items_future = sqlx::query_as!(
            Quotation,
            r#"
                SELECT
                    id,
                    translation,
                    definition,
                    span_start,
                    span_end,
                    created_at,
                    updated_at,
                    created_by,
                    updated_by
                FROM quotation
                WHERE definition = $1
                ORDER BY created_at DESC
                LIMIT $2
                OFFSET $3
            "#,
            definition_id,
            i64::from(pagination.limit),
            i64::from(pagination.offset)
        )
        .fetch_all(&self.state.pool);

        let count_future = sqlx::query_scalar!(
            r#"
                SELECT COUNT(*)
                FROM quotation
                WHERE definition = $1
            "#,
            definition_id
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
}

crate::util::repo_from_parts!(QuotationRepository);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::audit_log::{
        AuditActionType, AuditLogFilter, AuditLogRepository, AuditableResource,
    };
    use crate::model::definitions::{CreateDefinition, DefinitionRepository};
    use crate::model::languages::{CreateLanguage, LanguageRepository};
    use crate::model::translatable::{CreateTranslatable, TranslatableRepository};
    use crate::model::translations::{CreateTranslation, TranslationRepository};
    use crate::model::users::UserRepository;
    use crate::model::words::{CreateWord, WordRepository};
    use crate::pagination::PaginatedRequest;
    use crate::{config::CONFIG, create_router, email};
    use sqlx::PgPool;

    #[tokio::test]
    async fn test_create_quotation_as_admin_creates_audit_log() {
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

        let def_repo = DefinitionRepository::new(app_state.clone());
        let definition = def_repo
            .create(
                &owner,
                word.id,
                CreateDefinition {
                    definition: "test definition text here for quotation".to_string(),
                    context: None,
                },
            )
            .await
            .unwrap();

        // Create a translatable and translation
        let trans_repo = TranslatableRepository::new(app_state.clone());
        let translatable = trans_repo
            .create(
                &owner,
                CreateTranslatable {
                    title: "Test Translatable".to_string(),
                    english: "This is a test sentence for quotation testing.".to_string(),
                    source_name: None,
                    source_url: None,
                    source_content: None,
                    source_language: None,
                    description: None,
                },
            )
            .await
            .unwrap();

        let translation_repo = TranslationRepository::new(app_state.clone());
        let translation = translation_repo
            .create(
                &owner,
                translatable.id,
                lang.id,
                CreateTranslation {
                    translated_text: "Test translation text for quotation".to_string(),
                    translated_title: None,
                    ipa: None,
                    gloss: None,
                    notes: None,
                },
            )
            .await
            .unwrap();

        // Admin creates a quotation (without permission)
        let quot_repo = QuotationRepository::new(app_state.clone());
        let quotation = quot_repo
            .create(
                &admin,
                translation.id,
                definition.id,
                CreateQuotation {
                    definition: definition.id,
                    span_start: 0,
                    span_end: 10,
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
                    resource_type: Some(AuditableResource::Quotation),
                    resource_id: Some(quotation.id),
                },
            )
            .await
            .unwrap();

        assert_eq!(logs.items.len(), 1);
        let log = &logs.items[0];
        assert_eq!(log.user_id, Some(admin.id));
        assert_eq!(log.action, AuditActionType::Created);
        assert_eq!(log.resource_type, AuditableResource::Quotation);
        assert_eq!(log.resource_id, quotation.id);
        assert_eq!(log.details["language_id"], serde_json::json!(lang.id));
    }

    #[tokio::test]
    async fn test_update_quotation_as_moderator_creates_audit_log() {
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

        let def_repo = DefinitionRepository::new(app_state.clone());
        let definition = def_repo
            .create(
                &owner,
                word.id,
                CreateDefinition {
                    definition: "test definition text here for quotation".to_string(),
                    context: None,
                },
            )
            .await
            .unwrap();

        // Create a translatable and translation
        let trans_repo = TranslatableRepository::new(app_state.clone());
        let translatable = trans_repo
            .create(
                &owner,
                CreateTranslatable {
                    title: "Test Translatable".to_string(),
                    english: "This is a test sentence for quotation testing.".to_string(),
                    source_name: None,
                    source_url: None,
                    source_content: None,
                    source_language: None,
                    description: None,
                },
            )
            .await
            .unwrap();

        let translation_repo = TranslationRepository::new(app_state.clone());
        let translation = translation_repo
            .create(
                &owner,
                translatable.id,
                lang.id,
                CreateTranslation {
                    translated_text: "Test translation text for quotation".to_string(),
                    translated_title: None,
                    ipa: None,
                    gloss: None,
                    notes: None,
                },
            )
            .await
            .unwrap();

        let quot_repo = QuotationRepository::new(app_state.clone());
        let quotation = quot_repo
            .create(
                &owner,
                translation.id,
                definition.id,
                CreateQuotation {
                    definition: definition.id,
                    span_start: 0,
                    span_end: 10,
                },
            )
            .await
            .unwrap();

        // Moderator updates the quotation (without permission)
        let updated = quot_repo
            .update(
                &moderator,
                quotation.id,
                UpdateQuotation {
                    span_start: Some(5),
                    span_end: Some(15),
                },
            )
            .await
            .unwrap();

        assert_eq!(updated.span_start, 5);
        assert_eq!(updated.span_end, 15);

        // Check audit log was created
        let audit_repo = AuditLogRepository::new(app_state.clone());
        let logs = audit_repo
            .search(
                &moderator,
                PaginatedRequest::default(),
                AuditLogFilter {
                    user_id: Some(moderator.id),
                    action: Some(AuditActionType::Updated),
                    resource_type: Some(AuditableResource::Quotation),
                    resource_id: Some(quotation.id),
                },
            )
            .await
            .unwrap();

        assert_eq!(logs.items.len(), 1);
        let log = &logs.items[0];
        assert_eq!(log.user_id, Some(moderator.id));
        assert_eq!(log.action, AuditActionType::Updated);
        assert_eq!(log.resource_type, AuditableResource::Quotation);
        assert_eq!(log.resource_id, quotation.id);
        assert_eq!(log.details["language_id"], serde_json::json!(lang.id));
    }

    #[tokio::test]
    async fn test_delete_quotation_as_admin_creates_audit_log() {
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

        let def_repo = DefinitionRepository::new(app_state.clone());
        let definition = def_repo
            .create(
                &owner,
                word.id,
                CreateDefinition {
                    definition: "test definition text here for quotation".to_string(),
                    context: None,
                },
            )
            .await
            .unwrap();

        // Create a translatable and translation
        let trans_repo = TranslatableRepository::new(app_state.clone());
        let translatable = trans_repo
            .create(
                &owner,
                CreateTranslatable {
                    title: "Test Translatable".to_string(),
                    english: "This is a test sentence for quotation testing.".to_string(),
                    source_name: None,
                    source_url: None,
                    source_content: None,
                    source_language: None,
                    description: None,
                },
            )
            .await
            .unwrap();

        let translation_repo = TranslationRepository::new(app_state.clone());
        let translation = translation_repo
            .create(
                &owner,
                translatable.id,
                lang.id,
                CreateTranslation {
                    translated_text: "Test translation text for quotation".to_string(),
                    translated_title: None,
                    ipa: None,
                    gloss: None,
                    notes: None,
                },
            )
            .await
            .unwrap();

        let quot_repo = QuotationRepository::new(app_state.clone());
        let quotation = quot_repo
            .create(
                &owner,
                translation.id,
                definition.id,
                CreateQuotation {
                    definition: definition.id,
                    span_start: 0,
                    span_end: 10,
                },
            )
            .await
            .unwrap();

        let quot_id = quotation.id;

        // Admin deletes the quotation (without permission)
        let deleted = quot_repo.delete(&admin, quot_id).await.unwrap();
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
                    resource_type: Some(AuditableResource::Quotation),
                    resource_id: Some(quot_id),
                },
            )
            .await
            .unwrap();

        assert_eq!(logs.items.len(), 1);
        let log = &logs.items[0];
        assert_eq!(log.user_id, Some(admin.id));
        assert_eq!(log.action, AuditActionType::Deleted);
        assert_eq!(log.resource_type, AuditableResource::Quotation);
        assert_eq!(log.resource_id, quot_id);
        assert_eq!(log.details["language_id"], serde_json::json!(lang.id));
    }
}
