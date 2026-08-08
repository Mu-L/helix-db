//! Generation-qualified equality and deployed V3 secondary-row keys.

use bytes::{BufMut, Bytes};

use crate::encoding::error::EncodingError;
use crate::encoding::indexes::range::RangeIndexDirection;
use crate::encoding::v1::property::equality_value::{CanonicalEqualityValue, EQUALITY_DIGEST_LEN};
use crate::encoding::v1::property::range_value::CanonicalRangeValue;
use crate::index_lifecycle::{IndexElementKind, IndexEntityId, IndexGenerationId, IndexId};

use super::super::{KEY_MAX_LEN, KIND_LEN, PREFIX_LEN, U32_LEN, U64_LEN};

/// Frozen generation-qualified V3 secondary lanes.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum SecondaryEntryLane {
    NodeEquality = 0x01,
    NodeUniqueEquality = 0x02,
    NodeRangeAscending = 0x03,
    NodeRangeDescending = 0x04,
    EdgeEquality = 0x05,
    EdgeRangeAscending = 0x06,
    EdgeRangeDescending = 0x07,
}

impl SecondaryEntryLane {
    pub(crate) const fn as_u8(self) -> u8 {
        self as u8
    }

    pub(crate) fn try_from_u8(value: u8) -> Result<Self, EncodingError> {
        match value {
            0x01 => Ok(Self::NodeEquality),
            0x02 => Ok(Self::NodeUniqueEquality),
            0x03 => Ok(Self::NodeRangeAscending),
            0x04 => Ok(Self::NodeRangeDescending),
            0x05 => Ok(Self::EdgeEquality),
            0x06 => Ok(Self::EdgeRangeAscending),
            0x07 => Ok(Self::EdgeRangeDescending),
            unknown => Err(EncodingError::InvalidKey(format!(
                "unknown V2 secondary lane {unknown:#04x}"
            ))),
        }
    }

    pub(crate) const fn is_unique(self) -> bool {
        matches!(self, Self::NodeUniqueEquality)
    }

    pub(crate) const fn is_equality(self) -> bool {
        matches!(
            self,
            Self::NodeEquality | Self::NodeUniqueEquality | Self::EdgeEquality
        )
    }

    pub(crate) const fn range_direction(self) -> Option<RangeIndexDirection> {
        match self {
            Self::NodeRangeAscending | Self::EdgeRangeAscending => Some(RangeIndexDirection::Asc),
            Self::NodeRangeDescending | Self::EdgeRangeDescending => {
                Some(RangeIndexDirection::Desc)
            }
            Self::NodeEquality | Self::NodeUniqueEquality | Self::EdgeEquality => None,
        }
    }
}

/// Canonical secondary value bytes whose shape is fixed by the lane.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum CanonicalSecondaryValue {
    Equality(CanonicalEqualityValue),
    Range(CanonicalRangeValue),
}

impl CanonicalSecondaryValue {
    pub(crate) const fn equality(value: CanonicalEqualityValue) -> Self {
        Self::Equality(value)
    }

    #[cfg(test)]
    pub(crate) fn equality_string(value: &str) -> Self {
        let crate::encoding::v1::property::equality_value::EqualityValueProjection::Indexed(value) =
            crate::encoding::v1::property::equality_value::project_equality_value(
                &crate::encoding::property::property_value::PropertyValue::String(
                    value.to_string(),
                ),
            )
        else {
            panic!("string equality fixtures are always indexable");
        };
        Self::Equality(value)
    }

    pub(crate) const fn range(value: CanonicalRangeValue) -> Self {
        Self::Range(value)
    }

    #[cfg(test)]
    pub(crate) fn range_string(direction: RangeIndexDirection, value: &str) -> Self {
        let crate::encoding::v1::property::range_value::RangeValueProjection::Indexed(value) =
            crate::encoding::v1::property::range_value::project_range_value(
                &crate::encoding::property::property_value::PropertyValue::String(
                    value.to_string(),
                ),
                direction,
            )
        else {
            panic!("string range fixtures are always indexable");
        };
        Self::Range(value)
    }

    pub(crate) fn try_encoded_range(
        direction: RangeIndexDirection,
        value: Bytes,
    ) -> Result<Self, EncodingError> {
        Ok(Self::Range(CanonicalRangeValue::try_from_encoded(
            direction, value,
        )?))
    }

