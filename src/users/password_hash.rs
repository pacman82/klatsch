use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier as _,
    password_hash::{SaltString, rand_core::OsRng},
};

/// Generates a password hash for the given password. The hash should be stored rather than a
/// password, in order to avoid leaking the password in case of a data breach.
pub fn generate(password: &str) -> String {
    let salt = SaltString::generate(OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), salt.as_salt())
        .unwrap()
        .to_string()
}

/// `True` if the password matches the hash, `false` otherwise.
pub fn verify(password: &str, hash: &str) -> bool {
    let password_hash = PasswordHash::new(hash)
        .expect("Persisted password hash must be valid, utf-8 encoded PHC hash");
    Argon2::default()
        .verify_password(password.as_bytes(), &password_hash)
        .is_ok()
}
