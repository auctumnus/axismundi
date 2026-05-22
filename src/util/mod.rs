use std::collections::HashMap;

use crate::{
    err::{AppError, AppResult, internal_error},
    md::render_md,
    model::users::User,
    pagination::PaginatedRequest,
};
use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{SaltString, rand_core::OsRng},
};
pub mod extract_session;
pub mod graph_svg;
mod images;
pub mod metrics;
pub mod s3;
pub mod search_template;
use askama::Template;
use base64::Engine;

mod re {
    macro_rules! re {
        ($r:expr) => {
            std::sync::LazyLock::new(|| regex::Regex::new($r).unwrap())
        };
    }

    pub(crate) use re;
}

use chrono::Utc;
pub(crate) use re::re;

mod repo {
    macro_rules! repo_from_parts {
        ($repo:ident) => {
            impl axum::extract::FromRequestParts<crate::util::AppState> for $repo {
                type Rejection = (axum::http::StatusCode, &'static str);

                async fn from_request_parts(
                    _: &mut axum::http::request::Parts,
                    state: &crate::util::AppState,
                ) -> Result<Self, Self::Rejection> {
                    Ok(Self::new(state.clone()))
                }
            }
        };
    }

    pub(crate) use repo_from_parts;
}

pub(crate) use repo::repo_from_parts;
use serde::{Deserialize, Serialize};
use uri_encode::encode_uri;
use uuid::Uuid;
use validator::ValidationError;

/// Hash plaintext using Argon2 and return the hash as a string.
pub fn hash(plaintext: &str) -> AppResult<String> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let hashed = argon2
        .hash_password(plaintext.as_bytes(), &salt)
        .map_err(|_| internal_error("Failed to hash password"))?;
    Ok(hashed.to_string())
}

/// Verify plaintext against a hash.
pub fn verify(password: &str, hash: &str) -> AppResult<bool> {
    let parsed_hash = PasswordHash::new(hash).map_err(|_| internal_error("Could not read hash"))?;
    let argon2 = Argon2::default();
    Ok(argon2
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok())
}

pub fn stable_hash(plaintext: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(plaintext.as_bytes());
    let result = hasher.finalize();
    base64::prelude::BASE64_STANDARD.encode(result)
}

pub fn ensure_verified(user: &User) -> AppResult<()> {
    if !user.is_verified() {
        return Err(crate::err::needs_verification());
    }
    Ok(())
}

/// Check if a user is an admin or moderator
pub async fn is_admin_or_mod(state: &AppState, user_id: uuid::Uuid) -> AppResult<bool> {
    use crate::model::user_tags::UserTagRepository;

    let user_tags = UserTagRepository::new(state.clone());
    let is_admin = user_tags.is_admin(user_id).await?;
    let is_moderator = user_tags.is_moderator(user_id).await?;

    Ok(is_admin || is_moderator)
}

/// Check if an operation on a language family will create an audit log entry.
/// This happens when an admin/mod user performs an action without proper family permissions.
pub async fn will_create_audit_log_for_family(
    state: &AppState,
    user: &User,
    family_id: uuid::Uuid,
) -> bool {
    use crate::model::language_family_permissions::LanguageFamilyPermissionRepository;
    use crate::model::language_invites::PermissionLevel;

    // Check if user is admin/mod
    let is_admin_or_mod = is_admin_or_mod(state, user.id).await.unwrap_or(false);

    if !is_admin_or_mod {
        return false;
    }

    // Check if they have proper permission for this family (Editor or above)
    let perms = LanguageFamilyPermissionRepository::new(state.clone());
    let has_permission = perms
        .has_permission(user.id, family_id, PermissionLevel::Editor)
        .await
        .unwrap_or(false);

    // Will create audit log if admin/mod but lacks permission
    !has_permission
}

/// Check if an operation on a language will create an audit log entry.
/// This happens when an admin/mod user performs an action without proper language permissions.
pub async fn will_create_audit_log_for_language(
    state: &AppState,
    user: &User,
    language_id: uuid::Uuid,
) -> bool {
    use crate::model::language_invites::PermissionLevel;
    use crate::model::language_permissions::LanguagePermissionRepository;

    // Check if user is admin/mod
    let is_admin_or_mod = is_admin_or_mod(state, user.id).await.unwrap_or(false);

    if !is_admin_or_mod {
        return false;
    }

    // Check if they have proper permission for this language (Editor or above)
    let perms = LanguagePermissionRepository::new(state.clone());
    let has_permission = perms
        .has_permission(user.id, language_id, PermissionLevel::Editor)
        .await
        .unwrap_or(false);

    // Will create audit log if admin/mod but lacks permission
    !has_permission
}

