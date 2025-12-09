use crate::protocol::HelixError;
use std::sync::LazyLock;

/// API KEY HASH (bcrypt hash read from HELIX_API_KEY env var on startup)
static API_KEY: LazyLock<String> =
    LazyLock::new(|| std::env::var("HELIX_API_KEY").unwrap_or_default());

#[inline(always)]
pub(crate) fn verify_key(key: &str) -> Result<(), HelixError> {
    if API_KEY.is_empty() {
        return Err(HelixError::InvalidApiKey);
    }
    match bcrypt::verify(key, &*API_KEY) {
        Ok(true) => Ok(()),
        Ok(false) => Err(HelixError::InvalidApiKey),
        Err(_) => Err(HelixError::InvalidApiKey),
    }
}

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use super::*;

    // ============================================================================
    // Key Verification Tests
    // ============================================================================

    #[test]
    fn test_bcrypt_verify_correct_key() {
        // Generate a bcrypt hash for testing
        let test_key = "test-api-key-12345";
        let hash = bcrypt::hash(test_key, bcrypt::DEFAULT_COST).unwrap();

        // Verify that bcrypt::verify works correctly
        assert!(bcrypt::verify(test_key, &hash).unwrap());
    }

    #[test]
    fn test_bcrypt_verify_wrong_key() {
        let test_key = "test-api-key-12345";
        let wrong_key = "wrong-api-key";
        let hash = bcrypt::hash(test_key, bcrypt::DEFAULT_COST).unwrap();

        // Verify that wrong key fails
        assert!(!bcrypt::verify(wrong_key, &hash).unwrap());
    }

    #[test]
    fn test_bcrypt_verify_empty_key() {
        let test_key = "test-api-key-12345";
        let hash = bcrypt::hash(test_key, bcrypt::DEFAULT_COST).unwrap();

        // Empty key should not verify
        assert!(!bcrypt::verify("", &hash).unwrap());
    }

    #[test]
    fn test_bcrypt_verify_similar_key() {
        let test_key = "test-api-key-12345";
        let similar_key = "test-api-key-12346"; // Off by one character
        let hash = bcrypt::hash(test_key, bcrypt::DEFAULT_COST).unwrap();

        // Similar key should not verify
        assert!(!bcrypt::verify(similar_key, &hash).unwrap());
    }

    #[test]
    fn test_bcrypt_hash_format() {
        let test_key = "test-api-key";
        let hash = bcrypt::hash(test_key, bcrypt::DEFAULT_COST).unwrap();

        // bcrypt hashes start with $2b$ (or $2a$ or $2y$)
        assert!(hash.starts_with("$2"));
        // bcrypt hashes are 60 characters long
        assert_eq!(hash.len(), 60);
    }

    #[test]
    fn test_verify_key_invalid_hash_format() {
        // If the stored hash is invalid, verify should fail gracefully
        let result = bcrypt::verify("any-key", "not-a-valid-bcrypt-hash");
        assert!(result.is_err());
    }
}
