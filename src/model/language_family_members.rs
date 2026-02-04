use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Type, prelude::FromRow};
use uuid::Uuid;

use crate::{err::{AppResult, forbidden}, model::{language_families::{LanguageFamily, LanguageFamilyRepository}, language_family_permissions::LanguageFamilyPermissionRepository, language_invites::PermissionLevel, language_permissions::LanguagePermissionRepository, languages::{Language, LanguageRepository}, users::{User, UserRepository}}, pagination::{PaginatedRequest, PaginatedResponse}, util::{AppState, repo_from_parts}};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type, PartialEq, Eq)]
#[sqlx(type_name = "language_family_relation_type", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum LanguageFamilyRelationType {
    Descendant,
    Hybrid,
}

#[derive(FromRow, Clone, Serialize)]
pub struct LanguageFamilyMember {
    pub id: Uuid,
    #[serde(skip_serializing)]
    pub family_id: Uuid,
    #[serde(skip_serializing)]
    pub language_id: Option<Uuid>,
    #[serde(skip_serializing)]
    pub parent_member_id: Option<Uuid>,
    pub relation_type: LanguageFamilyRelationType,
    pub notes: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub created_by: Uuid,
    pub updated_by: Uuid,
}

#[derive(Serialize)]
pub struct MemberWithLanguages {
    pub member: LanguageFamilyMember,
    pub language: Option<Language>,
    pub parent_language: Option<Language>,
    pub family: LanguageFamily,
    pub creator: User,
    pub updater: User,
}

impl MemberWithLanguages {
    pub fn is_grouping(&self) -> bool {
        self.member.language_id.is_none()
    }

