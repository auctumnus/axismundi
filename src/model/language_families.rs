use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::prelude::FromRow;
use uuid::Uuid;

use crate::config::CONFIG;
use crate::err::{AppResult, bad_request, internal_error, not_found};
use crate::model::contribution_stats::{ContributionStatsRepository, ContributionsSearch};
use crate::model::language_family_members::{
    LanguageFamilyMember, LanguageFamilyMemberRepository, LanguageFamilyRelationType,
};
use crate::model::language_family_permissions::{
    CreateLanguageFamilyPermission, LanguageFamilyPermissionRepository,
};
use crate::model::language_invites::PermissionLevel;
use crate::model::languages::Language;
use crate::model::users::User;
use crate::pagination::{PaginatedRequest, PaginatedResponse};
use crate::util::{AppState, repo_from_parts};

use super::users::UserRepository;

#[derive(FromRow, Clone, Serialize, Deserialize, Debug)]
pub struct LanguageFamily {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    pub description: String,
    pub tree: Value,
    pub like_count: i64,
    #[serde(skip)]
    #[sqlx(default)]
    pub banner_object_id: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub created_by: Uuid,
    pub updated_by: Uuid,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize)]
pub struct FamilyWithContributors {
    pub family: LanguageFamily,
    pub contributors: Vec<User>,
    pub is_liked: bool,
}

impl LanguageFamily {
    pub fn tree_schema(&self) -> AppResult<LanguageFamilyInner> {
        let schema: LanguageFamilyInner = serde_json::from_value(self.tree.clone())
            .map_err(|e| internal_error(format!("Failed to parse tree schema: {}", e)))?;
        Ok(schema)
    }

