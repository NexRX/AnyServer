//! Authenticated encryption helpers for secrets stored at rest.
//!
//! Uses AES-256-GCM with a key derived from the JWT secret via HKDF-SHA256.
//! The encrypted format is:
//!
//! ```text
//! enc:v1:<base64url(nonce ‖ ciphertext ‖ tag)>
//! ```
//!
//! The `enc:v1:` prefix makes it trivial to distinguish encrypted values
//! from legacy plaintext without fragile content-guessing.

use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{AeadCore, Aes256Gcm, Key, Nonce};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use hkdf::Hkdf;
use sha2::Sha256;
use std::sync::OnceLock;

/// Prefix that identifies an encrypted value produced by this module.
const ENC_PREFIX: &str = "enc:v1:";

/// Fixed HKDF info/context string so the derived key is domain-separated
/// from other uses of the JWT secret (e.g. token signing).
const HKDF_INFO: &[u8] = b"anyserver-smtp-encryption";

/// Cached 256-bit encryption key derived from the JWT secret.
static ENC_KEY: OnceLock<[u8; 32]> = OnceLock::new();

/// Derive (and cache) the AES-256-GCM key from the JWT secret.
///
/// Must be called after [`crate::auth::init_jwt_secret`] — panics if the
/// JWT secret is not yet initialised.
fn encryption_key() -> &'static [u8; 32] {
    ENC_KEY.get_or_init(|| {
        let jwt_secret = crate::auth::jwt_secret();
        let hk = Hkdf::<Sha256>::new(None, jwt_secret);
        let mut okm = [0u8; 32];
        hk.expand(HKDF_INFO, &mut okm)
            .expect("HKDF-SHA256 expand failed for 32-byte output — this should never happen");
        okm
    })
}

/// Encrypt a plaintext string, returning the `enc:v1:…` envelope.
///
/// Returns an error only if AES-GCM encryption itself fails (extremely
/// unlikely with valid inputs).
pub fn encrypt(plaintext: &str) -> Result<String, String> {
    let key = Key::<Aes256Gcm>::from_slice(encryption_key());
    let cipher = Aes256Gcm::new(key);
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);

    let ciphertext = cipher
        .encrypt(&nonce, plaintext.as_bytes())
        .map_err(|e| format!("AES-GCM encryption failed: {e}"))?;

    // nonce (12 bytes) ‖ ciphertext+tag
    let mut blob = Vec::with_capacity(nonce.len() + ciphertext.len());
    blob.extend_from_slice(&nonce);
    blob.extend_from_slice(&ciphertext);

    Ok(format!("{}{}", ENC_PREFIX, URL_SAFE_NO_PAD.encode(&blob)))
}

/// Decrypt a value previously produced by [`encrypt`].
///
/// Returns `Err` if the value doesn't have the expected prefix, the
/// base64 is invalid, the nonce is missing, or authentication fails
/// (wrong key / tampered data).
pub fn decrypt(envelope: &str) -> Result<String, String> {
    let encoded = envelope
        .strip_prefix(ENC_PREFIX)
        .ok_or_else(|| "value is not encrypted (missing enc:v1: prefix)".to_string())?;

    let blob = URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|e| format!("base64 decode failed: {e}"))?;

    if blob.len() < 12 {
        return Err("encrypted blob too short (missing nonce)".into());
    }

    let (nonce_bytes, ciphertext) = blob.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);

    let key = Key::<Aes256Gcm>::from_slice(encryption_key());
    let cipher = Aes256Gcm::new(key);

    let plaintext = cipher.decrypt(nonce, ciphertext).map_err(|_| {
        "AES-GCM decryption failed — the JWT secret may have changed since this \
             value was encrypted. Re-save the SMTP configuration with the correct password."
            .to_string()
    })?;

    String::from_utf8(plaintext).map_err(|e| format!("decrypted value is not valid UTF-8: {e}"))
}

/// Returns `true` if `value` looks like it was produced by [`encrypt`].
pub fn is_encrypted(value: &str) -> bool {
    value.starts_with(ENC_PREFIX)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ensure the JWT secret is initialised for tests.
    fn ensure_jwt_secret() {
        // In the test binary the secret may already be initialised by
        // another test; `init_jwt_secret` is idempotent (OnceLock).
        let dir = std::env::temp_dir().join("anyserver-enc-test");
        let _ = std::fs::create_dir_all(&dir);
        crate::auth::init_jwt_secret(&dir);
    }

    #[test]
    fn round_trip() {
        ensure_jwt_secret();
        let original = "s3cret-smtp-p@ssword!";
        let encrypted = encrypt(original).expect("encrypt failed");
        assert!(encrypted.starts_with(ENC_PREFIX));
        assert_ne!(encrypted, original);

        let decrypted = decrypt(&encrypted).expect("decrypt failed");
        assert_eq!(decrypted, original);
    }

    #[test]
    fn is_encrypted_detection() {
        ensure_jwt_secret();
        assert!(!is_encrypted("plaintext-password"));
        assert!(!is_encrypted("$argon2id$some-hash"));
        assert!(is_encrypted("enc:v1:AAAA"));

        let encrypted = encrypt("test").unwrap();
        assert!(is_encrypted(&encrypted));
    }

    #[test]
    fn decrypt_rejects_plaintext() {
        ensure_jwt_secret();
        let result = decrypt("not-encrypted");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("missing enc:v1: prefix"));
    }

    #[test]
    fn decrypt_rejects_tampered_data() {
        ensure_jwt_secret();
        let encrypted = encrypt("hello").unwrap();

        // Flip a byte in the ciphertext portion
        let encoded = encrypted.strip_prefix(ENC_PREFIX).unwrap();
        let mut blob = URL_SAFE_NO_PAD.decode(encoded).unwrap();
        if let Some(last) = blob.last_mut() {
            *last ^= 0xFF;
        }
        let tampered = format!("{}{}", ENC_PREFIX, URL_SAFE_NO_PAD.encode(&blob));

        let result = decrypt(&tampered);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("decryption failed"));
    }

    #[test]
    fn different_encryptions_differ() {
        ensure_jwt_secret();
        let a = encrypt("same-password").unwrap();
        let b = encrypt("same-password").unwrap();
        // Different nonces → different ciphertexts
        assert_ne!(a, b);
        // But both decrypt to the same value
        assert_eq!(decrypt(&a).unwrap(), decrypt(&b).unwrap());
    }

    #[test]
    fn empty_string_round_trip() {
        ensure_jwt_secret();
        let encrypted = encrypt("").unwrap();
        assert_eq!(decrypt(&encrypted).unwrap(), "");
    }

    #[test]
    fn unicode_round_trip() {
        ensure_jwt_secret();
        let original = "pässwörd→✓";
        let encrypted = encrypt(original).unwrap();
        assert_eq!(decrypt(&encrypted).unwrap(), original);
    }
}
