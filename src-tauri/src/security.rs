use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use rand_core::OsRng;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SecurityError {
    #[error("échec hachage mot de passe: {0}")]
    Hash(String),
    #[error("mot de passe incorrect")]
    InvalidPassword,
}

pub fn hash_password(password: &str) -> Result<String, SecurityError> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    argon2
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| SecurityError::Hash(e.to_string()))
}

pub fn verify_password(password: &str, hash: &str) -> Result<(), SecurityError> {
    let parsed = PasswordHash::new(hash).map_err(|_| SecurityError::InvalidPassword)?;
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .map_err(|_| SecurityError::InvalidPassword)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_and_verify_roundtrip() {
        let h = hash_password("SecretPass1").unwrap();
        assert!(verify_password("SecretPass1", &h).is_ok());
        assert!(verify_password("wrong", &h).is_err());
    }
}
