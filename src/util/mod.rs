use crate::{
    err::{AppError, AppResult, internal_error},
    model::users::User, pagination::PaginatedRequest,
};
use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{SaltString, rand_core::OsRng},
};
pub mod extract_session;
mod images;
pub mod s3;
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
use serde::Serialize;
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

pub type PasswordValidationContext<'v_a> = &'v_a [&'v_a str];

pub fn validate_password(value: &str, user_inputs: PasswordValidationContext) -> Result<(), ValidationError> {
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

// we use this in the html templates, and we get a &Option<AppError> which is annoying to work with
#[allow(clippy::ref_option)]
pub fn get_error(error: &Option<AppError>, field: &str) -> Option<Vec<String>> {
    if let Some(err) = error {
        err.error_for_field(field)
    } else {
        None
    }
}

pub fn get_top_level_error(error: &Option<AppError>) -> Option<&str> {
    if let Some(err) = error {
        err.top_level_error()
    } else {
        None
    }
}

pub fn relative_time(timestamp: &chrono::DateTime<Utc>) -> String {
    let now = Utc::now();
    let dt = *timestamp - now;
    chrono_humanize::HumanTime::from(dt).to_string()
}

pub fn first_sentence(text: &str) -> &str {
    if let Some(pos) = text.find('.') {
        &text[..=pos]
    } else {
        text.split_at(100).0
    }
}

pub fn serialize_search<T: Serialize>(pagination: &PaginatedRequest, query: T) -> String {
    let q = serde_urlencoded::to_string(query).unwrap_or_default();
    let p = serde_urlencoded::to_string(pagination).unwrap_or_default();
    if q.is_empty() {
        p
    } else if p.is_empty() {
        q
    } else {
        format!("{}&{}", q, p)
    }
}