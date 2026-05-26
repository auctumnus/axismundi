use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;
use uuid::Uuid;

use crate::err::{AppResult, not_found};
use crate::model::bookmarks::{LinkType, ResolveBookmark};
use crate::model::language_families::LanguageFamilyRepository;
use crate::model::language_family_permissions::LanguageFamilyPermissionRepository;
use crate::model::language_invites::PermissionLevel;
use crate::model::users::User;
use crate::pagination::{PaginatedRequest, PaginatedResponse};
use crate::util::{AppState, ensure_verified};

#[derive(FromRow)]
pub struct LanguageFamilyInvite {
    pub id: Uuid,
    pub family: Uuid,
    pub sender: Uuid,
    pub recipient: Uuid,
    pub permissions: PermissionLevel,
    pub sent_at: DateTime<Utc>,
    pub accepted_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateLanguageFamilyInvite {
    pub language_family: String,
    pub recipient: String,
    pub permissions: PermissionLevel,
}

#[derive(Debug, Deserialize)]
pub struct FamilyInviteSearch {
    pub sender: Option<String>,
    pub recipient: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::util::deserialize_optional_form_datetime"
    )]
    pub created_before: Option<DateTime<Utc>>,
    #[serde(
        default,
        deserialize_with = "crate::util::deserialize_optional_form_datetime"
    )]
    pub created_after: Option<DateTime<Utc>>,
    #[serde(
        default,
        deserialize_with = "crate::util::deserialize_optional_form_datetime"
    )]
    pub accepted_before: Option<DateTime<Utc>>,
    #[serde(
        default,
        deserialize_with = "crate::util::deserialize_optional_form_datetime"
    )]
    pub accepted_after: Option<DateTime<Utc>>,
}

pub struct LanguageFamilyInviteRepository {
    state: AppState,
}

impl LanguageFamilyInviteRepository {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    pub async fn create(
        &self,
        sender: User,
        invite: CreateLanguageFamilyInvite,
    ) -> AppResult<LanguageFamilyInvite> {
        ensure_verified(&sender)?;

        let language_families = LanguageFamilyRepository::new(self.state.clone());
        let language_family = language_families
            .find_by_code(&invite.language_family)
            .await?;

        let permissions = LanguageFamilyPermissionRepository::new(self.state.clone());

        let Some(sender_permission) = permissions
            .find_by_family_and_user(language_family.id, sender.id)
            .await?
        else {
            return Err(not_found("sender's language permission"));
        };

        let recipient = crate::model::users::UserRepository::new(self.state.clone())
            .find_by_username(&invite.recipient)
            .await?;

        let recipient_has_permission = permissions
            .find_by_family_and_user(language_family.id, recipient.id)
            .await?
            .is_some();

        if recipient_has_permission {
            return Err(crate::err::bad_request(
                "recipient already has permission for this language family",
            ));
        }

        if !matches!(
            sender_permission.permission,
            PermissionLevel::Owner | PermissionLevel::Admin
        ) {
            return Err(crate::err::bad_request(
                "only owners and admins can create language family invites",
            ));
        }

        if invite.permissions == PermissionLevel::Owner
            && sender_permission.permission != PermissionLevel::Owner
        {
            return Err(crate::err::bad_request(
                "only owners can invite with owner permissions",
            ));
        }

        let result = sqlx::query_as!(
            LanguageFamilyInvite,
            r#"INSERT INTO language_family_invites (family, sender, recipient, permissions, sent_at) VALUES ($1, $2, $3, $4, NOW()) RETURNING id, family, sender, recipient, permissions as "permissions: PermissionLevel", sent_at, accepted_at"#,
            language_family.id,
            sender.id,
            recipient.id,
            invite.permissions as PermissionLevel
        )
        .fetch_one(&self.state.pool)
        .await?;

        Ok(result)
    }

    pub async fn find_by_id(&self, id: Uuid) -> AppResult<LanguageFamilyInvite> {
        let result = sqlx::query_as!(
            LanguageFamilyInvite,
            r#"SELECT id, family, sender, recipient, permissions as "permissions: PermissionLevel", sent_at, accepted_at FROM language_family_invites WHERE id = $1"#,
            id
        )
        .fetch_optional(&self.state.pool)
        .await?;

        match result {
            Some(invite) => Ok(invite),
            None => Err(not_found("language family invite")),
        }
    }

