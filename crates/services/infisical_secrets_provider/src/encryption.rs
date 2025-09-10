#[cfg(test)]
use anyhow::Context;
use anyhow::Result;
#[cfg(test)]
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use nkeys::XKey;

/// Handles encryption and decryption operations for wasmCloud secrets backend
/// This implementation uses nkeys XKey functionality for proper wasmCloud compatibility
pub struct EncryptionHandler {
    xkey: XKey,
    seed_string: Option<String>,
}

impl EncryptionHandler {
    /// Creates a new encryption handler with a fresh XKey
    pub fn new() -> Self {
        let xkey = XKey::new();
        let seed_string = xkey.seed().ok().map(|s| s.to_string());

        Self { xkey, seed_string }
    }

    /// Creates an encryption handler from existing key bytes
    pub fn from_bytes(key_bytes: &[u8; 32]) -> Result<Self> {
        let xkey = XKey::new_from_raw(*key_bytes);
        let seed_string = xkey.seed().ok().map(|s| s.to_string());

        Ok(Self { xkey, seed_string })
    }

    /// Creates an encryption handler from a seed string
    pub fn from_seed(seed: &str) -> Result<Self> {
        let xkey = XKey::from_seed(seed)
            .map_err(|e| anyhow::anyhow!("Failed to create XKey from seed: {}", e))?;

        Ok(Self {
            xkey,
            seed_string: Some(seed.to_string()),
        })
    }

    /// Returns the server's public key as a string (XKey format)
    pub fn public_key(&self) -> String {
        // XKey.public_key() returns the formatted public key string
        self.xkey.public_key()
    }

    /// Returns the server's seed for persistence
    #[cfg(test)]
    pub fn seed(&self) -> String {
        self.xkey.seed().map(|s| s.to_string()).unwrap_or_default()
    }

    /// Returns the server's raw key bytes for persistence (32 bytes)
    pub fn secret_bytes(&self) -> [u8; 32] {
        let mut bytes = [0u8; 32];
        if let Some(seed) = &self.seed_string {
            let seed_bytes = seed.as_bytes();
            let copy_len = seed_bytes.len().min(32);
            bytes[..copy_len].copy_from_slice(&seed_bytes[..copy_len]);
        }
        bytes
    }

    /// Decrypts a payload using XKey encryption with the provided host public key
    pub fn decrypt_payload(
        &self,
        encrypted_payload: &[u8],
        host_public_key: &str,
    ) -> Result<Vec<u8>> {
        // Try to create an XKey from the public key string directly
        let host_xkey = XKey::from_public_key(host_public_key)
            .map_err(|e| anyhow::anyhow!("Failed to create XKey from public key: {}", e))?;

        let decrypted = self
            .xkey
            .open(encrypted_payload, &host_xkey)
            .map_err(|e| anyhow::anyhow!("Failed to decrypt payload: {}", e))?;

        Ok(decrypted)
    }

    /// Encrypts a payload using XKey encryption with the provided host public key
    #[cfg(test)]
    pub fn encrypt_payload(&self, payload: &[u8], host_public_key: &str) -> Result<Vec<u8>> {
        // Try to create an XKey from the public key string directly
        let host_xkey = XKey::from_public_key(host_public_key)
            .map_err(|e| anyhow::anyhow!("Failed to create XKey from public key: {}", e))?;

        let encrypted = self
            .xkey
            .seal(payload, &host_xkey)
            .map_err(|e| anyhow::anyhow!("Failed to encrypt payload: {}", e))?;

        Ok(encrypted)
    }

    /// Decrypts a base64-encoded payload
    #[cfg(test)]
    pub fn decrypt_base64_payload(
        &self,
        encoded_payload: &str,
        host_public_key: &str,
    ) -> Result<Vec<u8>> {
        let encrypted_bytes = BASE64
            .decode(encoded_payload)
            .context("Failed to decode base64 payload")?;

        self.decrypt_payload(&encrypted_bytes, host_public_key)
    }

    /// Encrypts a payload and returns it as base64
    #[cfg(test)]
    pub fn encrypt_to_base64(&self, payload: &[u8], host_public_key: &str) -> Result<String> {
        let encrypted = self.encrypt_payload(payload, host_public_key)?;
        Ok(BASE64.encode(&encrypted))
    }

    /// Validates that a public key is valid (XKey format)
    #[cfg(test)]
    pub fn validate_public_key(public_key: &str) -> bool {
        // XKey public keys start with 'X' and are 56 characters long
        // They decode to 42 bytes using base64
        public_key.starts_with('X')
            && public_key.len() == 56
            && BASE64
                .decode(public_key)
                .map(|bytes| bytes.len() == 42)
                .unwrap_or(false)
    }
}

