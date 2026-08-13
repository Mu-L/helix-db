//! Generation-qualified range index keys.

use bytes::BufMut;

use crate::encoding::error::EncodingError;
use crate::encoding::v2::values::property::range_index_value::CanonicalRangeValue;
use crate::index_lifecycle::{IndexEntityId, IndexGenerationId, IndexId};

use super::super::{KEY_MAX_LEN, KIND_LEN, PREFIX_LEN, U64_LEN};
use super::SecondaryEntryLane;

/// Deployed V3 generation-qualified range entry key.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct SecondaryRangeEntryKey {
    pub(crate) index_id: IndexId,
    pub(crate) generation: IndexGenerationId,
    pub(crate) lane: SecondaryEntryLane,
    pub(crate) value: CanonicalRangeValue,
    pub(crate) entity_id: IndexEntityId,
}

impl SecondaryRangeEntryKey {
    pub(crate) fn try_new(
        index_id: IndexId,
        generation: IndexGenerationId,
        lane: SecondaryEntryLane,
        value: CanonicalRangeValue,
        entity_id: IndexEntityId,
    ) -> Result<Self, EncodingError> {
        let Some(direction) = lane.range_direction() else {
            return Err(EncodingError::InvalidKey(
                "secondary range key requires a range lane".to_string(),
            ));
        };
        if direction != value.direction() {
            return Err(EncodingError::InvalidKey(
                "secondary range lane/value direction mismatch".to_string(),
            ));
        }
        let encoded_len =
            PREFIX_LEN + KIND_LEN + U64_LEN + U64_LEN + KIND_LEN + value.encoded().len() + U64_LEN;
        if encoded_len > KEY_MAX_LEN {
            return Err(EncodingError::InvalidKey(
                "secondary range key exceeds 1 MiB".to_string(),
            ));
        }
        Ok(Self {
            index_id,
            generation,
            lane,
            value,
            entity_id,
        })
    }

    pub(crate) fn encoded_suffix_len(&self) -> usize {
        U64_LEN + U64_LEN + KIND_LEN + self.value.encoded().len() + U64_LEN
    }

    pub(crate) fn encode_suffix<B: BufMut>(&self, buffer: &mut B) {
        buffer.put_u64(self.index_id.get());
        buffer.put_u64(self.generation.get());
        buffer.put_u8(self.lane.as_u8());
        buffer.put_slice(self.value.encoded());
        buffer.put_u64(self.entity_id.get());
    }
}
