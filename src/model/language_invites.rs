use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Type};
use uuid::Uuid;
use validator::Validate;

use crate::err::{AppResult, not_found};
use crate::model::bookmarks::{LinkType, ResolveBookmark};
use crate::model::users::User;
use crate::pagination::{PaginatedRequest, PaginatedResponse};
use crate::util::AppState;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type, PartialEq, Eq, PartialOrd, Ord)]
#[sqlx(type_name = "permission_level", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum PermissionLevel {
    Viewer,
    Editor,
    Admin,
    Owner,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct LanguageInvite {
    pub id: Uuid,
    pub language: Uuid,
    pub sender: Uuid,
    pub recipient: Uuid,
    pub permissions: PermissionLevel,
    pub sent_at: DateTime<Utc>,
    pub accepted_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CreateLanguageInvite {
    pub language: String,
    pub recipient: String,
    pub permissions: PermissionLevel,
}

#[derive(Debug, Deserialize)]
pub struct InviteSearch {
    pub sender: Option<String>,
    pub recipient: Option<String>,
    pub created_before: Option<DateTime<Utc>>,
    pub created_after: Option<DateTime<Utc>>,
    pub accepted_before: Option<DateTime<Utc>>,
    pub accepted_after: Option<DateTime<Utc>>,
}

pub struct LanguageInviteRepository {
    state: AppState,
}

impl LanguageInviteRepository {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    pub async fn create(
        &self,
        invite: CreateLanguageInvite,
        sender: Uuid,
    ) -> AppResult<LanguageInvite> {
        invite.validate()?;

        let languages = crate::model::languages::LanguageRepository::new(self.state.clone());
        let language = languages.find_by_code(&invite.language).await?;

        let permissions = crate::model::language_permissions::LanguagePermissionRepository::new(
            self.state.clone(),
        );
        let sender_permission = permissions
            .find_by_user_and_language(sender, language.id)
            .await?;

        let Some(sender_permission) = sender_permission else {
            return Err(not_found("sender's language permission"));
        };

        let recipient = crate::model::users::UserRepository::new(self.state.clone())
            .find_by_username(&invite.recipient)
            .await?;

        let recipient_has_permission = permissions
            .find_by_user_and_language(recipient.id, language.id)
            .await?
            .is_some();

        if recipient_has_permission {
            return Err(crate::err::bad_request(
                "recipient already has permission for this language",
            ));
        }

        if !matches!(
            sender_permission.permission,
            PermissionLevel::Owner | PermissionLevel::Admin
        ) {
            return Err(crate::err::bad_request(
                "only owners and admins can create language invites",
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
            LanguageInvite,
            r#"
                INSERT INTO language_invites (language, sender, recipient, permissions)
                VALUES ($1, $2, $3, $4)
                RETURNING id, language, sender, recipient, permissions as "permissions: PermissionLevel", sent_at, accepted_at
            "#,
            language.id,
            sender,
            recipient.id,
            invite.permissions as PermissionLevel
        )
        .fetch_one(&self.state.pool)
        .await?;

        Ok(result)
    }

    async fn _find_by_language_and_recipient(
        &self,
        language: Uuid,
        recipient: Uuid,
    ) -> AppResult<Option<LanguageInvite>> {
        let result = sqlx::query_as!(
            LanguageInvite,
            r#"SELECT id, language, sender, recipient, permissions as "permissions: PermissionLevel", sent_at, accepted_at FROM language_invites WHERE language = $1 AND recipient = $2"#,
            language,
            recipient
        )
        .fetch_optional(&self.state.pool)
        .await?;

        Ok(result)
    }

    pub async fn find_by_language_and_recipient(
        &self,
        requestor: &User,
        language: Uuid,
        recipient: Uuid,
    ) -> AppResult<LanguageInvite> {
        let permissions = crate::model::language_permissions::LanguagePermissionRepository::new(
            self.state.clone(),
        );
        let requestor_permission = permissions
            .find_by_user_and_language(requestor.id, language)
            .await?;

        if let Some(requestor_permission) = &requestor_permission {
            if requestor.id != recipient
                && !matches!(
                    requestor_permission.permission,
                    PermissionLevel::Owner | PermissionLevel::Admin
                )
            {
                return Err(crate::err::forbidden(
                    "only owners and admins can view language invites",
                ));
            }
        } else {
            if requestor.id != recipient {
                return Err(crate::err::forbidden(
                    "only owners and admins can view language invites",
                ));
            }
        }

        let invite = self
            ._find_by_language_and_recipient(language, recipient)
            .await?;

        let Some(invite) = invite else {
            return Err(not_found("language invite"));
        };

        Ok(invite)
    }

    pub async fn find_by_id(&self, id: Uuid) -> AppResult<LanguageInvite> {
        let result = sqlx::query_as!(
            LanguageInvite,
            r#"SELECT id, language, sender, recipient, permissions as "permissions: PermissionLevel", sent_at, accepted_at FROM language_invites WHERE id = $1"#,
            id
        )
        .fetch_optional(&self.state.pool)
        .await?;

        result.ok_or_else(|| not_found(format!("language invite with id '{id}'")))
    }

    pub async fn search(
        &self,
        requestor: &User,
        language: Uuid,
        pagination: PaginatedRequest,
        search: InviteSearch,
    ) -> AppResult<PaginatedResponse<LanguageInvite>> {
        let permissions = crate::model::language_permissions::LanguagePermissionRepository::new(
            self.state.clone(),
        );
        let requestor_permission = permissions
            .find_by_user_and_language(requestor.id, language)
            .await?;
        let Some(requestor_permission) = requestor_permission else {
            return Err(not_found("requestor's language permission"));
        };
        if !matches!(
            requestor_permission.permission,
            PermissionLevel::Owner | PermissionLevel::Admin
        ) {
            return Err(crate::err::bad_request(
                "only owners and admins can search language invites",
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
            LanguageInvite,
            r#"
                SELECT
                    id,
                    language,
                    sender,
                    recipient,
                    permissions as "permissions: PermissionLevel",
                    sent_at,
                    accepted_at
                FROM language_invites
                WHERE
                language = $1
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
            language,
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
                FROM language_invites
                WHERE
                language = $1
                AND ($2::UUID IS NULL OR sender = $2)
                AND ($3::UUID IS NULL OR recipient = $3)
                AND ($4::TIMESTAMPTZ IS NULL OR sent_at <= $4)
                AND ($5::TIMESTAMPTZ IS NULL OR sent_at >= $5)
                AND ($6::TIMESTAMPTZ IS NULL OR accepted_at <= $6)
                AND ($7::TIMESTAMPTZ IS NULL OR accepted_at >= $7)
            "#,
            language,
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
        let has_more = (i64::from(pagination.offset) + invites.len() as i64) < total_count;

        Ok(PaginatedResponse {
            items: invites,
            total: total_count,
            offset: pagination.offset,
            limit: pagination.limit,
            has_more,
        })
    }

    pub async fn accept(&self, requestor: &User, language: Uuid) -> AppResult<LanguageInvite> {
        let mut tx = self.state.pool.begin().await?;

        let result = sqlx::query_as!(
            LanguageInvite,
            r#"
                UPDATE language_invites
                SET accepted_at = CURRENT_TIMESTAMP
                WHERE language = $1 AND recipient = $2 AND accepted_at IS NULL
                RETURNING id, language, sender, recipient, permissions as "permissions: PermissionLevel", sent_at, accepted_at
            "#,
            language,
            requestor.id
        )
        .fetch_optional(&mut *tx)
        .await?;

        let Some(invite) = result else {
            return Err(not_found(
                "language invite does not exist or has already been accepted",
            ));
        };

        // apply permissions
        let permissions = crate::model::language_permissions::LanguagePermissionRepository::new(
            self.state.clone(),
        );

        permissions.create_from_invite(&invite, &mut tx).await?;

        tx.commit().await?;
        Ok(invite)
    }

    pub async fn delete(&self, requestor: &User, language: Uuid, recipient: Uuid) -> AppResult<()> {
        let permissions = crate::model::language_permissions::LanguagePermissionRepository::new(
            self.state.clone(),
        );

        // fetch the invite first
        let invite = self
            ._find_by_language_and_recipient(language, recipient)
            .await?;

        let Some(invite) = invite else {
            return Err(not_found("language invite"));
        };

        // recipients can delete invites sent to them
        if requestor.id == invite.recipient {
            sqlx::query!("DELETE FROM language_invites WHERE id = $1", invite.id)
                .execute(&self.state.pool)
                .await?;
            return Ok(());
        }

        // otherwise check permissions
        let sender_permission = permissions
            .find_by_user_and_language(requestor.id, language)
            .await?;

        if let Some(sender_permission) = sender_permission {
            if !matches!(
                sender_permission.permission,
                PermissionLevel::Owner | PermissionLevel::Admin
            ) {
                // kind of a lie, but we don't want to reveal the existence of the invite
                return Err(crate::err::not_found("language invite"));
            }

            match sender_permission.permission {
                PermissionLevel::Owner => {}
                PermissionLevel::Admin => {
                    let owner_permission = permissions.find_owner(language).await?;
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
                    return Err(crate::err::not_found("language invite"));
                }
            }
        } else {
            return Err(not_found("requestor's language permission"));
        };

        sqlx::query!("DELETE FROM language_invites WHERE id = $1", invite.id)
            .execute(&self.state.pool)
            .await?;

        Ok(())
    }
}

#[async_trait]
impl ResolveBookmark for LanguageInviteRepository {
    async fn resolve_bookmark(&self, item: Uuid, link_type: LinkType) -> AppResult<String> {
        // api: /api/languages/{code}/invites/{username}
        // web: /languages/{code}/invites/{username}
        let invite = self.find_by_id(item).await?;

        let languages = crate::model::languages::LanguageRepository::new(self.state.clone());
        let language = languages.find_by_id(invite.language).await?;

        let users = crate::model::users::UserRepository::new(self.state.clone());
        let recipient = users.find_by_id(invite.recipient).await?;

        let slug = match link_type {
            LinkType::Web => format!(
                "/languages/{}/invites/{}",
                language.code, recipient.username
            ),
            LinkType::Api => format!(
                "/api/languages/{}/invites/{}",
                language.code, recipient.username
            ),
        };
        
        Ok(slug)
    }
}

crate::util::repo_from_parts!(LanguageInviteRepository);
