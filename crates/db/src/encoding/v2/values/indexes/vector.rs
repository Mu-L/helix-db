//! Stored values for lifecycle-managed vector mappings.

use bytes::Bytes;

use crate::encoding::error::EncodingError;
use crate::index_v2::work::VectorPartitionMappingValue;

use super::{decode_value, encode_value, WorkValue};

pub(crate) fn encode_partition_mapping(value: &VectorPartitionMappingValue) -> Bytes {
    encode_value(&WorkValue::VectorPartitionMapping(value.clone()))
}

pub(crate) fn decode_partition_mapping(
    value: &[u8],
) -> Result<VectorPartitionMappingValue, EncodingError> {
    let WorkValue::VectorPartitionMapping(value) = decode_value(value)? else {
        return Err(EncodingError::Custom(
            "vector partition mapping key contains another value kind".to_string(),
        ));
    };
    Ok(value)
}
