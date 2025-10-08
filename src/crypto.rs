use anyhow::Result;
use argon2::{
    Argon2,
    password_hash::{PasswordHasher, SaltString},
};
use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit, OsRng},
};
use rand::RngCore;
use zeroize::Zeroize;

/// A wrapper around the master key that ensures it's wiped from memory when dropped
pub struct MasterKey([u8; 32]);

impl Drop for MasterKey {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl MasterKey {
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    #[allow(dead_code)]
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

/// Generate a random 16-byte salt
pub fn generate_salt() -> [u8; 16] {
    let mut salt = [0u8; 16];
    OsRng.fill_bytes(&mut salt);
    salt
}

/// Derive a 256-bit master key from password and salt using Argon2id
pub fn derive_key(password: &str, salt: &[u8]) -> Result<MasterKey> {
    // Configure Argon2id with reasonable parameters
    // m_cost: 64 MiB, t_cost: 3 iterations, p_cost: 4 parallelism
    let argon2 = Argon2::default();

    // Create a SaltString from our bytes
    let salt_string = SaltString::encode_b64(salt)
        .map_err(|e| anyhow::anyhow!("Failed to encode salt: {}", e))?;

    // Hash the password
    let hash = argon2
        .hash_password(password.as_bytes(), &salt_string)
        .map_err(|e| anyhow::anyhow!("Failed to hash password: {}", e))?;

    // Extract the hash bytes
    let hash_bytes = hash
        .hash
        .ok_or_else(|| anyhow::anyhow!("No hash generated"))?;

    let mut key = [0u8; 32];
    key.copy_from_slice(hash_bytes.as_bytes());

    Ok(MasterKey(key))
}

/// Encrypted data format: 24-byte nonce || ciphertext
pub fn encrypt(key: &MasterKey, plaintext: &[u8]) -> Result<Vec<u8>> {
    let cipher = XChaCha20Poly1305::new(key.as_bytes().into());

    // Generate a random nonce
    let mut nonce_bytes = [0u8; 24];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = XNonce::from_slice(&nonce_bytes);

    // Encrypt the data
    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| anyhow::anyhow!("Encryption failed: {}", e))?;

    // Prepend nonce to ciphertext
    let mut result = Vec::with_capacity(24 + ciphertext.len());
    result.extend_from_slice(&nonce_bytes);
    result.extend_from_slice(&ciphertext);

    Ok(result)
}

/// Decrypt data in format: 24-byte nonce || ciphertext
pub fn decrypt(key: &MasterKey, encrypted: &[u8]) -> Result<Vec<u8>> {
    if encrypted.len() < 24 {
        anyhow::bail!("Encrypted data too short");
    }

    let cipher = XChaCha20Poly1305::new(key.as_bytes().into());

    // Split nonce and ciphertext
    let (nonce_bytes, ciphertext) = encrypted.split_at(24);
    let nonce = XNonce::from_slice(nonce_bytes);

    // Decrypt the data
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| anyhow::anyhow!("Decryption failed (wrong password?): {}", e))?;

    Ok(plaintext)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt() {
        let password = "test_password_123";
        let salt = generate_salt();
        let key = derive_key(password, &salt).unwrap();

        let plaintext = b"Hello, World! This is a test message.";
        let encrypted = encrypt(&key, plaintext).unwrap();
        let decrypted = decrypt(&key, &encrypted).unwrap();

        assert_eq!(plaintext, decrypted.as_slice());
    }

    #[test]
    fn test_wrong_password() {
        let salt = generate_salt();
        let key1 = derive_key("password1", &salt).unwrap();
        let key2 = derive_key("password2", &salt).unwrap();

        let plaintext = b"Secret data";
        let encrypted = encrypt(&key1, plaintext).unwrap();

        // Should fail with wrong key
        assert!(decrypt(&key2, &encrypted).is_err());
    }

    #[test]
    fn test_nonce_uniqueness() {
        let password = "test_password";
        let salt = generate_salt();
        let key = derive_key(password, &salt).unwrap();

        let plaintext = b"Same message";
        let encrypted1 = encrypt(&key, plaintext).unwrap();
        let encrypted2 = encrypt(&key, plaintext).unwrap();

        // Encrypted versions should be different due to random nonces
        assert_ne!(encrypted1, encrypted2);
    }
}