pub type PasswordValidationContext<'v_a> = &'v_a [&'v_a str];

pub fn validate_password(
    value: &str,
    user_inputs: PasswordValidationContext,
) -> Result<(), ValidationError> {
    let password_strength = zxcvbn::zxcvbn(value, user_inputs);
    if password_strength.score() < zxcvbn::Score::Three {
        let message = password_strength
            .feedback()
            .map_or("password is too weak".to_string(), |reason| {
                format!("password is too weak: {reason}")
            });

        let err = ValidationError::new("password_strength");

        let err = err.with_message(message.into());
        return Err(err);
    }
    Ok(())
}

pub fn sanitize_external_url(url: &str) -> Result<String, ValidationError> {
    let parsed = ammonia::Url::parse(url).map_err(|e| {
        let message = format!("invalid URL: {e}");
        ValidationError::new("invalid_url").with_message(message.into())
    })?;

    let scheme = parsed.scheme();

    if scheme != "http" && scheme != "https" {
        return Err(ValidationError::new("invalid_url")
            .with_message("URL must start with http:// or https://".into()));
    }

    Ok(parsed.to_string())
}

#[derive(Debug)]
pub struct AppState {
    pub pool: sqlx::PgPool,
    pub email_service: std::sync::Arc<dyn crate::email::EmailService>,
}

impl Clone for AppState {
    fn clone(&self) -> Self {
        Self {
            pool: self.pool.clone(),
            email_service: self.email_service.clone(),
        }
    }
}

// we use this in the html templates
pub fn get_error(error: Option<&AppError>, field: &str) -> Option<Vec<String>> {
    error.and_then(|err| err.error_for_field(field))
}

pub fn get_top_level_error(error: Option<&AppError>) -> Option<&str> {
    error.and_then(|err| err.top_level_error())
}

pub fn relative_time(timestamp: &chrono::DateTime<Utc>) -> String {
    let now = Utc::now();
    let dt = *timestamp - now;
    chrono_humanize::HumanTime::from(dt).to_string()
}

pub fn strip(text: &str) -> String {
    ammonia::Builder::empty().clean(text).to_string()
}

pub fn first_sentence(text: &str) -> String {
    let text = render_md(text).unwrap_or_default();
    let text = strip(&text);
    text.split_at(100.min(text.len())).0.to_string()
}

pub fn serialize_search<T: Serialize>(pagination: &PaginatedRequest, query: T) -> String {
    let q = serde_html_form::to_string(query).unwrap_or_default();
    let p = serde_html_form::to_string(pagination).unwrap_or_default();
    if q.is_empty() {
        p
    } else if p.is_empty() {
        q
    } else {
        format!("{}&{}", q, p)
    }
}

pub fn back_url<T: Serialize>(base: &str, pagination: &PaginatedRequest, query: T) -> String {
    let qs = serialize_search(pagination, query);
    if qs.is_empty() {
        base.to_string()
    } else {
        format!("{}?{}", base, qs)
    }
}

pub fn urlencode(input: &str) -> String {
    encode_uri(input)
}

// look ma, Introduction to Algorithms, fourth edition by Cormen et al!
pub fn dfs(
    adjacency_list: &HashMap<Uuid, Vec<Uuid>>,
    current: Uuid,
    target: Uuid,
    visited: &mut HashMap<Uuid, bool>,
) -> bool {
    if current == target {
        return true;
    }
    if let Some(&was_visited) = visited.get(&current) {
        if was_visited {
            return false;
        }
    }
    visited.insert(current, true);
    if let Some(neighbors) = adjacency_list.get(&current) {
        for &neighbor in neighbors {
            if dfs(adjacency_list, neighbor, target, visited) {
                return true;
            }
        }
    }
    false
}

pub trait HasTextQuery {
    fn text_query(&self) -> Option<&str>;
}

mod tq {
    macro_rules! text_query {
        ($struct:ident) => {
            impl crate::util::HasTextQuery for $struct {
                fn text_query(&self) -> Option<&str> {
                    self.q.as_deref()
                }
            }
        };
    }

    pub(crate) use text_query;
}

pub(crate) use tq::text_query;

#[derive(Template)]
#[template(source = "", ext = "html")]
pub struct EmptyTemplate;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListHeaderKind {
    Preview,
    Search,
}

#[derive(Deserialize)]
pub struct BackQuery {
    pub back: Option<String>,
}

pub fn is_discord(
    user_agent: Option<axum_extra::TypedHeader<axum_extra::headers::UserAgent>>,
) -> bool {
    if let Some(ua) = user_agent {
        ua.as_str().to_lowercase().contains("discordbot")
    } else {
        false
    }
}
