//! Stored values for lifecycle-managed equality indexes.

use std::io::Cursor;

use bytes::Bytes;
use roaring::RoaringTreemap;

use crate::encoding::error::EncodingError;
use crate::index_lifecycle::work::SecondaryEntryValue;

use super::{encode_value, WorkValue};

/// Portable V4 value for one non-unique equality key.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SecondaryEqualityBitmapValue(RoaringTreemap);

impl SecondaryEqualityBitmapValue {
    pub(crate) fn new(ids: RoaringTreemap) -> Self {
        Self(ids)
    }

    pub(crate) fn encode(&self) -> Bytes {
        let mut bytes = Vec::new();
        self.0
            .serialize_into(&mut bytes)
            .expect("serializing a RoaringTreemap into memory is infallible");
        Bytes::from(bytes)
    }

    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, EncodingError> {
        let mut cursor = Cursor::new(bytes);
        let ids = RoaringTreemap::deserialize_from(&mut cursor).map_err(|error| {
            EncodingError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("failed to decode secondary equality bitmap: {error}"),
            ))
        })?;
        if cursor.position() != bytes.len() as u64 {
            return Err(EncodingError::Custom(format!(
                "secondary equality bitmap has {} trailing bytes",
                bytes.len() as u64 - cursor.position()
            )));
        }
        Ok(Self(ids))
    }

    pub(crate) fn ids(&self) -> &RoaringTreemap {
        &self.0
    }

    pub(crate) fn into_ids(self) -> RoaringTreemap {
        self.0
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitmap_value_round_trips_portable_roaring_bytes() {
        let ids = RoaringTreemap::from_iter([1, 9, u64::from(u32::MAX) + 1]);
        let encoded = SecondaryEqualityBitmapValue::new(ids.clone()).encode();
        let decoded = SecondaryEqualityBitmapValue::decode(&encoded).unwrap();

        assert_eq!(decoded.ids(), &ids);
        assert!(SecondaryEqualityBitmapValue::decode(b"not roaring").is_err());
        let mut trailing = encoded.to_vec();
        trailing.push(0);
        assert!(SecondaryEqualityBitmapValue::decode(&trailing).is_err());
    }
}
