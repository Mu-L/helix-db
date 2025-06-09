use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use hex;
use ring::rand::{SecureRandom, SystemRandom};
use sha2::{Digest, Sha256};
use std::fmt;

/// Configuration for API key generation
#[derive(Debug, Clone)]
pub struct ApiKeyConfig {
    /// Length of the random bytes to generate (default: 32)
    pub key_length: usize,
    /// Prefix for the API key (e.g., "hx_", "api_")
    pub prefix: Option<String>,
    /// Whether to include a checksum for validation
    pub include_checksum: bool,
}

impl Default for ApiKeyConfig {
    fn default() -> Self {
        Self {
            key_length: 32,
            prefix: Some("hx_".to_string()),
            include_checksum: true,
        }
    }
}

/// A generated API key with metadata
#[derive(Debug, Clone)]
pub struct ApiKey {
    /// The actual API key string
    pub key: String,
    /// Hash of the key for storage (never store the raw key)
    pub key_hash: String,
    /// Optional key ID for reference
    pub key_id: Option<String>,
}

impl fmt::Display for ApiKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.key)
    }
}

/// Generates a cryptographically secure API key
///
/// # Arguments
/// * `config` - Configuration for key generation
///
/// # Returns
/// * `Result<ApiKey, KeyGenError>` - The generated API key or an error
///
/// # Example
/// ```
/// use helixdb::helix_gateway::auth::key_gen::{generate_api_key, ApiKeyConfig};
///
/// let config = ApiKeyConfig::default();
/// let api_key = generate_api_key(&config).unwrap();
/// println!("Generated API key: {}", api_key.key);
/// ```
pub fn generate_api_key(config: &ApiKeyConfig) -> Result<ApiKey, KeyGenError> {
    let rng = SystemRandom::new();

    // Generate random bytes
    let mut random_bytes = vec![0u8; config.key_length];
    rng.fill(&mut random_bytes)
        .map_err(|_| KeyGenError::RandomGenerationFailed)?;

    // Encode to base64url (URL-safe, no padding)
    let encoded = URL_SAFE_NO_PAD.encode(&random_bytes);

    // Add checksum if requested
    let key_with_checksum = if config.include_checksum {
        let checksum = generate_checksum(&encoded);
        format!("{}_{}", encoded, checksum)
    } else {
        encoded.to_string()
    };

    // Add prefix if specified
    let final_key = if let Some(ref prefix) = config.prefix {
        format!("{}{}", prefix, key_with_checksum)
    } else {
        key_with_checksum
    };

    // Generate hash for storage
    let key_hash = hash_api_key(&final_key);

    // Generate a key ID (first 8 chars of the hash)
    let key_id = Some(key_hash[..8].to_string());

    Ok(ApiKey {
        key: final_key,
        key_hash,
        key_id,
    })
}

/// Generates a simple API key with default configuration
///
/// # Returns
/// * `Result<ApiKey, KeyGenError>` - The generated API key or an error
///
/// # Example
/// ```
/// use helixdb::helix_gateway::auth::key_gen::generate_simple_api_key;
///
/// let api_key = generate_simple_api_key().unwrap();
/// println!("Generated API key: {}", api_key.key);
/// ```
pub fn generate_simple_api_key() -> Result<ApiKey, KeyGenError> {
    generate_api_key(&ApiKeyConfig::default())
}

/// Validates an API key by checking its checksum (if present)
///
/// # Arguments
/// * `api_key` - The API key to validate
///
/// # Returns
/// * `bool` - True if the key is valid, false otherwise
pub fn validate_api_key(api_key: &str) -> bool {
    // Remove prefix if present
    let key_without_prefix = if api_key.starts_with("hx_") {
        &api_key[3..]
    } else {
        api_key
    };

    // Check if it has a checksum (contains an underscore)
    if let Some(underscore_pos) = key_without_prefix.rfind('_') {
        let (encoded_part, checksum_part) = key_without_prefix.split_at(underscore_pos);
        let expected_checksum = generate_checksum(encoded_part);
        let provided_checksum = &checksum_part[1..]; // Remove the underscore

        expected_checksum == provided_checksum
    } else {
        // No checksum, assume valid if it's valid base64url
        URL_SAFE_NO_PAD.decode(key_without_prefix).is_ok()
    }
}

/// Hashes an API key for secure storage
///
/// # Arguments
/// * `api_key` - The API key to hash
///
/// # Returns
/// * `String` - Hex-encoded SHA256 hash of the key
pub fn hash_api_key(api_key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(api_key.as_bytes());
    hex::encode(hasher.finalize())
}

/// Generates a checksum for an encoded key part
fn generate_checksum(encoded: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(encoded.as_bytes());
    let hash = hasher.finalize();
    // Take first 4 bytes and encode as hex
    hex::encode(&hash[..4])
}

/// Errors that can occur during key generation
#[derive(Debug, Clone)]
pub enum KeyGenError {
    RandomGenerationFailed,
    InvalidConfiguration(String),
}

impl fmt::Display for KeyGenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KeyGenError::RandomGenerationFailed => {
                write!(f, "Failed to generate random bytes for API key")
            }
            KeyGenError::InvalidConfiguration(msg) => {
                write!(f, "Invalid key generation configuration: {}", msg)
            }
        }
    }
}

impl std::error::Error for KeyGenError {}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_generate_api_key_default() {
        let api_key = generate_simple_api_key().unwrap();
        assert!(api_key.key.starts_with("hx_"));
        assert!(!api_key.key_hash.is_empty());
        assert!(api_key.key_id.is_some());
    }

    #[test]
    fn test_generate_api_key_custom_config() {
        let config = ApiKeyConfig {
            key_length: 16,
            prefix: Some("api".to_string()),
            include_checksum: false,
        };

        let api_key = generate_api_key(&config).unwrap();
        assert!(api_key.key.starts_with("api"));
        assert!(!api_key.key.contains('_'), "Should not contain checksum");
    }

    #[test]
    fn test_validate_api_key() {
        let api_key = generate_simple_api_key().unwrap();
        assert!(validate_api_key(&api_key.key));

        // Test invalid key
        assert!(!validate_api_key("hx_invalid_key"));
    }

    #[test]
    fn test_hash_api_key() {
        let key = "test_key";
        let hash1 = hash_api_key(key);
        let hash2 = hash_api_key(key);

        assert_eq!(hash1, hash2);
        assert_eq!(hash1.len(), 64); // SHA256 hex length
    }

    #[test]
    fn test_key_uniqueness() {
        let key1 = generate_simple_api_key().unwrap();
        let key2 = generate_simple_api_key().unwrap();

        assert_ne!(key1.key, key2.key);
        assert_ne!(key1.key_hash, key2.key_hash);
    }
}
