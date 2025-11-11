use crate::protocol::HelixError;
use std::sync::LazyLock;
use subtle::ConstantTimeEq;

static API_KEY: LazyLock<&'static [u8]> = LazyLock::new(|| {
    std::env::var("HELIX_API_KEY")
        .expect("HELIX_API_KEY must be set")
        .leak()
        .as_bytes()
});

pub(crate) fn verify_key(key: &[u8]) -> Result<(), HelixError> {
    if API_KEY.ct_eq(key).into() {
        Ok(())
    } else {
        Err(HelixError::InvalidApiKey)
    }
}
