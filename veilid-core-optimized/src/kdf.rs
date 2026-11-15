use crate::Error;
use argon2::{
    password_hash::{
        phc::{PasswordHash, Salt},
        PasswordHasher, PasswordVerifier,
    },
    Argon2,
};

/// Shortest salt Argon2 accepts, in bytes
pub const ARGON2_SALT_MIN_LENGTH: usize = Salt::MIN_LENGTH;
/// Longest salt Argon2 accepts, in bytes
pub const ARGON2_SALT_MAX_LENGTH: usize = Salt::MAX_LENGTH;

fn check_salt(salt: &[u8]) -> Result<(), Error> {
    if salt.len() < ARGON2_SALT_MIN_LENGTH || salt.len() > ARGON2_SALT_MAX_LENGTH {
        return Err(Error::Argon2("invalid salt length".to_owned()));
    }
    Ok(())
}

/// PHC string (`$argon2id$v=19$...`) of `password` hashed under `salt` with the default
/// Argon2id parameters
pub fn argon2_hash_password(password: &[u8], salt: &[u8]) -> Result<String, Error> {
    check_salt(salt)?;
    Argon2::default()
        .hash_password_with_salt(password, salt)
        .map(|h| h.to_string())
        .map_err(|e| Error::Argon2(e.to_string()))
}

/// Whether `password` matches the PHC string `password_hash`
pub fn argon2_verify_password(password: &[u8], password_hash: &str) -> Result<bool, Error> {
    let parsed =
        PasswordHash::new(password_hash).map_err(|e| Error::InvalidPasswordHash(e.to_string()))?;
    Ok(Argon2::default().verify_password(password, &parsed).is_ok())
}

/// Fill `out` with Argon2id key material derived from `password` under `salt`
pub fn argon2_derive_key(password: &[u8], salt: &[u8], out: &mut [u8]) -> Result<(), Error> {
    check_salt(salt)?;
    Argon2::default()
        .hash_password_into(password, salt, out)
        .map_err(|e| Error::Argon2(e.to_string()))
}
