use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Type, prelude::FromRow};
use uuid::Uuid;

use crate::{
    err::{AppResult, bad_request, forbidden, internal_error},
    model::{
        language_families::{LanguageFamily, LanguageFamilyRepository},
        language_family_permissions::LanguageFamilyPermissionRepository,
        language_invites::PermissionLevel,
        language_permissions::LanguagePermissionRepository,
        languages::{Language, LanguageRepository},
        users::{User, UserRepository},
    },
    pagination::{PaginatedRequest, PaginatedResponse},
    util::{AppState, repo_from_parts},
};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type, PartialEq, Eq)]
#[sqlx(type_name = "language_family_relation_type", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum LanguageFamilyRelationType {
    Descendant,
    Hybrid,
}

// Private row struct for SQLx deserialization
#[derive(Debug, FromRow, Clone)]
struct LanguageFamilyMemberRow {
    pub id: Uuid,
    pub family_id: Uuid,
    pub language_id: Option<Uuid>,
    pub title: String,
    pub parent_member_id: Option<Uuid>,
    pub relation_type: LanguageFamilyRelationType,
    pub notes: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub created_by: Uuid,
    pub updated_by: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguageMember {
    pub id: Uuid,
    pub family_id: Uuid,
    pub language_id: Uuid,
    pub parent_member_id: Option<Uuid>,
    pub relation_type: LanguageFamilyRelationType,
    pub notes: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub created_by: Uuid,
    pub updated_by: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Grouping {
    pub id: Uuid,
    pub family_id: Uuid,
    pub title: String,
    pub parent_member_id: Option<Uuid>,
    pub notes: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub created_by: Uuid,
    pub updated_by: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LanguageFamilyMember {
    Language(LanguageMember),
    Grouping(Grouping),
}

#[allow(dead_code)]
impl LanguageFamilyMember {
    pub fn id(&self) -> Uuid {
        match self {
            Self::Language(m) => m.id,
            Self::Grouping(g) => g.id,
        }
    }

    pub fn family_id(&self) -> Uuid {
        match self {
            Self::Language(m) => m.family_id,
            Self::Grouping(g) => g.family_id,
        }
    }

    pub fn parent_member_id(&self) -> Option<Uuid> {
        match self {
            Self::Language(m) => m.parent_member_id,
            Self::Grouping(g) => g.parent_member_id,
        }
    }

    pub fn notes(&self) -> &str {
        match self {
            Self::Language(m) => &m.notes,
            Self::Grouping(g) => &g.notes,
        }
    }

    pub fn created_by(&self) -> Uuid {
        match self {
            Self::Language(m) => m.created_by,
            Self::Grouping(g) => g.created_by,
        }
    }

    pub fn updated_by(&self) -> Uuid {
        match self {
            Self::Language(m) => m.updated_by,
            Self::Grouping(g) => g.updated_by,
        }
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        match self {
            Self::Language(m) => m.created_at,
            Self::Grouping(g) => g.created_at,
        }
    }

    pub fn updated_at(&self) -> DateTime<Utc> {
        match self {
            Self::Language(m) => m.updated_at,
            Self::Grouping(g) => g.updated_at,
        }
    }

    pub fn as_language(&self) -> Option<&LanguageMember> {
        if let Self::Language(data) = self {
            Some(data)
        } else {
            None
        }
    }

    pub fn as_grouping(&self) -> Option<&Grouping> {
        if let Self::Grouping(data) = self {
            Some(data)
        } else {
            None
        }
    }
}

impl TryFrom<LanguageFamilyMemberRow> for LanguageFamilyMember {
    type Error = crate::err::AppError;

    fn try_from(row: LanguageFamilyMemberRow) -> AppResult<Self> {
        if let Some(lid) = row.language_id {
            Ok(Self::Language(LanguageMember {
                id: row.id,
                family_id: row.family_id,
                language_id: lid,
                parent_member_id: row.parent_member_id,
                relation_type: row.relation_type,
                notes: row.notes,
                created_at: row.created_at,
                updated_at: row.updated_at,
                created_by: row.created_by,
                updated_by: row.updated_by,
            }))
        } else {
            let title = if row.title.is_empty() {
                return Err(internal_error("grouping missing title"));
            } else {
                row.title
            };
            Ok(Self::Grouping(Grouping {
                id: row.id,
                family_id: row.family_id,
                title,
                parent_member_id: row.parent_member_id,
                notes: row.notes,
                created_at: row.created_at,
                updated_at: row.updated_at,
                created_by: row.created_by,
                updated_by: row.updated_by,
            }))
        }
    }
}

#[derive(Serialize, Clone)]
pub struct MemberWithLanguages {
    pub member: LanguageFamilyMember,
    pub language: Option<Language>,
    pub parent_language: Option<Language>,
    pub family: LanguageFamily,
    pub creator: User,
    pub updater: User,
}

impl MemberWithLanguages {
    pub fn name(&self) -> String {
        match &self.member {
            LanguageFamilyMember::Language(_) => self
                .language
                .as_ref()
                .map_or_else(|| "unknown".into(), |l| l.name.clone()),
            LanguageFamilyMember::Grouping(g) => g.title.clone(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct CreateLanguageFamilyMember {
    pub language_code: Option<String>,
    pub title: Option<String>,
    pub relation_type: LanguageFamilyRelationType,
    pub notes: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SearchLanguageFamilyMembers {
    pub family_code: Option<String>,
    pub parent_language_code: Option<String>,
    pub parent_member_id: Option<Uuid>,
    pub language_code: Option<String>,
    pub relation_type: Option<LanguageFamilyRelationType>,
    pub q: Option<String>, // name, code, description, notes, title
}

pub struct LanguageFamilyMemberRepository {
    state: AppState,
}

impl LanguageFamilyMemberRepository {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    pub async fn find_by_id(&self, id: Uuid) -> AppResult<LanguageFamilyMember> {
        let row = sqlx::query_as!(
            LanguageFamilyMemberRow,
            r#"
                SELECT id, family_id, language_id, title, parent_member_id, relation_type as "relation_type: LanguageFamilyRelationType", created_at, updated_at, created_by, updated_by, notes
                FROM language_family_members
                WHERE id = $1
            "#,
            id
        )
        .fetch_one(&self.state.pool)
        .await?;

        row.try_into()
    }

    pub async fn materialize(
        &self,
        member: LanguageFamilyMember,
    ) -> AppResult<MemberWithLanguages> {
        let languages = LanguageRepository::new(self.state.clone());

        let language = if let LanguageFamilyMember::Language(ref data) = member {
            Some(languages.find_by_id(data.language_id).await?)
        } else {
            None
        };

        let parent_language = if let Some(parent_member_id) = member.parent_member_id() {
            let parent_member = self.find_by_id(parent_member_id).await?;
            if let LanguageFamilyMember::Language(ref data) = parent_member {
                Some(languages.find_by_id(data.language_id).await?)
            } else {
                None
            }
        } else {
            None
        };

        let family = LanguageFamilyRepository::new(self.state.clone())
            .find_by_id(member.family_id())
            .await?;

        let users = UserRepository::new(self.state.clone());

        let creator = users.find_by_id(member.created_by()).await?;

        let updater = users.find_by_id(member.updated_by()).await?;

        Ok(MemberWithLanguages {
            member,
            language,
            parent_language,
            family,
            creator,
            updater,
        })
    }

    pub async fn find_by_family_and_language(
        &self,
        family_id: Uuid,
        language_id: Uuid,
    ) -> AppResult<Option<LanguageFamilyMember>> {
        let row = sqlx::query_as!(
            LanguageFamilyMemberRow,
            r#"
                SELECT id, family_id, language_id, title, parent_member_id, relation_type as "relation_type: LanguageFamilyRelationType", created_at, updated_at, created_by, updated_by, notes
                FROM language_family_members
                WHERE family_id = $1 AND language_id = $2
            "#,
            family_id,
            language_id
        )
        .fetch_optional(&self.state.pool)
        .await?;

        row.map(LanguageFamilyMember::try_from).transpose()
    }

    pub async fn create(
        &self,
        requestor: User,
        family: LanguageFamily,
        parent_id: Option<Uuid>,
        member: CreateLanguageFamilyMember,
    ) -> AppResult<LanguageFamilyMember> {
        use crate::model::audit_log::{AuditActionType, AuditableResource, PermissionCheck};
        use crate::model::language_family_permissions::CheckPermissionReq as FamilyCheckPermissionReq;
        use crate::model::language_permissions::CheckPermissionReq as LanguageCheckPermissionReq;

        crate::model::user_bans::UserBanRepository::new(self.state.clone())
            .ensure_not_banned(requestor.id)
            .await?;

        let language = if let Some(ref code) = member.language_code {
            let languages = LanguageRepository::new(self.state.clone());
            Some(languages.find_by_code(code).await?)
        } else {
            None
        };

        // For groupings, validate that a title is provided
        if language.is_none() && member.title.as_ref().map_or(true, |t| t.is_empty()) {
            return Err(bad_request("grouping requires a title"));
        }

        let parent_member_id = if let Some(parent_id) = &parent_id {
            let parent_member = self.find_by_id(*parent_id).await?;

            if parent_member.family_id() != family.id
                && member.relation_type != LanguageFamilyRelationType::Hybrid
            {
                return Err(forbidden(
                    "parent member does not belong to the same family",
                ));
            }

            Some(parent_member.id())
        } else {
            let existing_root = self.find_root(family.id).await?;

            if existing_root.is_some() {
                return Err(forbidden("a root member already exists for this family"));
            }

            None
        };

        let title = if language.is_none() {
            member.title.as_deref().unwrap_or("")
        } else {
            ""
        };

        let mut tx = self.state.pool.begin().await?;

        let row = sqlx::query_as!(
            LanguageFamilyMemberRow,
            r#"
                INSERT INTO language_family_members (family_id, language_id, title, parent_member_id, relation_type, notes, created_by, updated_by)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $7)
                RETURNING id, family_id, language_id, title, parent_member_id, relation_type as "relation_type: LanguageFamilyRelationType", created_at, updated_at, created_by, updated_by, notes
            "#,
            family.id,
            language.as_ref().map(|l| l.id),
            title,
            parent_member_id,
            member.relation_type as LanguageFamilyRelationType,
            member.notes.unwrap_or_default(),
            requestor.id
        )
        .fetch_one(&mut *tx)
        .await?;

        let result: LanguageFamilyMember = row.try_into()?;

        let family_permissions = LanguageFamilyPermissionRepository::new(self.state.clone());
        let family_perm = family_permissions
            .check_permission_with_audit(
                FamilyCheckPermissionReq {
                    user: requestor.id,
                    family: family.id,
                    required_level: PermissionLevel::Editor,
                    action_type: AuditActionType::Created,
                    resource_type: AuditableResource::LanguageFamilyMember,
                    resource_id: result.id(),
                    context: Some(serde_json::json!({
                        "family_id": family.id,
                        "language_id": language.as_ref().map(|l| l.id),
                        "is_grouping": language.is_none(),
                    })),
                },
                &mut tx,
            )
            .await?;

        if let Some(ref lang) = language {
            let language_permissions = LanguagePermissionRepository::new(self.state.clone());
            let language_perm = language_permissions
                .check_permission_with_audit(
                    LanguageCheckPermissionReq {
                        user: requestor.id,
                        language: lang.id,
                        required_level: PermissionLevel::Editor,
                        action_type: AuditActionType::Created,
                        resource_type: AuditableResource::LanguageFamilyMember,
                        resource_id: result.id(),
                        context: Some(serde_json::json!({
                            "family_id": family.id,
                            "language_id": lang.id,
                        })),
                    },
                    &mut tx,
                )
                .await?;

            if family_perm == PermissionCheck::NoPermission
                || language_perm == PermissionCheck::NoPermission
            {
                return Err(forbidden(
                    "user lacks permission to add member to language family",
                ));
            }
        } else if family_perm == PermissionCheck::NoPermission {
            return Err(forbidden(
                "user lacks permission to add member to language family",
            ));
        }

        LanguageFamilyRepository::new(self.state.clone())
            .add_to_tree(family, result.clone(), &mut tx)
            .await?;

        tx.commit().await?;

        Ok(result)
    }

    pub async fn find_root(&self, family_id: Uuid) -> AppResult<Option<LanguageFamilyMember>> {
        let row = sqlx::query_as!(
            LanguageFamilyMemberRow,
            r#"
                SELECT id, family_id, language_id, title, parent_member_id, relation_type as "relation_type: LanguageFamilyRelationType", created_at, updated_at, created_by, updated_by, notes
                FROM language_family_members
                WHERE family_id = $1 AND parent_member_id IS NULL
            "#,
            family_id
        )
        .fetch_optional(&self.state.pool)
        .await?;

        row.map(LanguageFamilyMember::try_from).transpose()
    }

    pub async fn delete(&self, requestor: &User, member_id: Uuid) -> AppResult<()> {
        use crate::model::audit_log::{AuditActionType, AuditableResource, PermissionCheck};
        use crate::model::language_family_permissions::CheckPermissionReq;

        crate::model::user_bans::UserBanRepository::new(self.state.clone())
            .ensure_not_banned(requestor.id)
            .await?;

        let existing_member = self.find_by_id(member_id).await?;

        let language_id = existing_member.as_language().map(|data| data.language_id);

        let mut tx = self.state.pool.begin().await?;

        // Check permission on family with audit
        let family_permissions = LanguageFamilyPermissionRepository::new(self.state.clone());
        let family_perm = family_permissions
            .check_permission_with_audit(
                CheckPermissionReq {
                    user: requestor.id,
                    family: existing_member.family_id(),
                    required_level: PermissionLevel::Editor,
                    action_type: AuditActionType::Deleted,
                    resource_type: AuditableResource::LanguageFamilyMember,
                    resource_id: member_id,
                    context: Some(serde_json::json!({
                        "family_id": existing_member.family_id(),
                        "language_id": language_id,
                    })),
                },
                &mut tx,
            )
            .await?;

        if family_perm == PermissionCheck::NoPermission {
            return Err(forbidden(
                "user lacks permission to delete language family member",
            ));
        }

        let row = sqlx::query_as!(
            LanguageFamilyMemberRow,
            r#"
                DELETE FROM language_family_members
                WHERE id = $1
                RETURNING id, family_id, language_id, title, parent_member_id, relation_type as "relation_type: LanguageFamilyRelationType", created_at, updated_at, created_by, updated_by, notes
            "#,
            member_id
        )
        .fetch_one(&mut *tx)
        .await?;

        let member: LanguageFamilyMember = row.try_into()?;

        let families = LanguageFamilyRepository::new(self.state.clone());

        let family = families.find_by_id(member.family_id()).await?;

        let _ = families.remove_from_tree(family, member, &mut tx).await?;

        tx.commit().await?;

        Ok(())
    }

    pub async fn convert_to_grouping(
        &self,
        requestor: &User,
        member_id: Uuid,
        title: String,
        notes: String,
    ) -> AppResult<LanguageFamilyMember> {
        use crate::model::audit_log::{AuditActionType, AuditableResource, PermissionCheck};
        use crate::model::language_family_permissions::CheckPermissionReq;

        crate::model::user_bans::UserBanRepository::new(self.state.clone())
            .ensure_not_banned(requestor.id)
            .await?;

        let existing = self.find_by_id(member_id).await?;

        let language_data = match &existing {
            LanguageFamilyMember::Language(data) => data.clone(),
            LanguageFamilyMember::Grouping(_) => {
                return Err(crate::err::bad_request("member is already a grouping"));
            }
        };

        if title.trim().is_empty() {
            return Err(crate::err::bad_request(
                "grouping requires a non-empty title",
            ));
        }

        let mut tx = self.state.pool.begin().await?;

        let family_permissions = LanguageFamilyPermissionRepository::new(self.state.clone());
        let family_perm = family_permissions
            .check_permission_with_audit(
                CheckPermissionReq {
                    user: requestor.id,
                    family: existing.family_id(),
                    required_level: PermissionLevel::Editor,
                    action_type: AuditActionType::Updated,
                    resource_type: AuditableResource::LanguageFamilyMember,
                    resource_id: member_id,
                    context: Some(serde_json::json!({
                        "action": "convert_to_grouping",
                        "title": title,
                    })),
                },
                &mut tx,
            )
            .await?;

        if family_perm == PermissionCheck::NoPermission {
            return Err(forbidden(
                "user lacks permission to edit this language family member",
            ));
        }

        let row = sqlx::query_as!(
            LanguageFamilyMemberRow,
            r#"
                UPDATE language_family_members
                SET language_id = NULL,
                    title = $1,
                    notes = $2,
                    relation_type = 'descendant',
                    updated_at = CURRENT_TIMESTAMP,
                    updated_by = $3
                WHERE id = $4
                RETURNING id, family_id, language_id, title, parent_member_id, relation_type as "relation_type: LanguageFamilyRelationType", created_at, updated_at, created_by, updated_by, notes
            "#,
            title,
            notes,
            requestor.id,
            member_id
        )
        .fetch_one(&mut *tx)
        .await?;

        let result: LanguageFamilyMember = row.try_into()?;

        let family = LanguageFamilyRepository::new(self.state.clone())
            .find_by_id(existing.family_id())
            .await?;

        LanguageFamilyRepository::new(self.state.clone())
            .rebuild_member_in_tree(
                family,
                member_id,
                language_data.parent_member_id,
                language_data.family_id,
                &mut tx,
            )
            .await?;

        tx.commit().await?;

        Ok(result)
    }

    pub async fn convert_to_language(
        &self,
        requestor: &User,
        member_id: Uuid,
        language_code: String,
    ) -> AppResult<LanguageFamilyMember> {
        use crate::model::audit_log::{AuditActionType, AuditableResource, PermissionCheck};
        use crate::model::language_family_permissions::CheckPermissionReq as FamilyCheckPermissionReq;
        use crate::model::language_permissions::CheckPermissionReq as LanguageCheckPermissionReq;

        crate::model::user_bans::UserBanRepository::new(self.state.clone())
            .ensure_not_banned(requestor.id)
            .await?;

        let existing = self.find_by_id(member_id).await?;

        match &existing {
            LanguageFamilyMember::Language(_) => {
                return Err(crate::err::bad_request("member is already a language node"));
            }
            LanguageFamilyMember::Grouping(_) => {}
        }

        let languages = LanguageRepository::new(self.state.clone());
        let language = languages.find_by_code(&language_code).await?;

        // Check language not already in this family
        if self
            .find_by_family_and_language(existing.family_id(), language.id)
            .await?
            .is_some()
        {
            return Err(crate::err::bad_request(
                "this language is already a member of this family",
            ));
        }

        // Check language doesn't already have a descendant relation in any family
        let has_descendant = sqlx::query!(
            r#"SELECT id FROM language_family_members WHERE language_id = $1 AND relation_type = 'descendant' LIMIT 1"#,
            language.id
        )
        .fetch_optional(&self.state.pool)
        .await?;

        if has_descendant.is_some() {
            return Err(crate::err::bad_request(
                "this language already belongs to a family tree as a descendant",
            ));
        }

        let mut tx = self.state.pool.begin().await?;

        let family_permissions = LanguageFamilyPermissionRepository::new(self.state.clone());
        let family_perm = family_permissions
            .check_permission_with_audit(
                FamilyCheckPermissionReq {
                    user: requestor.id,
                    family: existing.family_id(),
                    required_level: PermissionLevel::Editor,
                    action_type: AuditActionType::Updated,
                    resource_type: AuditableResource::LanguageFamilyMember,
                    resource_id: member_id,
                    context: Some(serde_json::json!({
                        "action": "convert_to_language",
                        "language_id": language.id,
                    })),
                },
                &mut tx,
            )
            .await?;

        let language_permissions = LanguagePermissionRepository::new(self.state.clone());
        let language_perm = language_permissions
            .check_permission_with_audit(
                LanguageCheckPermissionReq {
                    user: requestor.id,
                    language: language.id,
                    required_level: PermissionLevel::Editor,
                    action_type: AuditActionType::Updated,
                    resource_type: AuditableResource::LanguageFamilyMember,
                    resource_id: member_id,
                    context: Some(serde_json::json!({
                        "action": "convert_to_language",
                        "family_id": existing.family_id(),
                    })),
                },
                &mut tx,
            )
            .await?;

        if family_perm == PermissionCheck::NoPermission
            || language_perm == PermissionCheck::NoPermission
        {
            return Err(forbidden(
                "user lacks permission to convert this member to a language node",
            ));
        }

        let row = sqlx::query_as!(
            LanguageFamilyMemberRow,
            r#"
                UPDATE language_family_members
                SET language_id = $1,
                    title = '',
                    relation_type = 'descendant',
                    updated_at = CURRENT_TIMESTAMP,
                    updated_by = $2
                WHERE id = $3
                RETURNING id, family_id, language_id, title, parent_member_id, relation_type as "relation_type: LanguageFamilyRelationType", created_at, updated_at, created_by, updated_by, notes
            "#,
            language.id,
            requestor.id,
            member_id
        )
        .fetch_one(&mut *tx)
        .await?;

        let result: LanguageFamilyMember = row.try_into()?;

        tx.commit().await?;

        Ok(result)
    }

    pub async fn count_by_family(&self, family_id: Uuid) -> AppResult<i64> {
        let count = sqlx::query_scalar!(
            r#"
                SELECT COUNT(*)
                FROM language_family_members
                WHERE family_id = $1
            "#,
            family_id
        )
        .fetch_one(&self.state.pool)
        .await?;

        Ok(count.unwrap_or(0))
    }

    pub async fn search(
        &self,
        query: SearchLanguageFamilyMembers,
        pagination: PaginatedRequest,
    ) -> AppResult<PaginatedResponse<LanguageFamilyMember>> {
        let rows_future = sqlx::query_as!(
            LanguageFamilyMemberRow,
            r#"
                SELECT
                    lfm.id,
                    lfm.family_id,
                    lfm.language_id,
                    lfm.title,
                    lfm.parent_member_id,
                    lfm.relation_type as "relation_type: LanguageFamilyRelationType",
                    lfm.created_at,
                    lfm.updated_at,
                    lfm.created_by,
                    lfm.updated_by,
                    lfm.notes
                FROM language_family_members lfm
                JOIN language_families lf ON lf.id = lfm.family_id
                LEFT JOIN languages l ON l.id = lfm.language_id
                LEFT JOIN language_family_members pmpl ON pmpl.id = lfm.parent_member_id
                LEFT JOIN languages pl ON pl.id = pmpl.language_id
                WHERE
                ($1::TEXT IS NULL OR lf.code = $1)
                AND (($2::TEXT IS NULL OR pl.code = $2) OR ($3::TEXT IS NULL OR l.code = $3))
                AND ($4::language_family_relation_type IS NULL OR lfm.relation_type = $4)
                AND ($8::UUID IS NULL OR lfm.parent_member_id = $8)
                ORDER BY (
                    CASE
                        WHEN $5::TEXT IS NOT NULL AND lfm.title ILIKE '%' || $5 || '%' THEN 100.0
                        WHEN $5::TEXT IS NOT NULL AND l.name ILIKE '%' || $5 || '%' THEN 100.0
                        WHEN $5::TEXT IS NOT NULL AND l.code ILIKE '%' || $5 || '%' THEN 90.0
                        WHEN $5::TEXT IS NOT NULL AND l.description ILIKE '%' || $5 || '%' THEN 80.0
                        WHEN $5::TEXT IS NOT NULL AND lfm.notes ILIKE '%' || $5 || '%' THEN 70.0
                        ELSE 0.0
                    END +
                    CASE WHEN $5::TEXT IS NOT NULL THEN
                        COALESCE(similarity(lfm.title, $5), 0.0) * 3.0 +
                        COALESCE(similarity(l.name, $5), 0.0) * 3.0 +
                        COALESCE(similarity(l.code, $5), 0.0) * 2.0 +
                        COALESCE(similarity(l.description, $5), 0.0) * 1.0 +
                        similarity(lfm.notes, $5) * 1.0
                    ELSE 0.0
                    END
                ) DESC, lfm.id
                LIMIT $6
                OFFSET $7
            "#,
            query.family_code,
            query.parent_language_code,
            query.language_code,
            query.relation_type as Option<LanguageFamilyRelationType>,
            query.q,
            i64::from(pagination.limit),
            i64::from(pagination.offset),
            query.parent_member_id,
        )
        .fetch_all(&self.state.pool);

        let count_future = sqlx::query_scalar!(
            r#"
                SELECT COUNT(*)
                FROM language_family_members lfm
                JOIN language_families lf ON lf.id = lfm.family_id
                LEFT JOIN languages l ON l.id = lfm.language_id
                LEFT JOIN language_family_members pmpl ON pmpl.id = lfm.parent_member_id
                LEFT JOIN languages pl ON pl.id = pmpl.language_id
                WHERE
                ($1::TEXT IS NULL OR lf.code = $1)
                AND (($2::TEXT IS NULL OR pl.code = $2) OR ($3::TEXT IS NULL OR l.code = $3))
                AND ($4::language_family_relation_type IS NULL OR lfm.relation_type = $4)
            "#,
            query.family_code,
            query.parent_language_code,
            query.language_code,
            query.relation_type as Option<LanguageFamilyRelationType>,
        )
        .fetch_one(&self.state.pool);

        let (rows, count) = tokio::try_join!(rows_future, count_future)?;

        let items = rows
            .into_iter()
            .map(LanguageFamilyMember::try_from)
            .collect::<Result<Vec<_>, _>>()?;

        let total = count.unwrap_or(0);
        let has_more =
            i64::from(pagination.offset) + i64::try_from(items.len()).unwrap_or(i64::MAX) < total;

        Ok(PaginatedResponse {
            items,
            total,
            limit: pagination.limit,
            offset: pagination.offset,
            has_more,
        })
    }

    // avoid exposing to frontend
    pub async fn all_for_language(
        &self,
        language_id: Uuid,
    ) -> AppResult<Vec<LanguageFamilyMember>> {
        let rows = sqlx::query_as!(
            LanguageFamilyMemberRow,
            r#"
                SELECT id, family_id, language_id, title, parent_member_id, relation_type as "relation_type: LanguageFamilyRelationType", created_at, updated_at, created_by, updated_by, notes
                FROM language_family_members
                WHERE language_id = $1
            "#,
            language_id
        )
        .fetch_all(&self.state.pool)
        .await?;

        rows.into_iter()
            .map(LanguageFamilyMember::try_from)
            .collect::<Result<Vec<_>, _>>()
    }
}

repo_from_parts!(LanguageFamilyMemberRepository);