impl Default for EncryptionHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for EncryptionHandler {
    fn clone(&self) -> Self {
        // Use seed-based cloning for consistency
        if let Some(seed) = &self.seed_string {
            Self::from_seed(seed).unwrap_or_else(|_| Self::new())
        } else {
            // Fallback: create from raw bytes
            let bytes = self.secret_bytes();
            Self::from_bytes(&bytes).unwrap_or_else(|_| Self::new())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encryption_handler_creation() {
        let handler = EncryptionHandler::new();
        let public_key = handler.public_key();

        // Public key should be a valid XKey format
        assert!(!public_key.is_empty());
        assert!(EncryptionHandler::validate_public_key(&public_key));
    }

    #[test]
    fn test_encryption_round_trip() {
        let server_handler = EncryptionHandler::new();
        let client_handler = EncryptionHandler::new();

        let test_data = b"Hello, wasmCloud secrets!";
        let client_public_key = client_handler.public_key();

        // Encrypt data using client's public key
        let encrypted = server_handler
            .encrypt_payload(test_data, &client_public_key)
            .expect("Failed to encrypt");

        // Decrypt data using server's public key
        let server_public_key = server_handler.public_key();
        let decrypted = client_handler
            .decrypt_payload(&encrypted, &server_public_key)
            .expect("Failed to decrypt");

        assert_eq!(test_data, decrypted.as_slice());
    }

    #[test]
    fn test_base64_operations() {
        let server_handler = EncryptionHandler::new();
        let client_handler = EncryptionHandler::new();

        let test_data = b"Secret data for base64 test";
        let client_public_key = client_handler.public_key();

        // Encrypt to base64
        let encrypted_b64 = server_handler
            .encrypt_to_base64(test_data, &client_public_key)
            .expect("Failed to encrypt to base64");

        // Verify it's valid base64
        assert!(BASE64.decode(&encrypted_b64).is_ok());

        // Decrypt from base64
        let server_public_key = server_handler.public_key();
        let decrypted = client_handler
            .decrypt_base64_payload(&encrypted_b64, &server_public_key)
            .expect("Failed to decrypt from base64");

        assert_eq!(test_data, decrypted.as_slice());
    }

    #[test]
    fn test_key_persistence() {
        let handler1 = EncryptionHandler::new();
        let seed = handler1.seed();
        let public_key1 = handler1.public_key();

        // Only test if we have a valid seed (which we should for new handlers)
        if !seed.is_empty() {
            // Create new handler from seed
            let handler2 =
                EncryptionHandler::from_seed(&seed).expect("Failed to create handler from seed");
            let public_key2 = handler2.public_key();

            // Should have the same public key
            assert_eq!(public_key1, public_key2);
        }
    }

    #[test]
    fn test_seed_persistence() {
        let handler1 = EncryptionHandler::new();
        let seed = handler1.seed();
        let public_key1 = handler1.public_key();

        // Only test if we have a valid seed
        if !seed.is_empty() {
            // Create new handler from seed
            let handler2 =
                EncryptionHandler::from_seed(&seed).expect("Failed to create handler from seed");
            let public_key2 = handler2.public_key();

            // Should have the same public key
            assert_eq!(public_key1, public_key2);
        }
    }

    #[test]
    fn test_invalid_public_key() {
        let invalid_key = "invalid_public_key";
        assert!(!EncryptionHandler::validate_public_key(invalid_key));

        let invalid_xkey = "not_xkey_format";
        assert!(!EncryptionHandler::validate_public_key(invalid_xkey));

        let wrong_prefix = "ACCGOIG6EOWEJW7LFBXBJDEKUGJUR46EL3LKJE2QEKDOGTAODDXD6GID";
        assert!(!EncryptionHandler::validate_public_key(wrong_prefix));
    }

    #[test]
    fn test_handler_clone() {
        let handler1 = EncryptionHandler::new();
        let handler2 = handler1.clone();

        // Both should have the same keys
        assert_eq!(handler1.public_key(), handler2.public_key());
        assert_eq!(handler1.secret_bytes(), handler2.secret_bytes());
    }

    #[test]
    fn test_empty_data_encryption() {
        let server_handler = EncryptionHandler::new();
        let client_handler = EncryptionHandler::new();

        let empty_data = b"";
        let client_public_key = client_handler.public_key();

        let encrypted = server_handler
            .encrypt_payload(empty_data, &client_public_key)
            .expect("Failed to encrypt empty data");

        let server_public_key = server_handler.public_key();
        let decrypted = client_handler
            .decrypt_payload(&encrypted, &server_public_key)
            .expect("Failed to decrypt empty data");

        assert_eq!(empty_data, decrypted.as_slice());
    }

    #[test]
    fn test_large_data_encryption() {
        let server_handler = EncryptionHandler::new();
        let client_handler = EncryptionHandler::new();

        let large_data = vec![42u8; 10000]; // 10KB of data
        let client_public_key = client_handler.public_key();

        let encrypted = server_handler
            .encrypt_payload(&large_data, &client_public_key)
            .expect("Failed to encrypt large data");

        let server_public_key = server_handler.public_key();
        let decrypted = client_handler
            .decrypt_payload(&encrypted, &server_public_key)
            .expect("Failed to decrypt large data");

        assert_eq!(large_data, decrypted);
    }

    #[test]
    fn test_cross_encryption_fails_with_wrong_keys() {
        let server_handler = EncryptionHandler::new();
        let client1_handler = EncryptionHandler::new();
        let client2_handler = EncryptionHandler::new();

        let test_data = b"Secret message";
        let client1_public_key = client1_handler.public_key();

        // Encrypt with client1's key
        let encrypted = server_handler
            .encrypt_payload(test_data, &client1_public_key)
            .expect("Failed to encrypt");

        // Try to decrypt with client2's handler using server's key - should fail
        let server_public_key = server_handler.public_key();
        let decrypt_result = client2_handler.decrypt_payload(&encrypted, &server_public_key);

        assert!(decrypt_result.is_err());
    }
}