    pub fn name(&self) -> String {
        if let Some(language) = &self.language {
            language.name.clone()
        } else if !self.member.notes.is_empty() {
            self.member.notes.clone()
        } else {
            "(unnamed grouping)".to_string()
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct CreateLanguageFamilyMember {
    pub language_code: String,
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
    pub q: Option<String>, // name, code, description, notes
}

pub struct SearchRelatives {
    pub family_code: Option<String>,
    pub q: Option<String>,
    pub relation_type: Option<LanguageFamilyRelationType>,
}

pub struct LanguageFamilyMemberRepository {
    state: AppState,
}

impl LanguageFamilyMemberRepository {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    pub async fn find_by_id(&self, id: Uuid) -> AppResult<LanguageFamilyMember> {
        let result = sqlx::query_as!(
            LanguageFamilyMember,
            r#"
                SELECT id, family_id, language_id, parent_member_id, relation_type as "relation_type: LanguageFamilyRelationType", created_at, updated_at, created_by, updated_by, notes
                FROM language_family_members
                WHERE id = $1
            "#,
            id
        )
        .fetch_one(&self.state.pool)
        .await?;

        Ok(result)
    }

    pub async fn materialize(&self, member: LanguageFamilyMember) -> AppResult<MemberWithLanguages> {
        let languages = LanguageRepository::new(self.state.clone());

        let language = if let Some(language_id) = member.language_id {
            Some(
                languages
                    .find_by_id(language_id)
                    .await?
            )
        } else {
            None
        };

        let parent_language = if let Some(parent_member_id) = member.parent_member_id {
            let parent_member = self.find_by_id(parent_member_id).await?;
            if let Some(parent_language_id) = parent_member.language_id {
                Some(
                    languages
                        .find_by_id(parent_language_id)
                        .await?
                )
            } else {
                None
            }
        } else {
            None
        };

        let family = LanguageFamilyRepository::new(self.state.clone())
            .find_by_id(member.family_id)
            .await?;

        let users = UserRepository::new(self.state.clone());

        let creator = users
            .find_by_id(member.created_by)
            .await?;

        let updater = users
            .find_by_id(member.updated_by)
            .await?;

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
        let result = sqlx::query_as!(
            LanguageFamilyMember,
            r#"
                SELECT id, family_id, language_id, parent_member_id, relation_type as "relation_type: LanguageFamilyRelationType", created_at, updated_at, created_by, updated_by, notes
                FROM language_family_members
                WHERE family_id = $1 AND language_id = $2
            "#,
            family_id,
            language_id
        )
        .fetch_optional(&self.state.pool)
        .await?;

        Ok(result)
    }

    pub async fn create(&self, requestor: User, family: LanguageFamily, parent_id: Option<Uuid>, member: CreateLanguageFamilyMember) -> AppResult<LanguageFamilyMember> {
        crate::model::user_bans::UserBanRepository::new(self.state.clone())
            .ensure_not_banned(requestor.id)
            .await?;

        let languages = LanguageRepository::new(self.state.clone());
        let language = languages
            .find_by_code(&member.language_code)
            .await?;

        let family_permissions = LanguageFamilyPermissionRepository::new(self.state.clone());
        let family_has_permission = family_permissions
            .has_permission(
                requestor.id,
                family.id,
                PermissionLevel::Editor,
            )
            .await?;

        let language_permissions = LanguagePermissionRepository::new(self.state.clone());
        let language_has_permission = language_permissions
            .has_permission(
                requestor.id,
                language.id,
                PermissionLevel::Editor,
            )
            .await?;

        if !family_has_permission || !language_has_permission {
            return Err(forbidden(
                "user lacks permission to add member to language family",
            ));
        }

        let parent_member_id = if let Some(parent_id) = &parent_id {
            // ensure the parent member exists
            let parent_member = self
                .find_by_id(*parent_id)
                .await?;

            if parent_member.family_id != family.id && member.relation_type != LanguageFamilyRelationType::Hybrid {
                return Err(forbidden(
                    "parent member does not belong to the same family",
                ));
            }

            Some(parent_member.id)
        } else {
            // if no parent language is provided, ensure that there is no existing root for the family
            let existing_root = self
                .find_root(family.id)
                .await?;

            if existing_root.is_some() {
                return Err(forbidden(
                    "a root language already exists for this family",
                ));
            }

            None
        };

        let mut tx = self.state.pool.begin().await?;

        let result = sqlx::query_as!(
            LanguageFamilyMember,
            r#"
                INSERT INTO language_family_members (family_id, language_id, parent_member_id, relation_type, created_by, updated_by)
                VALUES ($1, $2, $3, $4, $5, $5)
                RETURNING id, family_id, language_id, parent_member_id, relation_type as "relation_type: LanguageFamilyRelationType", created_at, updated_at, created_by, updated_by, notes
            "#,
            family.id,
            language.id,
            parent_member_id,
            member.relation_type as LanguageFamilyRelationType,
            requestor.id
        )
        .fetch_one(&mut *tx)
        .await?;

        LanguageFamilyRepository::new(self.state.clone())
            .add_to_tree(family, result.clone(), &mut tx)
            .await?;

        tx.commit().await?;

        Ok(result)
    }

    /// Create a grouping node (no language attached).
    pub async fn create_grouping(&self, requestor: User, family: LanguageFamily, parent_id: Option<Uuid>, notes: Option<String>) -> AppResult<LanguageFamilyMember> {
        crate::model::user_bans::UserBanRepository::new(self.state.clone())
            .ensure_not_banned(requestor.id)
            .await?;

        let family_permissions = LanguageFamilyPermissionRepository::new(self.state.clone());
        let family_has_permission = family_permissions
            .has_permission(
                requestor.id,
                family.id,
                PermissionLevel::Editor,
            )
            .await?;

        if !family_has_permission {
            return Err(forbidden(
                "user lacks permission to add member to language family",
            ));
        }

        let parent_member_id = if let Some(parent_id) = &parent_id {
            let parent_member = self.find_by_id(*parent_id).await?;
            if parent_member.family_id != family.id {
                return Err(forbidden(
                    "parent member does not belong to the same family",
                ));
            }
            Some(parent_member.id)
        } else {
            None
        };

        let mut tx = self.state.pool.begin().await?;

        let result = sqlx::query_as!(
            LanguageFamilyMember,
            r#"
                INSERT INTO language_family_members (family_id, language_id, parent_member_id, relation_type, notes, created_by, updated_by)
                VALUES ($1, NULL, $2, 'descendant', $3, $4, $4)
                RETURNING id, family_id, language_id, parent_member_id, relation_type as "relation_type: LanguageFamilyRelationType", created_at, updated_at, created_by, updated_by, notes
            "#,
            family.id,
            parent_member_id,
            notes.unwrap_or_default(),
            requestor.id
        )
        .fetch_one(&mut *tx)
        .await?;

        LanguageFamilyRepository::new(self.state.clone())
            .add_to_tree(family, result.clone(), &mut tx)
            .await?;

        tx.commit().await?;

        Ok(result)
    }

    pub async fn find_root(&self, family_id: Uuid) -> AppResult<Option<LanguageFamilyMember>> {
        let results = sqlx::query_as!(
            LanguageFamilyMember,
            r#"
                SELECT id, family_id, language_id, parent_member_id, relation_type as "relation_type: LanguageFamilyRelationType", created_at, updated_at, created_by, updated_by, notes
                FROM language_family_members
                WHERE family_id = $1 AND parent_member_id IS NULL
            "#,
            family_id
        )
        .fetch_optional(&self.state.pool)
        .await?;

        Ok(results)
    }

    pub async fn delete(&self, requestor: &User, member_id: Uuid) -> AppResult<()> {
        if !LanguageFamilyPermissionRepository::new(self.state.clone())
            .has_permission(
                requestor.id,
                self.find_by_id(member_id).await?.family_id,
                PermissionLevel::Editor,
            )
            .await? {
            return Err(forbidden(
                "user lacks permission to delete language family member",
            ));
        }

        let mut tx = self.state.pool.begin().await?;
        let member  =sqlx::query_as!(
            LanguageFamilyMember,
            r#"
                DELETE FROM language_family_members
                WHERE id = $1
                RETURNING id, family_id, language_id, parent_member_id, relation_type as "relation_type: LanguageFamilyRelationType", created_at, updated_at, created_by, updated_by, notes
            "#,
            member_id
        )
        .fetch_one(&mut *tx)
        .await?;

        let families = LanguageFamilyRepository::new(self.state.clone());

        let family = families.find_by_id(member.family_id).await?;

        let _ = families
            .remove_from_tree(family, member, &mut tx)
            .await?;

        tx.commit().await?;

        Ok(())
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

    pub async fn search(&self, query: SearchLanguageFamilyMembers, pagination: PaginatedRequest) -> AppResult<PaginatedResponse<LanguageFamilyMember>> {
        let items_future = sqlx::query_as!(
            LanguageFamilyMember,
            r#"
                SELECT
                    lfm.id,
                    lfm.family_id,
                    lfm.language_id,
                    lfm.parent_member_id,
                    lfm.relation_type as "relation_type: LanguageFamilyRelationType",
                    lfm.created_at,
                    lfm.updated_at,
                    lfm.created_by,
                    lfm.updated_by,
                    lfm.notes
                FROM language_family_members lfm
                JOIN language_families lf ON lf.id = lfm.family_id
                JOIN languages l ON l.id = lfm.language_id
                LEFT JOIN language_family_members pmpl ON pmpl.id = lfm.parent_member_id
                LEFT JOIN languages pl ON pl.id = pmpl.language_id
                WHERE
                ($1::TEXT IS NULL OR lf.code = $1)
                AND (($2::TEXT IS NULL OR pl.code = $2) OR ($3::TEXT IS NULL OR l.code = $3))
                AND ($4::language_family_relation_type IS NULL OR lfm.relation_type = $4)
                AND ($8::UUID IS NULL OR lfm.parent_member_id = $8)
                ORDER BY (
                    CASE
                        WHEN $5::TEXT IS NOT NULL AND l.name ILIKE '%' || $5 || '%' THEN 100.0
                        WHEN $5::TEXT IS NOT NULL AND l.code ILIKE '%' || $5 || '%' THEN 90.0
                        WHEN $5::TEXT IS NOT NULL AND l.description ILIKE '%' || $5 || '%' THEN 80.0
                        WHEN $5::TEXT IS NOT NULL AND lfm.notes ILIKE '%' || $5 || '%' THEN 70.0
                        ELSE 0.0
                    END +
                    CASE WHEN $5::TEXT IS NOT NULL THEN
                        similarity(l.name, $5) * 3.0 +
                        similarity(l.code, $5) * 2.0 +
                        similarity(l.description, $5) * 1.0 +
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
                JOIN languages l ON l.id = lfm.language_id
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

        let (items, count) = tokio::try_join!(items_future, count_future)?;

        let total = count.unwrap_or(0);
        let has_more = i64::from(pagination.offset) + (items.len() as i64) < total;

        Ok(PaginatedResponse {
            items,
            total,
            limit: pagination.limit,
            offset: pagination.offset,
            has_more,
        })
    }



    // avoid exposing to frontend
    pub async fn all_for_language(&self, language_id: Uuid) -> AppResult<Vec<LanguageFamilyMember>> {
        let results = sqlx::query_as!(
            LanguageFamilyMember,
            r#"
                SELECT id, family_id, language_id, parent_member_id, relation_type as "relation_type: LanguageFamilyRelationType", created_at, updated_at, created_by, updated_by, notes
                FROM language_family_members
                WHERE language_id = $1
            "#,
            language_id
        )
        .fetch_all(&self.state.pool)
        .await?;

        Ok(results)
    }
}

repo_from_parts!(LanguageFamilyMemberRepository);