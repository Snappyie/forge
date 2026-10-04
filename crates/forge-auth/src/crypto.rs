//! Reversible encryption for values the server must send back to a third party.
//!
//! An OIDC client secret is the motivating case: the server has to present it to
//! the provider's token endpoint, so a one-way hash is useless for it — but a
//! plaintext column is a plaintext credential in a database dump.
//!
//! The construction is AES-256-GCM with a random nonce per message. GCM is
//! authenticated, so a tampered ciphertext fails to decrypt rather than
//! decrypting to attacker-chosen bytes — which matters because the plaintext is
//! a credential that goes straight into an outbound HTTP request.
//!
//! **Not a general-purpose secret store.** It protects a credential the server
//! must replay. Anything that can be stored hashed should be, and
//! `redesign.md` §G's broader secret management is separate work.

use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use rand::RngCore;

use crate::AuthError;

/// Encrypts a value with AES-256-GCM.
///
/// The key is derived from the deployment's secret with SHA-256 so that any
/// passphrase length is accepted, while the cipher still gets exactly the 32
/// bytes AES-256 requires.
pub fn encrypt(plaintext: &str, secret: &[u8]) -> Result<Vec<u8>, AuthError> {
    let key = derive_key(secret)?;
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key));

    // A fresh 96-bit nonce per message. Reusing one under the same key is
    // catastrophic for GCM — it leaks the XOR of plaintexts and breaks
    // authentication — so it is drawn from the OS RNG every time.
    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|_| AuthError::Crypto("encryption failed".into()))?;

    let mut out = Vec::with_capacity(nonce_bytes.len() + ciphertext.len());
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

/// Decrypts a value produced by [`encrypt`].
///
/// Fails on a wrong key, a modified nonce, or modified ciphertext: GCM
/// authenticates all three.
pub fn decrypt(ciphertext: &[u8], secret: &[u8]) -> Result<String, AuthError> {
    if ciphertext.len() <= 12 {
        return Err(AuthError::Crypto("ciphertext is too short".into()));
    }
    let key = derive_key(secret)?;
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key));

    let nonce = Nonce::from_slice(&ciphertext[..12]);
    let plaintext = cipher
        .decrypt(nonce, &ciphertext[12..])
        .map_err(|_| AuthError::Crypto("decryption failed".into()))?;

    String::from_utf8(plaintext)
        .map_err(|_| AuthError::Crypto("decrypted value is not valid UTF-8".into()))
}

/// Expands an arbitrary-length secret to the 32 bytes AES-256 needs.
fn derive_key(secret: &[u8]) -> Result<[u8; 32], AuthError> {
    if secret.is_empty() {
        // Failing closed rather than deriving a key from nothing: an empty
        // secret would otherwise produce a valid, universally-known key.
        return Err(AuthError::Crypto("encryption secret is empty".into()));
    }
    Ok(<sha2::Sha256 as sha2::Digest>::digest(secret).into())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &[u8] = b"a-deployment-secret-of-some-length";

    #[test]
    fn a_value_survives_a_round_trip() {
        let original = "s3cr3t-client-secret";
        let sealed = encrypt(original, SECRET).unwrap();
        assert_eq!(decrypt(&sealed, SECRET).unwrap(), original);
    }

    #[test]
    fn the_ciphertext_is_not_the_plaintext() {
        let sealed = encrypt("s3cr3t-client-secret", SECRET).unwrap();
        assert!(
            !String::from_utf8_lossy(&sealed).contains("s3cr3t"),
            "the secret must not survive into the stored bytes"
        );
    }

    #[test]
    fn encrypting_the_same_value_twice_gives_different_bytes() {
        // A fresh nonce per message. Identical ciphertexts for identical inputs
        // would let an observer see which providers share a secret.
        let a = encrypt("same-value", SECRET).unwrap();
        let b = encrypt("same-value", SECRET).unwrap();
        assert_ne!(a, b, "nonce reuse must not happen");
        assert_eq!(decrypt(&a, SECRET).unwrap(), decrypt(&b, SECRET).unwrap());
    }

    #[test]
    fn the_wrong_key_does_not_decrypt() {
        let sealed = encrypt("s3cr3t", SECRET).unwrap();
        assert!(
            decrypt(&sealed, b"a-different-secret").is_err(),
            "a wrong key must fail rather than return garbage"
        );
    }

    #[test]
    fn a_modified_ciphertext_is_rejected() {
        // This is the property that makes GCM worth choosing: the plaintext is
        // a credential that goes into an outbound request, so a tampered
        // ciphertext must not become a different credential.
        let mut sealed = encrypt("s3cr3t-client-secret", SECRET).unwrap();
        let last = sealed.len() - 1;
        sealed[last] ^= 0xff;
        assert!(decrypt(&sealed, SECRET).is_err());
    }

    #[test]
    fn a_modified_nonce_is_rejected() {
        let mut sealed = encrypt("s3cr3t", SECRET).unwrap();
        sealed[0] ^= 0xff;
        assert!(decrypt(&sealed, SECRET).is_err());
    }

    #[test]
    fn an_empty_secret_fails_closed() {
        // Deriving a key from nothing would produce a valid cipher under a key
        // anyone knows, which is worse than refusing.
        assert!(encrypt("x", b"").is_err());
    }

    #[test]
    fn a_truncated_ciphertext_is_rejected_rather_than_panicking() {
        assert!(decrypt(&[0u8; 4], SECRET).is_err());
        assert!(decrypt(&[], SECRET).is_err());
    }

    #[test]
    fn any_secret_length_is_accepted() {
        for length in [1usize, 16, 32, 33, 128] {
            let secret = vec![b'k'; length];
            let sealed = encrypt("value", &secret).unwrap();
            assert_eq!(decrypt(&sealed, &secret).unwrap(), "value");
        }
    }
}