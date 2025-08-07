use argon2::{password_hash::{rand_core::OsRng, SaltString}, Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use anyhow::{Result, anyhow};
pub mod extract_session;
pub mod s3;
use base64::Engine;
pub use extract_session::ExtractSession;

mod re {
    macro_rules! re {
        ($r:expr) => {
            std::sync::LazyLock::new(|| regex::Regex::new($r).unwrap())
        };
    }

    pub(crate) use re;
}

pub(crate) use re::re;

/// Hash plaintext using Argon2 and return the hash as a string.
pub fn hash(plaintext: &str) -> Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let hashed = argon2.hash_password(plaintext.as_bytes(), &salt).map_err(|_| anyhow!("Failed to hash passowrd"))?;
    Ok(hashed.to_string())
}

/// Verify plaintext against a hash.
pub fn verify(password: &str, hash: &str) -> Result<bool> {
    let parsed_hash = PasswordHash::new(hash).map_err(|_| anyhow!("Could not read hash"))?;
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

#[derive(Debug, Clone)]
pub struct AppState {
    pub pool: sqlx::PgPool,
    pub s3: s3::S3Config,
}