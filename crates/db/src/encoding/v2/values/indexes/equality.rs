//! Stored values for lifecycle-managed equality indexes.

use bytes::Bytes;

use crate::encoding::error::EncodingError;
use crate::index_v2::work::SecondaryEntryValue;

use super::{encode_value, WorkValue};

pub(crate) fn encode_entry(value: &SecondaryEntryValue) -> Result<Bytes, EncodingError> {
    if !value.lane.is_equality() {
        return Err(EncodingError::Custom(
            "range lane cannot use the equality value codec".to_string(),
        ));
    }
    Ok(encode_value(&WorkValue::SecondaryEntry(*value)))
}

pub(super) fn validate_entry(
    value: SecondaryEntryValue,
) -> Result<SecondaryEntryValue, EncodingError> {
    if !value.lane.is_equality() {
        return Err(EncodingError::Custom(
            "range lane cannot use the equality value codec".to_string(),
        ));
    }
    Ok(value)
}
