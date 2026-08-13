//! Stored values for secondary, text, and vector indexes.

pub(crate) mod equality;
mod equality_legacy;
pub(crate) mod range;
mod secondary_entry;
pub(crate) mod text;
#[cfg(any(test, feature = "fuzzing", feature = "production-coverage"))]
pub(crate) mod text_legacy;
pub mod vector;

pub(crate) use equality::SecondaryEqualityBitmapValue;
pub(crate) use equality_legacy::SecondaryEqualityValue;
pub(crate) use secondary_entry::{decode_secondary_entry, encode_secondary_entry};