    pub async fn find_by_family_and_recipient_unchecked(
        &self,
        family: Uuid,
        recipient: Uuid,
    ) -> AppResult<Option<LanguageFamilyInvite>> {
        let result = sqlx::query_as!(
            LanguageFamilyInvite,
            r#"SELECT id, family, sender, recipient, permissions as "permissions: PermissionLevel", sent_at, accepted_at FROM language_family_invites WHERE family = $1 AND recipient = $2"#,
            family,
            recipient
        )
        .fetch_optional(&self.state.pool)
        .await?;

        Ok(result)
    }

    pub async fn find_by_family_and_recipient(
        &self,
        requestor: &User,
        family: Uuid,
        recipient: Uuid,
    ) -> AppResult<LanguageFamilyInvite> {
        let permissions = LanguageFamilyPermissionRepository::new(self.state.clone());
        let requestor_permission = permissions
            .find_by_family_and_user(family, requestor.id)
            .await?;

        if let Some(requestor_permission) = &requestor_permission {
            if requestor.id != recipient
                && !matches!(
                    requestor_permission.permission,
                    PermissionLevel::Owner | PermissionLevel::Admin
                )
            {
                return Err(crate::err::forbidden(
                    "only owners and admins can view language family invites",
                ));
            }
        } else if requestor.id != recipient {
            return Err(crate::err::forbidden(
                "only owners and admins can view language family invites",
            ));
        }

        let invite = self
            .find_by_family_and_recipient_unchecked(family, recipient)
            .await?;

        let Some(invite) = invite else {
            return Err(not_found("language family invite"));
        };

        Ok(invite)
    }

    pub async fn search(
        &self,
        requestor: &User,
        family: Uuid,
        pagination: PaginatedRequest,
        search: FamilyInviteSearch,
    ) -> AppResult<PaginatedResponse<LanguageFamilyInvite>> {
        let permissions = LanguageFamilyPermissionRepository::new(self.state.clone());
        let requestor_permission = permissions
            .find_by_family_and_user(family, requestor.id)
            .await?;
        let Some(requestor_permission) = requestor_permission else {
            return Err(not_found("requestor's language family permission"));
        };
        if !matches!(
            requestor_permission.permission,
            PermissionLevel::Owner | PermissionLevel::Admin
        ) {
            return Err(crate::err::bad_request(
                "only owners and admins can search language family invites",
            ));
        }

        let users = crate::model::users::UserRepository::new(self.state.clone());

        let sender = if let Some(sender_username) = &search.sender {
            let sender_user = users.find_by_username(sender_username).await?;
            Some(sender_user.id)
        } else {
            None
        };

        let recipient = if let Some(recipient_username) = &search.recipient {
            let recipient_user = users.find_by_username(recipient_username).await?;
            Some(recipient_user.id)
        } else {
            None
        };

        let invites = sqlx::query_as!(
            LanguageFamilyInvite,
            r#"
                SELECT
                    id,
                    family,
                    sender,
                    recipient,
                    permissions as "permissions: PermissionLevel",
                    sent_at,
                    accepted_at
                FROM language_family_invites
                WHERE
                family = $1
                AND ($2::UUID IS NULL OR sender = $2)
                AND ($3::UUID IS NULL OR recipient = $3)
                AND ($4::TIMESTAMPTZ IS NULL OR sent_at <= $4)
                AND ($5::TIMESTAMPTZ IS NULL OR sent_at >= $5)
                AND ($6::TIMESTAMPTZ IS NULL OR accepted_at <= $6)
                AND ($7::TIMESTAMPTZ IS NULL OR accepted_at >= $7)
                ORDER BY sent_at DESC, id
                LIMIT $8
                OFFSET $9
            "#,
            family,
            sender,
            recipient,
            search.created_before,
            search.created_after,
            search.accepted_before,
            search.accepted_after,
            i64::from(pagination.limit),
            i64::from(pagination.offset)
        )
        .fetch_all(&self.state.pool);

        let total_count = sqlx::query_scalar!(
            r#"
                SELECT COUNT(*)
                FROM language_family_invites
                WHERE
                family = $1
                AND ($2::UUID IS NULL OR sender = $2)
                AND ($3::UUID IS NULL OR recipient = $3)
                AND ($4::TIMESTAMPTZ IS NULL OR sent_at <= $4)
                AND ($5::TIMESTAMPTZ IS NULL OR sent_at >= $5)
                AND ($6::TIMESTAMPTZ IS NULL OR accepted_at <= $6)
                AND ($7::TIMESTAMPTZ IS NULL OR accepted_at >= $7)
            "#,
            family,
            sender,
            recipient,
            search.created_before,
            search.created_after,
            search.accepted_before,
            search.accepted_after
        )
        .fetch_one(&self.state.pool);

        let (invites, total_count) = tokio::try_join!(invites, total_count)?;

        let total_count = total_count.unwrap_or(0);
        let has_more = (i64::from(pagination.offset)
            + i64::try_from(invites.len()).unwrap_or(i64::MAX))
            < total_count;

        Ok(PaginatedResponse {
            items: invites,
            total: total_count,
            offset: pagination.offset,
            limit: pagination.limit,
            has_more,
        })
    }