    pub(crate) fn encoded_key_len(&self) -> usize {
        match self {
            Self::Equality(value) => EQUALITY_DIGEST_LEN + U32_LEN + value.canonical().len(),
            Self::Range(value) => value.encoded().len(),
        }
    }

    pub(crate) fn encode_key_value<B: BufMut>(&self, buffer: &mut B) {
        match self {
            Self::Equality(value) => {
                buffer.put_slice(value.digest());
                buffer.put_u32(
                    u32::try_from(value.canonical().len())
                        .expect("canonical equality values are bounded below u32"),
                );
                buffer.put_slice(value.canonical());
            }
            Self::Range(value) => buffer.put_slice(value.encoded()),
        }
    }
}

/// Deployed V3 generation-qualified secondary entry key.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct SecondaryEntryKey {
    pub(crate) index_id: IndexId,
    pub(crate) generation: IndexGenerationId,
    pub(crate) lane: SecondaryEntryLane,
    pub(crate) value: CanonicalSecondaryValue,
    pub(crate) entity_id: Option<IndexEntityId>,
}

impl SecondaryEntryKey {
    pub(crate) fn try_new(
        index_id: IndexId,
        generation: IndexGenerationId,
        lane: SecondaryEntryLane,
        value: CanonicalSecondaryValue,
        entity_id: Option<IndexEntityId>,
    ) -> Result<Self, EncodingError> {
        let value_matches_lane = matches!(
            (lane.is_equality(), &value),
            (true, CanonicalSecondaryValue::Equality(_))
                | (false, CanonicalSecondaryValue::Range(_))
        );
        if !value_matches_lane || lane.is_unique() != entity_id.is_none() {
            return Err(EncodingError::InvalidKey(
                "secondary lane/value/entity shape mismatch".to_string(),
            ));
        }
        if let (Some(direction), CanonicalSecondaryValue::Range(value)) =
            (lane.range_direction(), &value)
            && direction != value.direction()
        {
            return Err(EncodingError::InvalidKey(
                "secondary range lane/value direction mismatch".to_string(),
            ));
        }
        let encoded_len = PREFIX_LEN
            + KIND_LEN
            + U64_LEN
            + U64_LEN
            + KIND_LEN
            + value.encoded_key_len()
            + entity_id.map_or(0, |_| U64_LEN);
        if encoded_len > KEY_MAX_LEN {
            return Err(EncodingError::InvalidKey(
                "secondary V2 key exceeds 1 MiB".to_string(),
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
}

/// V4 non-unique equality key with all matching entities stored in its value.
///
/// The digest accelerates comparisons but is not authoritative. Construction
/// and parsing both retain the complete typed canonical value, whose codec
/// validates that the digest matches those bytes.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct SecondaryEqualityBitmapKey {
    pub(crate) index_id: IndexId,
    pub(crate) generation: IndexGenerationId,
    pub(crate) element_kind: IndexElementKind,
    pub(crate) value: CanonicalEqualityValue,
}

impl SecondaryEqualityBitmapKey {
    pub(crate) fn try_new(
        index_id: IndexId,
        generation: IndexGenerationId,
        element_kind: IndexElementKind,
        value: CanonicalEqualityValue,
    ) -> Result<Self, EncodingError> {
        let encoded_len = PREFIX_LEN
            + KIND_LEN
            + U64_LEN
            + U64_LEN
            + KIND_LEN
            + EQUALITY_DIGEST_LEN
            + U32_LEN
            + value.canonical().len();
        if encoded_len > KEY_MAX_LEN {
            return Err(EncodingError::InvalidKey(
                "secondary equality bitmap key exceeds 1 MiB".to_string(),
            ));
        }
        Ok(Self {
            index_id,
            generation,
            element_kind,
            value,
        })
    }

    pub(crate) fn encoded_suffix_len(&self) -> usize {
        U64_LEN + U64_LEN + KIND_LEN + EQUALITY_DIGEST_LEN + U32_LEN + self.value.canonical().len()
    }

    pub(crate) fn encode_suffix<B: BufMut>(&self, buffer: &mut B) {
        buffer.put_u64(self.index_id.get());
        buffer.put_u64(self.generation.get());
        buffer.put_u8(self.element_kind as u8);
        buffer.put_slice(self.value.digest());
        buffer.put_u32(
            u32::try_from(self.value.canonical().len())
                .expect("canonical equality values are bounded below u32"),
        );
        buffer.put_slice(self.value.canonical());
    }
}
