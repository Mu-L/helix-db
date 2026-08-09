//! Stored values for lifecycle-managed vector mappings.

use bytes::Bytes;

use crate::encoding::error::EncodingError;
use crate::index_lifecycle::work::{VectorPartitionMappingValue, VectorTenantPartition};

use super::super::{
    model_error, put_generation, put_index_id, put_partition, take_generation, take_index_id,
    take_partition, unknown_discriminant, work_model_error, ValueDecoder, ValueEncoder,
};

const PARTITION_MAPPING_KIND: u8 = 0x0F;

pub(crate) fn encode_partition_mapping(value: &VectorPartitionMappingValue) -> Bytes {
    let mut encoder = ValueEncoder::with_header(PARTITION_MAPPING_KIND);
    put_index_id(&mut encoder, value.index_id);
    put_generation(&mut encoder, value.generation);
    put_partition(&mut encoder, value.partition.as_partition());
    encoder.put_u64(value.physical_index_id.get());
    encoder.finish()
}

pub(crate) fn decode_partition_mapping(
    value: &[u8],
) -> Result<VectorPartitionMappingValue, EncodingError> {
    let mut decoder = ValueDecoder::new(value)?;
    if decoder.kind() != PARTITION_MAPPING_KIND {
        return Err(unknown_discriminant(
            "vector partition mapping value kind",
            decoder.kind(),
        ));
    }
    let decoded = VectorPartitionMappingValue {
        index_id: take_index_id(&mut decoder)?,
        generation: take_generation(&mut decoder)?,
        partition: VectorTenantPartition::try_from_partition(take_partition(&mut decoder)?)
            .map_err(work_model_error)?,
        physical_index_id: crate::index_lifecycle::VectorPhysicalIndexId::new(decoder.take_u64()?)
            .map_err(model_error)?,
    };
    decoder.finish()?;
    Ok(decoded)
}

#[cfg(test)]
mod tests {
    use std::fmt::Write;

    use super::*;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().fold(String::new(), |mut output, byte| {
            write!(output, "{byte:02x}").expect("writing to String cannot fail");
            output
        })
    }

    #[test]
    fn partition_mapping_value_bytes_are_frozen() {
        let value = VectorPartitionMappingValue {
            index_id: crate::index_lifecycle::IndexId::new(1).unwrap(),
            generation: crate::index_lifecycle::IndexGenerationId::new(2).unwrap(),
            partition: VectorTenantPartition::try_new(Bytes::from_static(b"tenant")).unwrap(),
            physical_index_id: crate::index_lifecycle::VectorPhysicalIndexId::new(4).unwrap(),
        };
        let encoded = encode_partition_mapping(&value);

        assert_eq!(decode_partition_mapping(&encoded).unwrap(), value);
        insta::assert_snapshot!(
            hex(&encoded),
            @"010f00000000000000010000000000000002020000000674656e616e740000000000000004"
        );
    }
}