    pub async fn accept(&self, requestor: &User, family: Uuid) -> AppResult<LanguageFamilyInvite> {
        crate::model::user_bans::UserBanRepository::new(self.state.clone())
            .ensure_not_banned(requestor.id)
            .await?;

        let mut tx = self.state.pool.begin().await?;

        let result = sqlx::query_as!(
            LanguageFamilyInvite,
            r#"
                UPDATE language_family_invites
                SET accepted_at = CURRENT_TIMESTAMP
                WHERE family = $1 AND recipient = $2 AND accepted_at IS NULL
                RETURNING id, family, sender, recipient, permissions as "permissions: PermissionLevel", sent_at, accepted_at
            "#,
            family,
            requestor.id
        )
        .fetch_optional(&mut *tx)
        .await?;

        let Some(invite) = result else {
            return Err(not_found(
                "language family invite does not exist or has already been accepted",
            ));
        };

        // apply permissions
        let permissions = LanguageFamilyPermissionRepository::new(self.state.clone());

        permissions.create_from_invite(&invite, &mut tx).await?;

        tx.commit().await?;
        Ok(invite)
    }

    pub async fn delete(&self, requestor: &User, family: Uuid, recipient: Uuid) -> AppResult<()> {
        crate::model::user_bans::UserBanRepository::new(self.state.clone())
            .ensure_not_banned(requestor.id)
            .await?;

        let permissions = LanguageFamilyPermissionRepository::new(self.state.clone());

        // fetch the invite first
        let invite = self
            .find_by_family_and_recipient_unchecked(family, recipient)
            .await?;

        let Some(invite) = invite else {
            return Err(not_found("language family invite"));
        };

        // recipients can delete invites sent to them
        if requestor.id == invite.recipient {
            sqlx::query!(
                "DELETE FROM language_family_invites WHERE id = $1",
                invite.id
            )
            .execute(&self.state.pool)
            .await?;
            return Ok(());
        }

        // otherwise check permissions
        let sender_permission = permissions
            .find_by_family_and_user(family, requestor.id)
            .await?;

        if let Some(sender_permission) = sender_permission {
            if !matches!(
                sender_permission.permission,
                PermissionLevel::Owner | PermissionLevel::Admin
            ) {
                // kind of a lie, but we don't want to reveal the existence of the invite
                return Err(crate::err::not_found("language family invite"));
            }

            match sender_permission.permission {
                PermissionLevel::Owner => {}
                PermissionLevel::Admin => {
                    let owner_permission = permissions.find_owner(family).await?;
                    if invite.sender == owner_permission.user {
                        return Err(crate::err::bad_request(
                            "admins cannot delete invites sent by the owner",
                        ));
                    }

                    if invite.sender != requestor.id {
                        return Err(crate::err::bad_request(
                            "admins can only delete their own invites",
                        ));
                    }
                }
                _ => {
                    // should be unreachable due to earlier check
                    return Err(crate::err::not_found("language family invite"));
                }
            }
        } else {
            return Err(not_found("requestor's language family permission"));
        }

        sqlx::query!(
            "DELETE FROM language_family_invites WHERE id = $1",
            invite.id
        )
        .execute(&self.state.pool)
        .await?;

        Ok(())
    }
}

#[async_trait]
impl ResolveBookmark for LanguageFamilyInviteRepository {
    async fn resolve_bookmark(&self, item: Uuid, link_type: LinkType) -> AppResult<String> {
        // api: /api/language-families/{code}/invites/{username}
        // web: /language-families/{code}/invites/{username}
        let invite = self.find_by_id(item).await?;

        let families = LanguageFamilyRepository::new(self.state.clone());
        let family = families.find_by_id(invite.family).await?;

        let users = crate::model::users::UserRepository::new(self.state.clone());
        let recipient = users.find_by_id(invite.recipient).await?;

        let slug = match link_type {
            LinkType::Web => format!(
                "/language-families/{}/invites/{}",
                family.code, recipient.username
            ),
            LinkType::Api => format!(
                "/api/language-families/{}/invites/{}",
                family.code, recipient.username
            ),
        };

        Ok(slug)
    }
}

crate::util::repo_from_parts!(LanguageFamilyInviteRepository);
