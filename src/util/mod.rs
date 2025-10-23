use crate::err::{AppResult, internal_error};
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
                    Ok(Self::new(state.pool.clone()))
                }
            }
        };
    }

    pub(crate) use repo_from_parts;
}

pub(crate) use repo::repo_from_parts;

/// Hash plaintext using Argon2 and return the hash as a string.
pub fn hash(plaintext: &str) -> AppResult<String> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let hashed = argon2
        .hash_password(plaintext.as_bytes(), &salt)
        .map_err(|_| internal_error("Failed to hash passowrd"))?;
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
