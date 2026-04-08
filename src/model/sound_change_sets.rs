use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use crate::{
    err::{AppResult, forbidden, internal_error, not_found},
    lexurgy,
    model::{
        language_family_members::{LanguageFamilyMember, LanguageFamilyMemberRepository},
        language_family_permissions::LanguageFamilyPermissionRepository,
        language_invites::PermissionLevel,
        language_permissions::LanguagePermissionRepository,
        languages::Language,
        user_bans::UserBanRepository,
        users::User,
    },
    pagination::{PaginatedRequest, PaginatedResponse},
    util::{AppState, repo_from_parts},
};

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SoundChangeSet {
    pub id: Uuid,
    #[serde(skip_serializing)]
    pub language_id: Option<Uuid>,
    #[serde(skip_serializing)]
    pub member_id: Option<Uuid>,
    pub name: String,
    pub description: String,
    pub changes: String,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing)]
    pub created_by: Uuid,
    #[serde(skip_serializing)]
    pub updated_by: Uuid,

    pub is_ipa_estimator: bool,
}

#[derive(Debug, Clone)]
pub enum SoundChangeSetOwner {
    Language(Uuid),
    Member(Uuid),
}

impl SoundChangeSet {
    pub fn owner(&self) -> SoundChangeSetOwner {
        match (self.language_id, self.member_id) {
            (Some(id), None) => SoundChangeSetOwner::Language(id),
            (None, Some(id)) => SoundChangeSetOwner::Member(id),
            _ => unreachable!("sound_change_sets_one_reference constraint violated"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewSoundChangeSet {
    pub name: String,
    pub description: Option<String>,
    pub changes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateSoundChangeSet {
    pub name: Option<String>,
    pub description: Option<String>,
    pub changes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SearchSoundChangeSets {
    pub q: Option<String>,
    pub author: Option<String>,
}

#[derive(Debug, Clone)]
pub struct MemberTarget {
    pub id: Uuid,
    pub family_id: Uuid,
    pub family_name: String,
    pub family_code: String,
    pub display_name: String,
}

#[derive(Debug, Clone)]
pub struct DerivationPath {
    pub language_id: Uuid,
    pub scs_ids: Vec<Uuid>,
}

#[derive(Debug, Clone)]
pub struct FamilyLanguageInfo {
    pub language_id: Uuid,
    pub language_name: String,
    pub language_code: String,
    pub family_id: Uuid,
    pub family_name: String,
}

crate::util::text_query!(SearchSoundChangeSets);

pub struct SoundChangeSetRepository {
    state: AppState,
}

impl SoundChangeSetRepository {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    pub async fn create_for_language(
        &self,
        requestor: &User,
        language: &Language,
        new_set: NewSoundChangeSet,
    ) -> AppResult<SoundChangeSet> {
        UserBanRepository::new(self.state.clone())
            .ensure_not_banned(requestor.id)
            .await?;

        let language_permissions = LanguagePermissionRepository::new(self.state.clone());
        let can_edit = language_permissions
            .has_permission(requestor.id, language.id, PermissionLevel::Editor)
            .await?;

        if !can_edit {
            return Err(forbidden(
                "You do not have permission to create a sound change set for this language",
            ));
        }

        let result = sqlx::query_as!(
            SoundChangeSet,
            r#"with inserted as (
                insert into sound_change_sets (language_id, name, description, changes, created_by, updated_by)
                values ($1, $2, $3, $4, $5, $6) returning *
            )
            select i.*, exists(select 1 from ipa_estimators where sound_change_set_id = i.id) as "is_ipa_estimator!"
            from inserted i"#,
            language.id,
            new_set.name,
            new_set.description.unwrap_or_default(),
            new_set.changes,
            requestor.id,
            requestor.id
        )
        .fetch_one(&self.state.pool)
        .await?;

        Ok(result)
    }

    pub async fn create_for_member(
        &self,
        requestor: &User,
        member: &LanguageFamilyMember,
        new_set: NewSoundChangeSet,
    ) -> AppResult<SoundChangeSet> {
        UserBanRepository::new(self.state.clone())
            .ensure_not_banned(requestor.id)
            .await?;

        let family_permissions = LanguageFamilyPermissionRepository::new(self.state.clone());
        let can_edit = family_permissions
            .has_permission(requestor.id, member.family_id(), PermissionLevel::Editor)
            .await?;

        if !can_edit {
            return Err(forbidden(
                "You do not have permission to create a sound change set for this family member",
            ));
        }

        let result = sqlx::query_as!(
            SoundChangeSet,
            r#"with inserted as (
                insert into sound_change_sets (member_id, name, description, changes, created_by, updated_by)
                values ($1, $2, $3, $4, $5, $6) returning *
            )
            select i.*, exists(select 1 from ipa_estimators where sound_change_set_id = i.id) as "is_ipa_estimator!"
            from inserted i"#,
            member.id(),
            new_set.name,
            new_set.description.unwrap_or_default(),
            new_set.changes,
            requestor.id,
            requestor.id
        )
        .fetch_one(&self.state.pool)
        .await?;

        Ok(result)
    }

    pub async fn get(&self, id: Uuid) -> AppResult<Option<SoundChangeSet>> {
        let set = sqlx::query_as!(
            SoundChangeSet,
            r#"select scs.*, exists(select 1 from ipa_estimators where sound_change_set_id = scs.id) as "is_ipa_estimator!"
            from sound_change_sets scs where scs.id = $1"#,
            id
        )
        .fetch_optional(&self.state.pool)
        .await?;

        Ok(set)
    }

    pub async fn is_ipa_estimator(&self, id: Uuid) -> AppResult<bool> {
        let result = sqlx::query_scalar!(
            "select exists(select 1 from ipa_estimators where sound_change_set_id = $1)",
            id
        )
        .fetch_one(&self.state.pool)
        .await?;

        Ok(result.unwrap_or(false))
    }

    pub async fn get_for_member(&self, member_id: Uuid) -> AppResult<Option<SoundChangeSet>> {
        let set = sqlx::query_as!(
            SoundChangeSet,
            r#"select scs.*, exists(select 1 from ipa_estimators where sound_change_set_id = scs.id) as "is_ipa_estimator!"
            from sound_change_sets scs where scs.member_id = $1"#,
            member_id
        )
        .fetch_optional(&self.state.pool)
        .await?;

        Ok(set)
    }

    pub async fn update(
        &self,
        requestor: &User,
        set_id: &Uuid,
        update: UpdateSoundChangeSet,
    ) -> AppResult<SoundChangeSet> {
        UserBanRepository::new(self.state.clone())
            .ensure_not_banned(requestor.id)
            .await?;

        let Some(set) = self.get(*set_id).await? else {
            return Err(not_found("SoundChangeSet not found"));
        };

        self.check_edit_permission(requestor, &set).await?;

        let updated = sqlx::query_as!(
            SoundChangeSet,
            r#"with updated as (
                update sound_change_sets set
                    name = coalesce($1, name),
                    description = coalesce($2, description),
                    changes = coalesce($3, changes),
                    updated_by = $4,
                    updated_at = now()
                where id = $5 returning *
            )
            select u.*, exists(select 1 from ipa_estimators where sound_change_set_id = u.id) as "is_ipa_estimator!"
            from updated u"#,
            update.name,
            update.description,
            update.changes,
            requestor.id,
            set_id
        )
        .fetch_one(&self.state.pool)
        .await?;

        Ok(updated)
    }

    pub async fn delete(&self, requestor: &User, set_id: &Uuid) -> AppResult<()> {
        UserBanRepository::new(self.state.clone())
            .ensure_not_banned(requestor.id)
            .await?;

        let Some(set) = self.get(*set_id).await? else {
            return Err(not_found("SoundChangeSet not found"));
        };

        self.check_edit_permission(requestor, &set).await?;

        sqlx::query!("delete from sound_change_sets where id = $1", set_id)
            .execute(&self.state.pool)
            .await?;

        Ok(())
    }

    pub async fn can_edit(&self, user_id: Uuid, set: &SoundChangeSet) -> AppResult<bool> {
        match set.owner() {
            SoundChangeSetOwner::Language(language_id) => {
                LanguagePermissionRepository::new(self.state.clone())
                    .has_permission(user_id, language_id, PermissionLevel::Editor)
                    .await
            }
            SoundChangeSetOwner::Member(member_id) => {
                let member = LanguageFamilyMemberRepository::new(self.state.clone())
                    .find_by_id(member_id)
                    .await?;
                LanguageFamilyPermissionRepository::new(self.state.clone())
                    .has_permission(user_id, member.family_id(), PermissionLevel::Editor)
                    .await
            }
        }
    }

    async fn check_edit_permission(&self, requestor: &User, set: &SoundChangeSet) -> AppResult<()> {
        match set.owner() {
            SoundChangeSetOwner::Language(language_id) => {
                let language_permissions = LanguagePermissionRepository::new(self.state.clone());
                let can_edit = language_permissions
                    .has_permission(requestor.id, language_id, PermissionLevel::Editor)
                    .await?;
                if !can_edit {
                    return Err(forbidden(
                        "You do not have permission to edit this sound change set",
                    ));
                }
            }
            SoundChangeSetOwner::Member(member_id) => {
                let member = LanguageFamilyMemberRepository::new(self.state.clone())
                    .find_by_id(member_id)
                    .await?;
                let family_permissions =
                    LanguageFamilyPermissionRepository::new(self.state.clone());
                let can_edit = family_permissions
                    .has_permission(requestor.id, member.family_id(), PermissionLevel::Editor)
                    .await?;
                if !can_edit {
                    return Err(forbidden(
                        "You do not have permission to edit this sound change set",
                    ));
                }
            }
        }
        Ok(())
    }

    pub async fn reassign_to_member(
        &self,
        requestor: &User,
        set_id: Uuid,
        target_member_id: Uuid,
    ) -> AppResult<SoundChangeSet> {
        UserBanRepository::new(self.state.clone())
            .ensure_not_banned(requestor.id)
            .await?;

        let Some(set) = self.get(set_id).await? else {
            return Err(not_found("SoundChangeSet not found"));
        };

        let language_id = match set.owner() {
            SoundChangeSetOwner::Language(id) => id,
            SoundChangeSetOwner::Member(_) => {
                return Err(crate::err::bad_request(
                    "Sound change set is already member-owned",
                ));
            }
        };

        if self.is_ipa_estimator(set_id).await? {
            return Err(crate::err::bad_request(
                "Cannot reassign a sound change set that is used as an IPA estimator",
            ));
        }

        let language_permissions = LanguagePermissionRepository::new(self.state.clone());
        let can_edit_language = language_permissions
            .has_permission(requestor.id, language_id, PermissionLevel::Editor)
            .await?;
        if !can_edit_language {
            return Err(forbidden(
                "You do not have permission to reassign this sound change set",
            ));
        }

        let member = LanguageFamilyMemberRepository::new(self.state.clone())
            .find_by_id(target_member_id)
            .await?;

        let family_permissions = LanguageFamilyPermissionRepository::new(self.state.clone());
        let can_edit_family = family_permissions
            .has_permission(requestor.id, member.family_id(), PermissionLevel::Editor)
            .await?;
        if !can_edit_family {
            return Err(forbidden(
                "You do not have permission to assign a sound change set to this family member",
            ));
        }

        let existing = self.get_for_member(target_member_id).await?;
        if existing.is_some() {
            return Err(crate::err::bad_request(
                "This family member already has a sound change set",
            ));
        }

        let updated = sqlx::query_as!(
            SoundChangeSet,
            r#"with updated as (
                update sound_change_sets set language_id = null, member_id = $1, updated_by = $2, updated_at = now()
                where id = $3 returning *
            )
            select u.*, exists(select 1 from ipa_estimators where sound_change_set_id = u.id) as "is_ipa_estimator!"
            from updated u"#,
            target_member_id,
            requestor.id,
            set_id
        )
        .fetch_one(&self.state.pool)
        .await?;

        Ok(updated)
    }

    pub async fn reassign_to_language(
        &self,
        requestor: &User,
        set_id: Uuid,
        target_language_id: Uuid,
    ) -> AppResult<SoundChangeSet> {
        UserBanRepository::new(self.state.clone())
            .ensure_not_banned(requestor.id)
            .await?;

        let Some(set) = self.get(set_id).await? else {
            return Err(not_found("SoundChangeSet not found"));
        };

        let member_id = match set.owner() {
            SoundChangeSetOwner::Member(id) => id,
            SoundChangeSetOwner::Language(_) => {
                return Err(crate::err::bad_request(
                    "Sound change set is already language-owned",
                ));
            }
        };

        let member = LanguageFamilyMemberRepository::new(self.state.clone())
            .find_by_id(member_id)
            .await?;

        let family_permissions = LanguageFamilyPermissionRepository::new(self.state.clone());
        let can_edit_family = family_permissions
            .has_permission(requestor.id, member.family_id(), PermissionLevel::Editor)
            .await?;
        if !can_edit_family {
            return Err(forbidden(
                "You do not have permission to reassign this sound change set",
            ));
        }

        let language_permissions = LanguagePermissionRepository::new(self.state.clone());
        let can_edit_language = language_permissions
            .has_permission(requestor.id, target_language_id, PermissionLevel::Editor)
            .await?;
        if !can_edit_language {
            return Err(forbidden(
                "You do not have permission to assign a sound change set to this language",
            ));
        }

        let updated = sqlx::query_as!(
            SoundChangeSet,
            r#"with updated as (
                update sound_change_sets set member_id = null, language_id = $1, updated_by = $2, updated_at = now()
                where id = $3 returning *
            )
            select u.*, exists(select 1 from ipa_estimators where sound_change_set_id = u.id) as "is_ipa_estimator!"
            from updated u"#,
            target_language_id,
            requestor.id,
            set_id
        )
        .fetch_one(&self.state.pool)
        .await?;

        Ok(updated)
    }

    pub async fn search(
        &self,
        language: &Language,
        pagination: PaginatedRequest,
        search: SearchSoundChangeSets,
    ) -> AppResult<PaginatedResponse<SoundChangeSet>> {
        let items_future = sqlx::query_as!(
            SoundChangeSet,
            r#"
                SELECT scs.*,
                    exists(select 1 from ipa_estimators where sound_change_set_id = scs.id) as "is_ipa_estimator!"
                FROM sound_change_sets scs
                JOIN users u ON u.id = scs.created_by
                WHERE
                scs.language_id = $1
                AND ($5::TEXT IS NULL OR u.username = $5)
                ORDER BY (
                    CASE
                        WHEN $2::TEXT IS NOT NULL AND scs.name ILIKE '%' || $2 || '%' THEN 100.0
                        WHEN $2::TEXT IS NOT NULL AND scs.description ILIKE '%' || $2 || '%' THEN 80.0
                        ELSE 0.0
                    END +
                    CASE WHEN $2::TEXT IS NOT NULL THEN
                        similarity(scs.name, $2) * 3.0 +
                        COALESCE(similarity(scs.description, $2), 0.0) * 1.0
                    ELSE 0.0
                    END
                ) DESC, scs.created_at DESC, scs.id DESC
                LIMIT $3
                OFFSET $4
            "#,
            language.id,
            search.q,
            i64::from(pagination.limit),
            i64::from(pagination.offset),
            search.author,
        )
        .fetch_all(&self.state.pool);

        let count_future = sqlx::query_scalar!(
            r#"
                SELECT COUNT(*)
                FROM sound_change_sets scs
                JOIN users u ON u.id = scs.created_by
                WHERE scs.language_id = $1
                AND ($2::TEXT IS NULL OR u.username = $2)
            "#,
            language.id,
            search.author,
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

    pub async fn run_from_db(
        &self,
        set_id: &Uuid,
        input_words: Vec<String>,
    ) -> AppResult<lexurgy::Response> {
        let set = self.get(*set_id).await?;

        if let Some(set) = set {
            let response =
                crate::lexurgy::run_sound_changes(set.changes, input_words, None, None, None)
                    .await?;

            match response {
                Ok(response) => Ok(response),
                Err(error) => Err(internal_error(format!(
                    "Failed to run sound changes: {error}"
                ))),
            }
        } else {
            Err(not_found(format!("sound change set with id {set_id}")))
        }
    }

    /// For save-to-new: find all family members in all families where `user_id` has
    /// editor access, excluding members that already have a sound change set.
    pub async fn find_member_targets_for_user(
        &self,
        user_id: Uuid,
    ) -> AppResult<Vec<MemberTarget>> {
        #[derive(FromRow)]
        struct Row {
            id: Uuid,
            family_id: Uuid,
            family_name: String,
            family_code: String,
            display_name: String,
        }

        let rows = sqlx::query_as!(
            Row,
            r#"
            select
                lfm.id,
                lfm.family_id,
                lf.name as family_name,
                lf.code as family_code,
                coalesce(l.name, lfm.title) as "display_name!"
            from language_family_members lfm
            join language_families lf on lf.id = lfm.family_id
            left join languages l on l.id = lfm.language_id
            where exists (
                select 1 from language_family_permissions lfp
                where lfp.family = lfm.family_id
                  and lfp."user" = $1
                  and lfp.permission >= 'editor'
            )
            and not exists (
                select 1 from sound_change_sets scs
                where scs.member_id = lfm.id
            )
            order by lf.name, lfm.title, l.name
            "#,
            user_id,
        )
        .fetch_all(&self.state.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| MemberTarget {
                id: r.id,
                family_id: r.family_id,
                family_name: r.family_name,
                family_code: r.family_code,
                display_name: r.display_name,
            })
            .collect())
    }

    /// For the reassign form: find all family members in families where `user_id` has
    /// editor access and the family contains `language_id`, excluding members that
    /// already have a sound change set and the member that IS the language.
    pub async fn find_available_member_targets(
        &self,
        user_id: Uuid,
        language_id: Uuid,
    ) -> AppResult<Vec<MemberTarget>> {
        #[derive(FromRow)]
        struct Row {
            id: Uuid,
            family_id: Uuid,
            family_name: String,
            family_code: String,
            display_name: String,
        }

        let rows = sqlx::query_as!(
            Row,
            r#"
            select
                lfm.id,
                lfm.family_id,
                lf.name as family_name,
                lf.code as family_code,
                coalesce(l.name, lfm.title) as "display_name!"
            from language_family_members lfm
            join language_families lf on lf.id = lfm.family_id
            left join languages l on l.id = lfm.language_id
            -- only families that contain the source language
            where lfm.family_id in (
                select family_id from language_family_members
                where language_id = $2
            )
            -- only families where the user has editor access
            and exists (
                select 1 from language_family_permissions lfp
                where lfp.family = lfm.family_id
                  and lfp."user" = $1
                  and lfp.permission >= 'editor'
            )
            -- exclude members that already have a sound change set
            and not exists (
                select 1 from sound_change_sets scs
                where scs.member_id = lfm.id
            )
            order by lf.name, lfm.title, l.name
            "#,
            user_id,
            language_id,
        )
        .fetch_all(&self.state.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| MemberTarget {
                id: r.id,
                family_id: r.family_id,
                family_name: r.family_name,
                family_code: r.family_code,
                display_name: r.display_name,
            })
            .collect())
    }

    /// Find all language members in families containing source_language_id,
    /// excluding the source itself. Returns language + family info for display.
    pub async fn find_family_language_infos(
        &self,
        source_language_id: Uuid,
    ) -> AppResult<Vec<FamilyLanguageInfo>> {
        #[derive(FromRow)]
        struct Row {
            language_id: Uuid,
            language_name: String,
            language_code: String,
            family_id: Uuid,
            family_name: String,
        }

        let rows = sqlx::query_as!(
            Row,
            r#"
            SELECT DISTINCT ON (l.id)
                l.id as "language_id!",
                l.name as "language_name!",
                l.code as "language_code!",
                lf.id as "family_id!",
                lf.name as "family_name!"
            FROM language_family_members lfm
            JOIN languages l ON l.id = lfm.language_id
            JOIN language_families lf ON lf.id = lfm.family_id
            WHERE lfm.family_id IN (
                SELECT family_id FROM language_family_members WHERE language_id = $1
            )
            AND lfm.language_id != $1
            ORDER BY l.id, lf.name
            "#,
            source_language_id
        )
        .fetch_all(&self.state.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| FamilyLanguageInfo {
                language_id: r.language_id,
                language_name: r.language_name,
                language_code: r.language_code,
                family_id: r.family_id,
                family_name: r.family_name,
            })
            .collect())
    }