    pub fn get_banner_url(&self) -> Option<String> {
        if self.banner_object_id.is_empty() {
            None
        } else {
            Some(crate::util::s3::S3.get_banner_url(&self.banner_object_id))
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LanguageFamilyInner {
    #[serde(untagged)]
    V1(LanguageFamilySchemaV1),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguageFamilyEdgeV1 {
    pub parent_member_id: Option<Uuid>,
    pub child_member_id: Uuid,
    pub family_id: Uuid,
    pub relation_kind: FamilyRelationKindV1,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguageFamilySchemaV1 {
    pub edges: Vec<LanguageFamilyEdgeV1>,
    pub schema_version: i32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum FamilyRelationKindV1 {
    Descendant,
    Hybrid,
}

impl std::fmt::Display for FamilyRelationKindV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FamilyRelationKindV1::Descendant => write!(f, "Descendant"),
            FamilyRelationKindV1::Hybrid => write!(f, "Hybrid"),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct CreateLanguageFamily {
    pub code: String,
    pub name: String,
    pub description: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct UpdateLanguageFamily {
    pub code: Option<String>,
    pub name: Option<String>,
    pub description: Option<String>,
}

#[derive(Clone, Serialize, Deserialize, askama::Template, Default)]
#[template(path = "language_families/fragments/query.html")]
pub struct SearchLanguageFamilies {
    pub q: Option<String>,
    pub owner: Option<String>,
    pub has_language: Option<String>,
}

crate::util::text_query!(SearchLanguageFamilies);

pub struct LanguageFamilyRepository {
    state: AppState,
}

impl LanguageFamilyRepository {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    pub async fn materialize(
        &self,
        family: LanguageFamily,
        requestor: Option<&User>,
    ) -> AppResult<FamilyWithContributors> {
        let contribution_stats = ContributionStatsRepository::new(self.state.clone());
        let contributors = contribution_stats
            .search_top_contributors_for_family(
                &family.code,
                &ContributionsSearch {
                    q: None,
                    permission_level: None,
                },
                &PaginatedRequest {
                    limit: 5,
                    offset: 0,
                },
            )
            .await?;
        let is_liked = if let Some(requestor) = requestor {
            self.is_liked(&family.id, &requestor.id).await?
        } else {
            false
        };
        Ok(FamilyWithContributors {
            family,
            contributors: contributors.items,
            is_liked,
        })
    }

    pub async fn find_primary_family(
        &self,
        language: &Language,
    ) -> AppResult<Option<LanguageFamily>> {
        let family_id = sqlx::query_scalar!(
            r#"
                SELECT family_id
                FROM language_family_members
                WHERE language_id = $1
                AND relation_type = 'descendant'
                LIMIT 1
            "#,
            language.id
        )
        .fetch_optional(&self.state.pool)
        .await?;

        if let Some(family_id) = family_id {
            let family = self.find_by_id(family_id).await?;
            Ok(Some(family))
        } else {
            Ok(None)
        }
    }

    pub async fn create(
        &self,
        creator: User,
        family: CreateLanguageFamily,
    ) -> AppResult<LanguageFamily> {
        let mut tx = self.state.pool.begin().await?;

        let language_family = sqlx::query_as!(
            LanguageFamily,
            r#"
                INSERT INTO language_families (code, name, description, created_by, updated_by)
                VALUES ($1, $2, $3, $4, $4)
                RETURNING id, code, name, description, tree, like_count, banner_object_id, created_at, updated_at, created_by, updated_by
            "#,
            family.code,
            family.name,
            family.description,
            creator.id
        )
        .fetch_one(&mut *tx)
        .await?;

        let owner_permission = CreateLanguageFamilyPermission {
            family: language_family.id,
            user: creator.id,
            permission: PermissionLevel::Owner,
            via: None,
        };

        let permissions = LanguageFamilyPermissionRepository::new(self.state.clone());
        permissions
            .create_by_tx(&mut tx, owner_permission, creator.id)
            .await?;

        tx.commit().await?;

        let activity_repo =
            crate::model::user_activities::UserActivityRepository::new(self.state.clone());
        let _activity = activity_repo
            .create(
                creator.id,
                crate::model::user_activities::ActivityType::CreateLanguageFamily,
                language_family.id,
                "language_family",
                None,
                None,
            )
            .await?;

        Ok(language_family)
    }

    // at some point, we'll have more than 1 schema version
    #[allow(irrefutable_let_patterns)]
    pub async fn add_to_tree(
        &self,
        family: LanguageFamily,
        member: LanguageFamilyMember,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ) -> AppResult<LanguageFamily> {
        // TODO: cycle checking, adding to tree structure, etc.

        let schema = family.tree_schema()?;

        if let LanguageFamilyInner::V1(mut v1_schema) = schema {
            // check for cycles BEFORE adding the new edge
            // we check if we can reach the parent from the new member (via existing edges)
            // if so, adding parent -> new_member would create a cycle
            if let Some(parent_member_id) = member.parent_member_id() {
                let mut adjacency_list: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
                for e in &v1_schema.edges {
                    if let Some(pid) = e.parent_member_id {
                        adjacency_list
                            .entry(pid)
                            .or_default()
                            .push(e.child_member_id);
                    }
                }

                let mut visited = HashMap::new();

                // check if we can reach parent_member_id starting from member.id
                // (if so, adding the edge parent -> member creates a cycle)
                if crate::util::dfs(&adjacency_list, member.id(), parent_member_id, &mut visited) {
                    return Err(bad_request(
                        "adding this family relation would create a cycle in the language family graph",
                    ));
                }
            }

            let relation_kind = match &member {
                LanguageFamilyMember::Language(data) => match data.relation_type {
                    LanguageFamilyRelationType::Descendant => FamilyRelationKindV1::Descendant,
                    LanguageFamilyRelationType::Hybrid => FamilyRelationKindV1::Hybrid,
                },
                LanguageFamilyMember::Grouping(_) => FamilyRelationKindV1::Descendant,
            };

            let new_edge = LanguageFamilyEdgeV1 {
                parent_member_id: member.parent_member_id(),
                child_member_id: member.id(),
                family_id: member.family_id(),
                relation_kind,
            };
            v1_schema.edges.push(new_edge);

            if let LanguageFamilyMember::Language(ref lang_data) = member
                && lang_data.relation_type == LanguageFamilyRelationType::Hybrid
            {
                let language_id = lang_data.language_id;
                // get all of its hybrid parents and add them if they aren't already present
                let hybrid_parents = LanguageFamilyMemberRepository::new(self.state.clone())
                    .all_for_language(language_id)
                    .await?
                    .into_iter()
                    .filter_map(|m| {
                        if let LanguageFamilyMember::Language(data) = m {
                            if data.relation_type == LanguageFamilyRelationType::Hybrid {
                                data.parent_member_id.map(|pid| (pid, data.family_id))
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>();

                for (parent_id, family_id) in hybrid_parents {
                    let hybrid_edge = LanguageFamilyEdgeV1 {
                        parent_member_id: Some(parent_id),
                        child_member_id: member.id(),
                        family_id,
                        relation_kind: FamilyRelationKindV1::Hybrid,
                    };

                    // TODO: could be more efficient
                    if !v1_schema.edges.iter().any(|e| {
                        e.parent_member_id == hybrid_edge.parent_member_id
                            && e.child_member_id == hybrid_edge.child_member_id
                            && e.relation_kind == hybrid_edge.relation_kind
                    }) {
                        v1_schema.edges.push(hybrid_edge);
                    }
                }
            }

            // acyclic!
            let new_tree =
                serde_json::to_value(LanguageFamilyInner::V1(v1_schema)).map_err(|e| {
                    internal_error(format!("Failed to serialize updated tree schema: {}", e))
                })?;
            let updated_family = self.update_tree(family, new_tree, tx).await?;
            Ok(updated_family)
        } else {
            Err(internal_error("Unsupported language family schema version"))
        }
    }

    // at some point, we'll have more than 1 schema version
    #[allow(irrefutable_let_patterns)]
    pub async fn remove_from_tree(
        &self,
        family: LanguageFamily,
        member: LanguageFamilyMember,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ) -> AppResult<LanguageFamily> {
        let schema = family.tree_schema()?;

        if let LanguageFamilyInner::V1(mut v1_schema) = schema {
            v1_schema.edges.retain(|e| {
                e.child_member_id != member.id() && e.parent_member_id != Some(member.id())
            });

            let new_tree =
                serde_json::to_value(LanguageFamilyInner::V1(v1_schema)).map_err(|e| {
                    internal_error(format!("Failed to serialize updated tree schema: {}", e))
                })?;
            let updated_family = self.update_tree(family, new_tree, tx).await?;
            Ok(updated_family)
        } else {
            Err(internal_error("Unsupported language family schema version"))
        }
    }

    // Replaces all edges where `member_id` is the child with a single Descendant edge.
    // Used when converting a member's type (language ↔ grouping) to keep the tree consistent.
    // at some point, we'll have more than 1 schema version
    #[allow(irrefutable_let_patterns)]
    pub async fn rebuild_member_in_tree(
        &self,
        family: LanguageFamily,
        member_id: Uuid,
        parent_member_id: Option<Uuid>,
        family_id: Uuid,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ) -> AppResult<LanguageFamily> {
        let schema = family.tree_schema()?;

        if let LanguageFamilyInner::V1(mut v1_schema) = schema {
            v1_schema.edges.retain(|e| e.child_member_id != member_id);
            v1_schema.edges.push(LanguageFamilyEdgeV1 {
                parent_member_id,
                child_member_id: member_id,
                family_id,
                relation_kind: FamilyRelationKindV1::Descendant,
            });

            let new_tree =
                serde_json::to_value(LanguageFamilyInner::V1(v1_schema)).map_err(|e| {
                    internal_error(format!("Failed to serialize updated tree schema: {}", e))
                })?;
            self.update_tree(family, new_tree, tx).await
        } else {
            Err(internal_error("Unsupported language family schema version"))
        }
    }

    #[allow(irrefutable_let_patterns)]
    pub async fn swap_in_tree(
        &self,
        family: LanguageFamily,
        member_id: Uuid,
        parent_id: Uuid,
        grandparent_id: Option<Uuid>,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ) -> AppResult<LanguageFamily> {
        let schema = family.tree_schema()?;

        if let LanguageFamilyInner::V1(mut v1_schema) = schema {
            for edge in &mut v1_schema.edges {
                if edge.child_member_id == member_id
                    && edge.relation_kind == FamilyRelationKindV1::Descendant
                {
                    edge.parent_member_id = grandparent_id;
                } else if edge.child_member_id == parent_id
                    && edge.relation_kind == FamilyRelationKindV1::Descendant
                {
                    edge.parent_member_id = Some(member_id);
                }
            }

            let new_tree =
                serde_json::to_value(LanguageFamilyInner::V1(v1_schema)).map_err(|e| {
                    internal_error(format!("Failed to serialize updated tree schema: {}", e))
                })?;
            self.update_tree(family, new_tree, tx).await
        } else {
            Err(internal_error("Unsupported language family schema version"))
        }
    }

    #[allow(irrefutable_let_patterns)]
    pub async fn change_parent_in_tree(
        &self,
        family: LanguageFamily,
        member_id: Uuid,
        new_parent_id: Uuid,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ) -> AppResult<LanguageFamily> {
        use std::collections::HashMap;

        let schema = family.tree_schema()?;

        if let LanguageFamilyInner::V1(mut v1_schema) = schema {
            let mut adjacency_list: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
            for e in &v1_schema.edges {
                if let Some(pid) = e.parent_member_id {
                    adjacency_list.entry(pid).or_default().push(e.child_member_id);
                }
            }

            if crate::util::dfs(&adjacency_list, member_id, new_parent_id, &mut HashMap::new()) {
                return Err(crate::err::bad_request(
                    "changing parent would create a cycle in the language family tree",
                ));
            }

            let mut found = false;
            for edge in &mut v1_schema.edges {
                if edge.child_member_id == member_id
                    && edge.relation_kind == FamilyRelationKindV1::Descendant
                {
                    edge.parent_member_id = Some(new_parent_id);
                    found = true;
                    break;
                }
            }

            if !found {
                return Err(internal_error(
                    "member's descendant edge not found in tree",
                ));
            }

            let new_tree =
                serde_json::to_value(LanguageFamilyInner::V1(v1_schema)).map_err(|e| {
                    internal_error(format!("Failed to serialize updated tree schema: {}", e))
                })?;
            self.update_tree(family, new_tree, tx).await
        } else {
            Err(internal_error("Unsupported language family schema version"))
        }
    }

    pub async fn update_tree(
        &self,
        family: LanguageFamily,
        new_tree: Value,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ) -> AppResult<LanguageFamily> {
        let updated_family = sqlx::query_as!(
            LanguageFamily,
            r#"
                UPDATE language_families
                SET tree = $1, updated_at = CURRENT_TIMESTAMP
                WHERE id = $2
                RETURNING id, code, name, description, tree, like_count, banner_object_id, created_at, updated_at, created_by, updated_by
            "#,
            new_tree,
            family.id
        )
        .fetch_one(&mut **tx)
        .await?;

        Ok(updated_family)
    }

    pub async fn find_by_code(&self, code: &str) -> AppResult<LanguageFamily> {
        let language_family = sqlx::query_as!(
            LanguageFamily,
            "SELECT * FROM language_families WHERE code = $1",
            code
        )
        .fetch_one(&self.state.pool)
        .await?;

        Ok(language_family)
    }

    pub async fn update(
        &self,
        user: &User,
        id: Uuid,
        update: UpdateLanguageFamily,
    ) -> AppResult<LanguageFamily> {
        use crate::model::audit_log::{AuditActionType, AuditableResource, PermissionCheck};
        use crate::model::language_family_permissions::CheckPermissionReq;

        crate::model::user_bans::UserBanRepository::new(self.state.clone())
            .ensure_not_banned(user.id)
            .await?;

        let family = self.find_by_id(id).await?;

        let new_code = update.code.clone().unwrap_or(family.code.clone());
        let new_name = update.name.clone().unwrap_or(family.name.clone());
        let new_description = update
            .description
            .clone()
            .unwrap_or(family.description.clone());

        let mut tx = self.state.pool.begin().await?;

        // Check permission with audit
        let perms = LanguageFamilyPermissionRepository::new(self.state.clone());
        let perm_check = perms
            .check_permission_with_audit(
                CheckPermissionReq {
                    user: user.id,
                    family: id,
                    required_level: PermissionLevel::Editor,
                    action_type: AuditActionType::Updated,
                    resource_type: AuditableResource::LanguageFamily,
                    resource_id: id,
                    context: Some(serde_json::json!({
                        "code": update.code,
                        "name": update.name,
                        "description": update.description,
                    })),
                },
                &mut tx,
            )
            .await?;

        if perm_check == PermissionCheck::NoPermission {
            return Err(crate::err::forbidden(
                "You don't have permission to edit this language family",
            ));
        }

        let updated = sqlx::query_as!(
            LanguageFamily,
            r#"
                UPDATE language_families
                SET code = $1, name = $2, description = $3, updated_at = CURRENT_TIMESTAMP, updated_by = $4
                WHERE id = $5
                RETURNING id, code, name, description, tree, like_count, banner_object_id, created_at, updated_at, created_by, updated_by
            "#,
            new_code,
            new_name,
            new_description,
            user.id,
            id
        )
        .fetch_one(&mut *tx)
        .await?;

        tx.commit().await?;

        let activity_repo =
            crate::model::user_activities::UserActivityRepository::new(self.state.clone());
        let _activity = activity_repo
            .create(
                user.id,
                crate::model::user_activities::ActivityType::UpdateLanguageFamily,
                updated.id,
                "language_family",
                None,
                None,
            )
            .await?;

        Ok(updated)
    }

    pub async fn update_banner(
        &self,
        user: &User,
        id: Uuid,
        object_key: &str,
    ) -> AppResult<LanguageFamily> {
        let perms = LanguageFamilyPermissionRepository::new(self.state.clone());
        let has_perm = perms
            .has_permission(user.id, id, PermissionLevel::Editor)
            .await?;

        if !has_perm {
            return Err(crate::err::forbidden(
                "you don't have permission to edit this language family",
            ));
        }

        let result = sqlx::query_as!(
            LanguageFamily,
            r#"
                UPDATE language_families
                SET banner_object_id = $2,
                    updated_at = CURRENT_TIMESTAMP
                WHERE id = $1
                RETURNING id, code, name, description, tree, like_count, banner_object_id, created_at, updated_at, created_by, updated_by
            "#,
            id,
            object_key
        )
        .fetch_optional(&self.state.pool)
        .await?;

        result.ok_or_else(|| not_found(format!("language family with id '{id}'")))
    }

    pub async fn delete(&self, user: &User, id: Uuid) -> AppResult<()> {
        use crate::model::audit_log::{AuditActionType, AuditableResource, PermissionCheck};
        use crate::model::language_family_permissions::CheckPermissionReq;

        crate::model::user_bans::UserBanRepository::new(self.state.clone())
            .ensure_not_banned(user.id)
            .await?;

        // Get family details for audit log before deletion
        let family = self.find_by_id(id).await?;

        let mut tx = self.state.pool.begin().await?;

        // Check permission with audit - need Owner level to delete
        let perms = LanguageFamilyPermissionRepository::new(self.state.clone());
        let perm_check = perms
            .check_permission_with_audit(
                CheckPermissionReq {
                    user: user.id,
                    family: id,
                    required_level: PermissionLevel::Owner,
                    action_type: AuditActionType::Deleted,
                    resource_type: AuditableResource::LanguageFamily,
                    resource_id: id,
                    context: Some(serde_json::json!({
                        "code": &family.code,
                        "name": &family.name,
                    })),
                },
                &mut tx,
            )
            .await?;

        if perm_check == PermissionCheck::NoPermission {
            return Err(crate::err::forbidden(
                "You don't have permission to delete this language family",
            ));
        }

        // Delete all members first
        sqlx::query!(
            "DELETE FROM language_family_members WHERE family_id = $1",
            id
        )
        .execute(&mut *tx)
        .await?;

        // Delete permissions
        sqlx::query!(
            "DELETE FROM language_family_permissions WHERE family = $1",
            id
        )
        .execute(&mut *tx)
        .await?;

        // Delete likes
        sqlx::query!("DELETE FROM language_family_likes WHERE family_id = $1", id)
            .execute(&mut *tx)
            .await?;

        // Delete invites
        sqlx::query!("DELETE FROM language_family_invites WHERE family = $1", id)
            .execute(&mut *tx)
            .await?;

        // Delete the family itself
        sqlx::query!("DELETE FROM language_families WHERE id = $1", id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;

        Ok(())
    }

    pub async fn find_by_id(&self, id: Uuid) -> AppResult<LanguageFamily> {
        let result = sqlx::query_as!(
            LanguageFamily,
            "SELECT * FROM language_families WHERE id = $1",
            id
        )
        .fetch_optional(&self.state.pool)
        .await?;

        result.ok_or_else(|| not_found(format!("language family with id '{id}'")))
    }

    pub async fn search(
        &self,
        query: SearchLanguageFamilies,
        pagination: PaginatedRequest,
    ) -> AppResult<PaginatedResponse<LanguageFamily>> {
        let items_future = sqlx::query_as!(
            LanguageFamily,
            r#"
                SELECT
                    language_families.id,
                    language_families.code,
                    language_families.name,
                    language_families.description,
                    language_families.tree,
                    language_families.like_count,
                    language_families.banner_object_id,
                    language_families.created_at,
                    language_families.updated_at,
                    language_families.created_by,
                    language_families.updated_by
                FROM language_families
                JOIN users ON users.id = language_families.created_by
                WHERE
                ($1::TEXT IS NULL OR users.username = $1)
                AND ($3::TEXT IS NULL OR language_families.id IN (
                    SELECT language_family_members.family_id
                    FROM language_family_members
                    JOIN languages ON languages.id = language_family_members.language_id
                    WHERE languages.code = $3
                ))
                ORDER BY (
                    CASE
                        WHEN $2::TEXT IS NOT NULL AND language_families.name ILIKE '%' || $2 || '%' THEN 100.0
                        WHEN $2::TEXT IS NOT NULL AND language_families.code ILIKE '%' || $2 || '%' THEN 90.0
                        WHEN $2::TEXT IS NOT NULL AND language_families.description ILIKE '%' || $2 || '%' THEN 80.0
                        ELSE 0.0
                    END +
                    CASE WHEN $2::TEXT IS NOT NULL THEN
                        similarity(language_families.name, $2) * 3.0 +
                        similarity(language_families.code, $2) * 2.0 +
                        similarity(language_families.description, $2) * 1.0
                    ELSE 0.0
                    END
                ) DESC, language_families.like_count DESC, language_families.id
                LIMIT $4
                OFFSET $5
            "#,
            query.owner,
            query.q,
            query.has_language,
            i64::from(pagination.limit),
            i64::from(pagination.offset),
        )
        .fetch_all(&self.state.pool);

        let count_future = sqlx::query_scalar!(
            r#"
                SELECT COUNT(*)
                FROM language_families
                JOIN users ON users.id = language_families.created_by
                WHERE
                ($1::TEXT IS NULL OR users.username = $1)
                AND ($2::TEXT IS NULL OR language_families.id IN (
                    SELECT language_family_members.family_id
                    FROM language_family_members
                    JOIN languages ON languages.id = language_family_members.language_id
                    WHERE languages.code = $2
                ))
            "#,
            query.owner,
            query.has_language,
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

    pub async fn is_liked(&self, family_id: &Uuid, user_id: &Uuid) -> AppResult<bool> {
        let result = sqlx::query!(
            r#"
                SELECT 1 as exists FROM language_family_likes
                WHERE family_id = $1 AND user_id = $2
            "#,
            family_id,
            user_id
        )
        .fetch_optional(&self.state.pool)
        .await?;

        Ok(result.is_some())
    }

    pub async fn like_language_family(
        &self,
        family_id: Uuid,
        user_id: Uuid,
    ) -> AppResult<Option<i64>> {
        let mut tx = self.state.pool.begin().await?;
        let result = sqlx::query!(
            r#"
                INSERT INTO language_family_likes (family_id, user_id)
                VALUES ($1, $2)
                ON CONFLICT DO NOTHING
            "#,
            family_id,
            user_id
        )
        .execute(&mut *tx)
        .await?;

        let likes = if result.rows_affected() > 0 {
            let likes = sqlx::query_scalar!(
                r#"
                    UPDATE language_families
                    SET like_count = like_count + 1
                    WHERE id = $1
                    RETURNING like_count
                "#,
                family_id
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

    pub async fn unlike_language_family(
        &self,
        family_id: Uuid,
        user_id: Uuid,
    ) -> AppResult<Option<i64>> {
        let mut tx = self.state.pool.begin().await?;
        let result = sqlx::query!(
            r#"
                DELETE FROM language_family_likes
                WHERE family_id = $1 AND user_id = $2
            "#,
            family_id,
            user_id
        )
        .execute(&mut *tx)
        .await?;

        let likes = if result.rows_affected() > 0 {
            let likes = sqlx::query_scalar!(
                r#"
                    UPDATE language_families
                    SET like_count = GREATEST(like_count - 1, 0)
                    WHERE id = $1
                    RETURNING like_count
                "#,
                family_id
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

    pub async fn find_owner(&self, family_id: Uuid) -> AppResult<User> {
        let result = sqlx::query_as!(
            User,
            r#"
                SELECT
                    users.id,
                    users.username,
                    users.email,
                    users.password_hash,
                    users.display_name,
                    users.description,
                    users.pronouns,
                    users.gender,
                    users.profile_picture_object_id,
                    users.banner_object_id,
                    users.verified_at,
                    users.tags,
                    users.created_at,
                    users.updated_at,
                    COALESCE(bookmarks.slug, '')::text as "bookmark!"
                FROM users
                LEFT JOIN bookmarks ON bookmarks.item = users.id AND bookmarks.resource = 'user'
                JOIN language_family_permissions ON language_family_permissions.user = users.id
                WHERE language_family_permissions.family = $1
                AND language_family_permissions.permission = 'owner'
            "#,
            family_id
        )
        .fetch_one(&self.state.pool);

        result.await.map_err(Into::into)
    }

    pub async fn top_families(&self, user_id: Uuid, limit: i64) -> AppResult<Vec<LanguageFamily>> {
        let result = sqlx::query_as!(
            LanguageFamily,
            r#"
                SELECT
                    lf.id,
                    lf.code,
                    lf.name,
                    lf.description,
                    lf.tree,
                    lf.like_count,
                    lf.banner_object_id,
                    lf.created_at,
                    lf.updated_at,
                    lf.created_by,
                    lf.updated_by
                FROM language_families lf
                JOIN language_family_permissions lfp ON lfp.family = lf.id AND lfp."user" = $1
                LEFT JOIN (
                    SELECT lfm.family_id, MAX(ua.timestamp) as last_activity
                    FROM language_family_members lfm
                    JOIN user_activities ua ON (
                        (ua.entity_id = lfm.language_id AND ua.entity_type = 'language')
                        OR (ua.related_entity_id = lfm.language_id AND ua.related_entity_type = 'language')
                    )
                    WHERE ua.user_id = $1 AND lfm.language_id IS NOT NULL
                    GROUP BY lfm.family_id
                ) as la ON la.family_id = lf.id
                ORDER BY COALESCE(la.last_activity, lf.created_at) DESC
                LIMIT $2
            "#,
            user_id,
            limit
        )
        .fetch_all(&self.state.pool)
        .await?;

        Ok(result)
    }

    pub async fn as_json_ld(&self, family: &LanguageFamily) -> AppResult<Value> {
        let owner = self.find_owner(family.id).await?;

        let json_ld = json!({
            "@context": "https://schema.org",
            "@type": "CreativeWork",
            "name": family.name,
            "description": family.description,
            "identifier": family.code,
            "author": UserRepository::as_json_ld(&owner),
            "url": format!("{}/language-families/{}", CONFIG.public_url_base, family.code),
        });

        Ok(json_ld)
    }
}

repo_from_parts!(LanguageFamilyRepository);
