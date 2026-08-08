//! Stored values for lifecycle-managed range indexes.

use bytes::Bytes;

use crate::encoding::error::EncodingError;
use crate::index_lifecycle::work::SecondaryEntryValue;

use super::{encode_value, WorkValue};

pub(crate) fn encode_entry(value: &SecondaryEntryValue) -> Result<Bytes, EncodingError> {
    if value.lane.is_equality() {
        return Err(EncodingError::Custom(
            "equality lane cannot use the range value codec".to_string(),
        ));
    }
    Ok(encode_value(&WorkValue::SecondaryEntry(*value)))
}

pub(super) fn validate_entry(
    value: SecondaryEntryValue,
) -> Result<SecondaryEntryValue, EncodingError> {
    if value.lane.is_equality() {
        return Err(EncodingError::Custom(
            "equality lane cannot use the range value codec".to_string(),
        ));
    }
    Ok(value)
}