    /// Find all descendant languages reachable from `source_language_id` by a complete
    /// chain of sound change sets (one per edge). Returns each reachable language paired
    /// with the ordered list of sound change set ids from source → target.
    pub async fn find_derivation_paths(
        &self,
        source_language_id: Uuid,
    ) -> AppResult<Vec<DerivationPath>> {
        #[derive(FromRow)]
        struct Row {
            language_id: Option<Uuid>,
            scs_ids: Option<Vec<Uuid>>,
        }

        let rows = sqlx::query_as!(
            Row,
            r#"
            with recursive derivation_paths as (
                select
                    m.id           as member_id,
                    m.language_id,
                    array[]::uuid[] as scs_ids,
                    true            as complete
                from language_family_members m
                where m.language_id = $1
                  and m.relation_type = 'descendant'

                union all

                select
                    child.id,
                    child.language_id,
                    dp.scs_ids || scs.id,
                    dp.complete and scs.id is not null
                from language_family_members child
                join derivation_paths dp on child.parent_member_id = dp.member_id
                left join sound_change_sets scs on scs.member_id = child.id
                where dp.complete
            )
            select dp.language_id, dp.scs_ids as "scs_ids: Vec<Uuid>"
            from derivation_paths dp
            where dp.language_id is not null
              and dp.complete
              and dp.language_id != $1
            "#,
            source_language_id
        )
        .fetch_all(&self.state.pool)
        .await?;

        Ok(rows
            .into_iter()
            .filter_map(|r| {
                r.language_id.map(|language_id| DerivationPath {
                    language_id,
                    scs_ids: r.scs_ids.unwrap_or_default(),
                })
            })
            .collect())
    }
}

repo_from_parts!(SoundChangeSetRepository);
