//! Typed construction and parsing for the canonical V2 index namespace.
//!
//! Scoped records always begin with logical data prefix `0x06`; tenant bytes
//! are applied only by [`super::Key`]. Database-global records use the exact
//! seventeen-byte `0xFE` sentinel, so no parser guesses scope from arbitrary
//! bytes.

use bytes::{BufMut, Bytes};

use crate::encoding::error::EncodingError;
use crate::encoding::v1::property::equality_value::{CanonicalEqualityValue, EQUALITY_DIGEST_LEN};
use crate::index_lifecycle::{
    IndexComponent, IndexElementKind, IndexEntityId, IndexGenerationId, IndexId, IndexIdentity,
    IndexIdentityFamily, IndexOperationId,
};

use crate::encoding::v1::keys::tenant::DataScope;

pub(crate) mod global;
pub(crate) mod indexes;
pub(crate) mod lifecycle;

pub(crate) use global::{GlobalKey, GlobalKind, TextCompactionTarget, GLOBAL_SENTINEL};
pub(crate) use indexes::equality::{
    CanonicalSecondaryValue, SecondaryEntryKey, SecondaryEntryLane,
};
pub(crate) use indexes::text::{
    BlobHash, PartitionFingerprint, TextBuildArtifactKey, TextCorpusStatisticsKey,
    TextEntityStateKey, TextManifestPageKey, TextManifestRootKey, TextStatisticsEntityKey,
    TextTermFingerprint, TextTermStatisticsKey,
};
pub(crate) use indexes::vector::VectorPartitionMappingKey;
pub(crate) use lifecycle::{IndexEntity, IndexEntityStateKey, IndexOperationKey, IndexRecordKey};

const PREFIX_LEN: usize = core::mem::size_of::<u8>();
const KIND_LEN: usize = core::mem::size_of::<u8>();
const U32_LEN: usize = core::mem::size_of::<u32>();
const U64_LEN: usize = core::mem::size_of::<u64>();
pub(super) const UUID_LEN: usize = 16;
pub(super) const HASH_LEN: usize = 32;
/// Maximum complete cursor, logical-owner, or global reference key length.
pub(crate) const KEY_MAX_LEN: usize = 1024 * 1024;
const DATA_PREFIX: u8 = 0x06;

/// Complete physical key for one lifecycle-managed index record.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum Key {
    Global { kind: GlobalKey },
    Data { scope: DataScope, kind: ScopedKey },
}

impl Key {
    pub(crate) fn data_prefix(scope: DataScope, logical_prefix: Bytes) -> Bytes {
        match scope {
            DataScope::LegacyUnscoped => logical_prefix,
            DataScope::Tenant(tenant_id) => {
                let mut bytes = Vec::with_capacity(DataScope::PREFIX_LEN + logical_prefix.len());
                bytes.put_u128(tenant_id.as_u128());
                bytes.put_slice(&logical_prefix);
                Bytes::from(bytes)
            }
        }
    }

    pub(crate) fn parse_from_slice(scope: DataScope, slice: &[u8]) -> Result<Self, EncodingError> {
        let Some(logical) = scope.strip_key(slice) else {
            return Err(EncodingError::InvalidKey(
                "physical key does not match index data scope".to_string(),
            ));
        };
        Ok(Self::Data {
            scope,
            kind: ScopedKey::parse_from_slice(logical)?,
        })
    }

    pub(crate) fn to_bytes(&self) -> Bytes {
        let mut bytes = match self {
            Self::Global { kind } => Vec::with_capacity(kind.encoded_len()),
            Self::Data { scope, kind } => {
                Vec::with_capacity(scope.encoded_len() + kind.encoded_len())
            }
        };
        match self {
            Self::Global { kind } => kind.encode_into(&mut bytes),
            Self::Data { scope, kind } => {
                if let DataScope::Tenant(tenant_id) = scope {
                    bytes.put_u128(tenant_id.as_u128());
                }
                kind.encode_into(&mut bytes);
            }
        }
        Bytes::from(bytes)
    }
}

/// Frozen scoped/value record kinds.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum RecordKind {
    IndexRecord = 0x01,
    Operation = 0x02,
    BuildDelta = 0x03,
    AppliedState = 0x04,
    SecondaryEntry = 0x05,
    TextManifestRoot = 0x06,
    TextManifestPage = 0x07,
    TextBuildArtifact = 0x09,
    TextEntityState = 0x0C,
    VectorPartitionMapping = 0x0F,
    TextCorpusStatistics = 0x10,
    TextTermStatistics = 0x11,
    TextStatisticsEntity = 0x12,
}

impl RecordKind {
    pub(crate) const fn as_u8(self) -> u8 {
        self as u8
    }

    pub(crate) fn try_from_u8(value: u8) -> Result<Self, EncodingError> {
        match value {
            0x01 => Ok(Self::IndexRecord),
            0x02 => Ok(Self::Operation),
            0x03 => Ok(Self::BuildDelta),
            0x04 => Ok(Self::AppliedState),
            0x05 => Ok(Self::SecondaryEntry),
            0x06 => Ok(Self::TextManifestRoot),
            0x07 => Ok(Self::TextManifestPage),
            0x09 => Ok(Self::TextBuildArtifact),
            0x0C => Ok(Self::TextEntityState),
            0x0F => Ok(Self::VectorPartitionMapping),
            0x10 => Ok(Self::TextCorpusStatistics),
            0x11 => Ok(Self::TextTermStatistics),
            0x12 => Ok(Self::TextStatisticsEntity),
            unknown => Err(EncodingError::InvalidKey(format!(
                "unknown V2 index record kind {unknown:#04x}"
            ))),
        }
    }
}

/// Every legal scoped V2 key shape.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum ScopedKey {
    IndexRecord(IndexRecordKey),
    Operation(IndexOperationKey),
    BuildDelta(IndexEntityStateKey),
    AppliedState(IndexEntityStateKey),
    SecondaryEntry(SecondaryEntryKey),
    TextManifestRoot(TextManifestRootKey),
    TextManifestPage(TextManifestPageKey),
    TextBuildArtifact(TextBuildArtifactKey),
    TextEntityState(TextEntityStateKey),
    VectorPartitionMapping(VectorPartitionMappingKey),
    TextCorpusStatistics(TextCorpusStatisticsKey),
    TextTermStatistics(TextTermStatisticsKey),
    TextStatisticsEntity(TextStatisticsEntityKey),
}

impl ScopedKey {
    pub(crate) const fn record_kind(&self) -> RecordKind {
        match self {
            Self::IndexRecord(_) => RecordKind::IndexRecord,
            Self::Operation(_) => RecordKind::Operation,
            Self::BuildDelta(_) => RecordKind::BuildDelta,
            Self::AppliedState(_) => RecordKind::AppliedState,
            Self::SecondaryEntry(_) => RecordKind::SecondaryEntry,
            Self::TextManifestRoot(_) => RecordKind::TextManifestRoot,
            Self::TextManifestPage(_) => RecordKind::TextManifestPage,
            Self::TextBuildArtifact(_) => RecordKind::TextBuildArtifact,
            Self::TextEntityState(_) => RecordKind::TextEntityState,
            Self::VectorPartitionMapping(_) => RecordKind::VectorPartitionMapping,
            Self::TextCorpusStatistics(_) => RecordKind::TextCorpusStatistics,
            Self::TextTermStatistics(_) => RecordKind::TextTermStatistics,
            Self::TextStatisticsEntity(_) => RecordKind::TextStatisticsEntity,
        }
    }

    pub(crate) const fn key_prefix() -> u8 {
        DATA_PREFIX
    }

    pub(crate) fn index_record(identity: IndexIdentity) -> Self {
        Self::IndexRecord(IndexRecordKey { identity })
    }

    pub(crate) fn operation(operation_id: IndexOperationId) -> Self {
        Self::Operation(IndexOperationKey { operation_id })
    }

    pub(crate) fn logical_prefix(kind: RecordKind) -> Bytes {
        Bytes::from(vec![Self::key_prefix(), kind.as_u8()])
    }

    /// Returns one exact index-generation prefix for a physical work kind.
    pub(crate) fn generation_prefix(
        kind: RecordKind,
        index_id: IndexId,
        generation: IndexGenerationId,
    ) -> Bytes {
        let mut bytes = Vec::with_capacity(PREFIX_LEN + KIND_LEN + U64_LEN + U64_LEN);
        bytes.put_u8(Self::key_prefix());
        bytes.put_u8(kind.as_u8());
        bytes.put_u64(index_id.get());
        bytes.put_u64(generation.get());
        Bytes::from(bytes)
    }

    /// Returns one exact lane prefix inside a secondary generation.
    pub(crate) fn secondary_lane_prefix(
        index_id: IndexId,
        generation: IndexGenerationId,
        lane: SecondaryEntryLane,
    ) -> Bytes {
        let mut bytes =
            Self::generation_prefix(RecordKind::SecondaryEntry, index_id, generation).to_vec();
        bytes.put_u8(lane.as_u8());
        Bytes::from(bytes)
    }

    /// Returns the complete scoped prefix for one exact canonical equality
    /// value inside a non-unique secondary lane.
    pub(crate) fn secondary_equality_scan_prefix(
        scope: DataScope,
        index_id: IndexId,
        generation: IndexGenerationId,
        lane: SecondaryEntryLane,
        value: &CanonicalSecondaryValue,
    ) -> Result<Bytes, EncodingError> {
        if !lane.is_equality() || lane.is_unique() {
            return Err(EncodingError::InvalidKey(
                "equality scan prefixes require a non-unique equality lane".to_string(),
            ));
        }
        let mut prefix = Key::data_prefix(
            scope,
            Self::secondary_lane_prefix(index_id, generation, lane),
        )
        .to_vec();
        prefix.put_slice(&value.equality_scan_prefix()?);
        Ok(Bytes::from(prefix))
    }

    pub(crate) fn encoded_len(&self) -> usize {
        let suffix = match self {
            Self::IndexRecord(key) => identity_encoded_len(&key.identity),
            Self::Operation(_) => UUID_LEN,
            Self::BuildDelta(_) | Self::AppliedState(_) => U64_LEN + U64_LEN + KIND_LEN + U64_LEN,
            Self::SecondaryEntry(key) => {
                U64_LEN
                    + U64_LEN
                    + KIND_LEN
                    + key.value.encoded_key_len()
                    + key.entity_id.map_or(0, |_| U64_LEN)
            }
            Self::TextManifestRoot(_) => U64_LEN + U64_LEN + HASH_LEN,
            Self::TextManifestPage(_) => U64_LEN + U64_LEN + HASH_LEN + U32_LEN,
            Self::TextBuildArtifact(_) => U64_LEN + U64_LEN + HASH_LEN + U32_LEN,
            Self::TextEntityState(_) => U64_LEN + U64_LEN + HASH_LEN + KIND_LEN + U64_LEN,
            Self::VectorPartitionMapping(_) => U64_LEN + U64_LEN + HASH_LEN,
            Self::TextCorpusStatistics(_) => U64_LEN + U64_LEN + HASH_LEN,
            Self::TextTermStatistics(_) => U64_LEN + U64_LEN + HASH_LEN + HASH_LEN,
            Self::TextStatisticsEntity(_) => U64_LEN + U64_LEN + KIND_LEN + U64_LEN,
        };
        PREFIX_LEN + KIND_LEN + suffix
    }

    pub(crate) fn encode_into<B: BufMut>(&self, buffer: &mut B) {
        buffer.put_u8(Self::key_prefix());
        buffer.put_u8(self.record_kind().as_u8());
        match self {
            Self::IndexRecord(key) => encode_identity(&key.identity, buffer),
            Self::Operation(key) => buffer.put_slice(key.operation_id.as_bytes()),
            Self::BuildDelta(key) | Self::AppliedState(key) => encode_entity_state_key(key, buffer),
            Self::SecondaryEntry(key) => {
                buffer.put_u64(key.index_id.get());
                buffer.put_u64(key.generation.get());
                buffer.put_u8(key.lane.as_u8());
                key.value.encode_key_value(buffer);
                key.entity_id
                    .iter()
                    .for_each(|entity_id| buffer.put_u64(entity_id.get()));
            }
            Self::TextManifestRoot(key) => encode_text_root(key, buffer),
            Self::TextManifestPage(key) => {
                encode_text_root(&key.root, buffer);
                buffer.put_u32(key.page);
            }
            Self::TextBuildArtifact(key) => {
                encode_text_root(&key.root, buffer);
                buffer.put_u32(key.ordinal);
            }
            Self::TextEntityState(key) => {
                encode_text_root(&key.root, buffer);
                buffer.put_u8(key.entity.kind as u8);
                buffer.put_u64(key.entity.id.get());
            }
            Self::VectorPartitionMapping(key) => {
                buffer.put_u64(key.index_id.get());
                buffer.put_u64(key.generation.get());
                buffer.put_slice(key.partition.as_bytes());
            }
            Self::TextCorpusStatistics(key) => encode_text_corpus_statistics(key, buffer),
            Self::TextTermStatistics(key) => {
                encode_text_corpus_statistics(&key.corpus, buffer);
                buffer.put_slice(key.term.as_bytes());
            }
            Self::TextStatisticsEntity(key) => {
                buffer.put_u64(key.index_id.get());
                buffer.put_u64(key.generation.get());
                buffer.put_u8(key.entity.kind as u8);
                buffer.put_u64(key.entity.id.get());
            }
        }
    }

    pub(crate) fn parse_from_slice(slice: &[u8]) -> Result<Self, EncodingError> {
        const PREFIX_OFFSET: usize = 0;
        const KIND_OFFSET: usize = PREFIX_OFFSET + PREFIX_LEN;
        const SUFFIX_OFFSET: usize = KIND_OFFSET + KIND_LEN;
        if slice.len() < PREFIX_LEN + KIND_LEN {
            return Err(EncodingError::BufferTooShort {
                expected: PREFIX_LEN + KIND_LEN,
                actual: slice.len(),
            });
        }
        if slice[PREFIX_OFFSET] != DATA_PREFIX {
            return Err(EncodingError::InvalidKeyPrefix(slice[PREFIX_OFFSET]));
        }
        let kind = RecordKind::try_from_u8(slice[KIND_OFFSET])?;
        let mut decoder =
            KeyDecoder::new(&slice[SUFFIX_OFFSET..SUFFIX_OFFSET + slice.len() - SUFFIX_OFFSET]);
        let key = match kind {
            RecordKind::IndexRecord => Self::IndexRecord(IndexRecordKey {
                identity: decode_identity(&mut decoder)?,
            }),
            RecordKind::Operation => Self::Operation(IndexOperationKey {
                operation_id: decode_operation_id(&mut decoder)?,
            }),
            RecordKind::BuildDelta | RecordKind::AppliedState => {
                let state = decode_entity_state_key(&mut decoder)?;
                if kind == RecordKind::BuildDelta {
                    Self::BuildDelta(state)
                } else {
                    Self::AppliedState(state)
                }
            }
            RecordKind::SecondaryEntry => {
                Self::SecondaryEntry(decode_secondary_entry(&mut decoder)?)
            }
            RecordKind::TextManifestRoot => Self::TextManifestRoot(decode_text_root(&mut decoder)?),
            RecordKind::TextManifestPage => {
                let root = decode_text_root(&mut decoder)?;
                let page = decoder.take_u32()?;
                Self::TextManifestPage(TextManifestPageKey { root, page })
            }
            RecordKind::TextBuildArtifact => {
                let root = decode_text_root(&mut decoder)?;
                let ordinal = decoder.take_u32()?;
                Self::TextBuildArtifact(TextBuildArtifactKey { root, ordinal })
            }
            RecordKind::TextEntityState => {
                let root = decode_text_root(&mut decoder)?;
                let entity = decode_entity(&mut decoder)?;
                Self::TextEntityState(TextEntityStateKey { root, entity })
            }
            RecordKind::VectorPartitionMapping => {
                Self::VectorPartitionMapping(VectorPartitionMappingKey {
                    index_id: decode_index_id(&mut decoder)?,
                    generation: decode_generation(&mut decoder)?,
                    partition: PartitionFingerprint::new(decoder.take_array::<HASH_LEN>()?),
                })
            }
            RecordKind::TextCorpusStatistics => {
                Self::TextCorpusStatistics(decode_text_corpus_statistics(&mut decoder)?)
            }
            RecordKind::TextTermStatistics => Self::TextTermStatistics(TextTermStatisticsKey {
                corpus: decode_text_corpus_statistics(&mut decoder)?,
                term: TextTermFingerprint::new(decoder.take_array::<HASH_LEN>()?),
            }),
            RecordKind::TextStatisticsEntity => {
                Self::TextStatisticsEntity(TextStatisticsEntityKey {
                    index_id: decode_index_id(&mut decoder)?,
                    generation: decode_generation(&mut decoder)?,
                    entity: decode_entity(&mut decoder)?,
                })
            }
        };
        decoder.finish()?;
        Ok(key)
    }
}

fn identity_encoded_len(identity: &IndexIdentity) -> usize {
    KIND_LEN
        + KIND_LEN
        + U32_LEN
        + identity.label().as_str().len()
        + U32_LEN
        + identity.property().as_str().len()
}

fn encode_identity<B: BufMut>(identity: &IndexIdentity, buffer: &mut B) {
    buffer.put_u8(identity.family() as u8);
    buffer.put_u8(identity.element_kind() as u8);
    put_component(identity.label(), buffer);
    put_component(identity.property(), buffer);
}

fn put_component<B: BufMut>(component: &IndexComponent, buffer: &mut B) {
    let len = u32::try_from(component.as_str().len())
        .expect("validated V2 index components are bounded below u32");
    buffer.put_u32(len);
    buffer.put_slice(component.as_str().as_bytes());
}

fn decode_identity(decoder: &mut KeyDecoder<'_>) -> Result<IndexIdentity, EncodingError> {
    let family = match decoder.take_u8()? {
        0x01 => IndexIdentityFamily::SecondaryEquality,
        0x02 => IndexIdentityFamily::SecondaryRange,
        0x03 => IndexIdentityFamily::Vector,
        0x04 => IndexIdentityFamily::Text,
        unknown => {
            return Err(EncodingError::InvalidKey(format!(
                "unknown V2 identity family {unknown:#04x}"
            )));
        }
    };
    let element_kind = decode_element_kind(decoder.take_u8()?)?;
    let label = decoder.take_component("label")?;
    let property = decoder.take_component("property")?;
    Ok(IndexIdentity::new(family, element_kind, label, property))
}

fn encode_entity_state_key<B: BufMut>(key: &IndexEntityStateKey, buffer: &mut B) {
    buffer.put_u64(key.index_id.get());
    buffer.put_u64(key.generation.get());
    buffer.put_u8(key.entity.kind as u8);
    buffer.put_u64(key.entity.id.get());
}

fn decode_entity_state_key(
    decoder: &mut KeyDecoder<'_>,
) -> Result<IndexEntityStateKey, EncodingError> {
    Ok(IndexEntityStateKey {
        index_id: decode_index_id(decoder)?,
        generation: decode_generation(decoder)?,
        entity: decode_entity(decoder)?,
    })
}

fn decode_secondary_entry(
    decoder: &mut KeyDecoder<'_>,
) -> Result<SecondaryEntryKey, EncodingError> {
    let index_id = decode_index_id(decoder)?;
    let generation = decode_generation(decoder)?;
    let lane = SecondaryEntryLane::try_from_u8(decoder.take_u8()?)?;
    let (value, entity_id) = if lane.is_equality() {
        let digest = decoder.take_array::<EQUALITY_DIGEST_LEN>()?;
        let canonical_len = decoder.take_u32()? as usize;
        let canonical = Bytes::copy_from_slice(decoder.take_bytes(canonical_len)?);
        let value = CanonicalEqualityValue::try_from_parts(digest, canonical)?;
        let entity_id = if lane.is_unique() {
            None
        } else {
            Some(IndexEntityId::new(decoder.take_u64()?))
        };
        (CanonicalSecondaryValue::Equality(value), entity_id)
    } else {
        if decoder.remaining_len() < U64_LEN {
            return Err(EncodingError::BufferTooShort {
                expected: U64_LEN,
                actual: decoder.remaining_len(),
            });
        }
        let range_len = decoder.remaining_len() - U64_LEN;
        let range = Bytes::copy_from_slice(decoder.take_bytes(range_len)?);
        let Some(direction) = lane.range_direction() else {
            return Err(EncodingError::InvalidKey(
                "non-equality secondary lane has no range direction".to_string(),
            ));
        };
        (
            CanonicalSecondaryValue::try_encoded_range(direction, range)?,
            Some(IndexEntityId::new(decoder.take_u64()?)),
        )
    };
    SecondaryEntryKey::try_new(index_id, generation, lane, value, entity_id)
}

fn encode_text_root<B: BufMut>(key: &TextManifestRootKey, buffer: &mut B) {
    buffer.put_u64(key.index_id.get());
    buffer.put_u64(key.generation.get());
    buffer.put_slice(key.partition.as_bytes());
}

fn decode_text_root(decoder: &mut KeyDecoder<'_>) -> Result<TextManifestRootKey, EncodingError> {
    Ok(TextManifestRootKey {
        index_id: decode_index_id(decoder)?,
        generation: decode_generation(decoder)?,
        partition: PartitionFingerprint::new(decoder.take_array::<HASH_LEN>()?),
    })
}

fn encode_text_corpus_statistics<B: BufMut>(key: &TextCorpusStatisticsKey, buffer: &mut B) {
    buffer.put_u64(key.index_id.get());
    buffer.put_u64(key.generation.get());
    buffer.put_slice(key.partition.as_bytes());
}

fn decode_text_corpus_statistics(
    decoder: &mut KeyDecoder<'_>,
) -> Result<TextCorpusStatisticsKey, EncodingError> {
    Ok(TextCorpusStatisticsKey {
        index_id: decode_index_id(decoder)?,
        generation: decode_generation(decoder)?,
        partition: PartitionFingerprint::new(decoder.take_array::<HASH_LEN>()?),
    })
}

fn decode_entity(decoder: &mut KeyDecoder<'_>) -> Result<IndexEntity, EncodingError> {
    Ok(IndexEntity {
        kind: decode_element_kind(decoder.take_u8()?)?,
        id: IndexEntityId::new(decoder.take_u64()?),
    })
}

fn decode_element_kind(value: u8) -> Result<IndexElementKind, EncodingError> {
    match value {
        0x01 => Ok(IndexElementKind::Node),
        0x02 => Ok(IndexElementKind::Edge),
        unknown => Err(EncodingError::InvalidKey(format!(
            "unknown V2 element kind {unknown:#04x}"
        ))),
    }
}

fn decode_index_id(decoder: &mut KeyDecoder<'_>) -> Result<IndexId, EncodingError> {
    IndexId::new(decoder.take_u64()?).map_err(model_key_error)
}

fn decode_generation(decoder: &mut KeyDecoder<'_>) -> Result<IndexGenerationId, EncodingError> {
    IndexGenerationId::new(decoder.take_u64()?).map_err(model_key_error)
}

fn decode_operation_id(decoder: &mut KeyDecoder<'_>) -> Result<IndexOperationId, EncodingError> {
    IndexOperationId::from_bytes(decoder.take_array::<UUID_LEN>()?).map_err(model_key_error)
}

fn model_key_error(error: crate::index_lifecycle::IndexV2ModelError) -> EncodingError {
    EncodingError::InvalidKey(error.to_string())
}

/// Small bounded decoder shared by the many fixed V2 key suffixes.
struct KeyDecoder<'a> {
    remaining: &'a [u8],
}

impl<'a> KeyDecoder<'a> {
    const fn new(remaining: &'a [u8]) -> Self {
        Self { remaining }
    }

    const fn remaining_len(&self) -> usize {
        self.remaining.len()
    }

    fn take_bytes(&mut self, len: usize) -> Result<&'a [u8], EncodingError> {
        const FIELD_OFFSET: usize = 0;
        if self.remaining.len() < len {
            return Err(EncodingError::BufferTooShort {
                expected: len,
                actual: self.remaining.len(),
            });
        }
        let value = &self.remaining[FIELD_OFFSET..FIELD_OFFSET + len];
        self.remaining = &self.remaining[FIELD_OFFSET + len..FIELD_OFFSET + self.remaining.len()];
        Ok(value)
    }

    fn take_array<const LEN: usize>(&mut self) -> Result<[u8; LEN], EncodingError> {
        Ok(self
            .take_bytes(LEN)?
            .try_into()
            .expect("fixed decoder slice matches requested array length"))
    }

    fn take_u8(&mut self) -> Result<u8, EncodingError> {
        const BYTE_OFFSET: usize = 0;
        Ok(self.take_bytes(KIND_LEN)?[BYTE_OFFSET])
    }

    fn take_u32(&mut self) -> Result<u32, EncodingError> {
        Ok(u32::from_be_bytes(self.take_array::<U32_LEN>()?))
    }

    fn take_u64(&mut self) -> Result<u64, EncodingError> {
        Ok(u64::from_be_bytes(self.take_array::<U64_LEN>()?))
    }

    fn take_component(&mut self, kind: &'static str) -> Result<IndexComponent, EncodingError> {
        let len = self.take_u32()? as usize;
        let bytes = self.take_bytes(len)?;
        let value = std::str::from_utf8(bytes)?;
        IndexComponent::try_new(kind, value).map_err(model_key_error)
    }

    fn finish(self) -> Result<(), EncodingError> {
        if !self.remaining.is_empty() {
            return Err(EncodingError::InvalidKey(format!(
                "V2 key has {} trailing bytes",
                self.remaining.len()
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod wire_fixtures {
    use std::fmt::Write;

    use super::*;
    use crate::encoding::indexes::range::RangeIndexDirection;
    use crate::encoding::v1::keys::tenant::TenantId;
    use crate::index_lifecycle::VectorPhysicalIndexId;

    fn index_id() -> IndexId {
        IndexId::new(1).unwrap()
    }

    fn generation() -> IndexGenerationId {
        IndexGenerationId::new(2).unwrap()
    }

    fn entity(kind: IndexElementKind) -> IndexEntity {
        IndexEntity {
            kind,
            id: IndexEntityId::new(3),
        }
    }

    fn identity(family: IndexIdentityFamily) -> IndexIdentity {
        IndexIdentity::new(
            family,
            IndexElementKind::Node,
            IndexComponent::try_new("label", "L").unwrap(),
            IndexComponent::try_new("property", "p").unwrap(),
        )
    }

    fn root() -> TextManifestRootKey {
        TextManifestRootKey {
            index_id: index_id(),
            generation: generation(),
            partition: PartitionFingerprint::new([0x22; HASH_LEN]),
        }
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().fold(String::new(), |mut output, byte| {
            write!(output, "{byte:02x}").expect("writing to String cannot fail");
            output
        })
    }

    fn scoped_fixtures() -> Vec<(&'static str, ScopedKey)> {
        let equality = |lane, entity_id| {
            ScopedKey::SecondaryEntry(
                SecondaryEntryKey::try_new(
                    index_id(),
                    generation(),
                    lane,
                    CanonicalSecondaryValue::equality_string("shared"),
                    entity_id,
                )
                .unwrap(),
            )
        };
        let range = |lane, direction| {
            ScopedKey::SecondaryEntry(
                SecondaryEntryKey::try_new(
                    index_id(),
                    generation(),
                    lane,
                    CanonicalSecondaryValue::range_string(direction, "shared"),
                    Some(IndexEntityId::new(3)),
                )
                .unwrap(),
            )
        };
        vec![
            (
                "lifecycle.index_record",
                ScopedKey::index_record(identity(IndexIdentityFamily::SecondaryEquality)),
            ),
            (
                "lifecycle.operation",
                ScopedKey::operation(IndexOperationId::from_bytes([0x11; UUID_LEN]).unwrap()),
            ),
            (
                "lifecycle.build_delta",
                ScopedKey::BuildDelta(IndexEntityStateKey {
                    index_id: index_id(),
                    generation: generation(),
                    entity: entity(IndexElementKind::Node),
                }),
            ),
            (
                "lifecycle.applied_state",
                ScopedKey::AppliedState(IndexEntityStateKey {
                    index_id: index_id(),
                    generation: generation(),
                    entity: entity(IndexElementKind::Edge),
                }),
            ),
            (
                "equality.node_nonunique",
                equality(
                    SecondaryEntryLane::NodeEquality,
                    Some(IndexEntityId::new(3)),
                ),
            ),
            (
                "equality.node_unique",
                equality(SecondaryEntryLane::NodeUniqueEquality, None),
            ),
            (
                "equality.edge_nonunique",
                equality(
                    SecondaryEntryLane::EdgeEquality,
                    Some(IndexEntityId::new(3)),
                ),
            ),
            (
                "range.node_ascending",
                range(
                    SecondaryEntryLane::NodeRangeAscending,
                    RangeIndexDirection::Asc,
                ),
            ),
            (
                "range.node_descending",
                range(
                    SecondaryEntryLane::NodeRangeDescending,
                    RangeIndexDirection::Desc,
                ),
            ),
            (
                "range.edge_ascending",
                range(
                    SecondaryEntryLane::EdgeRangeAscending,
                    RangeIndexDirection::Asc,
                ),
            ),
            (
                "range.edge_descending",
                range(
                    SecondaryEntryLane::EdgeRangeDescending,
                    RangeIndexDirection::Desc,
                ),
            ),
            ("text.manifest_root", ScopedKey::TextManifestRoot(root())),
            (
                "text.manifest_page",
                ScopedKey::TextManifestPage(TextManifestPageKey {
                    root: root(),
                    page: 4,
                }),
            ),
            (
                "text.build_artifact",
                ScopedKey::TextBuildArtifact(TextBuildArtifactKey {
                    root: root(),
                    ordinal: 5,
                }),
            ),
            (
                "text.entity_state",
                ScopedKey::TextEntityState(TextEntityStateKey {
                    root: root(),
                    entity: entity(IndexElementKind::Node),
                }),
            ),
            (
                "text.corpus_statistics",
                ScopedKey::TextCorpusStatistics(TextCorpusStatisticsKey {
                    index_id: index_id(),
                    generation: generation(),
                    partition: PartitionFingerprint::new([0x22; HASH_LEN]),
                }),
            ),
            (
                "text.term_statistics",
                ScopedKey::TextTermStatistics(TextTermStatisticsKey {
                    corpus: TextCorpusStatisticsKey {
                        index_id: index_id(),
                        generation: generation(),
                        partition: PartitionFingerprint::new([0x22; HASH_LEN]),
                    },
                    term: TextTermFingerprint::new([0x33; HASH_LEN]),
                }),
            ),
            (
                "text.statistics_entity",
                ScopedKey::TextStatisticsEntity(TextStatisticsEntityKey {
                    index_id: index_id(),
                    generation: generation(),
                    entity: entity(IndexElementKind::Edge),
                }),
            ),
            (
                "vector.partition_mapping",
                ScopedKey::VectorPartitionMapping(VectorPartitionMappingKey {
                    index_id: index_id(),
                    generation: generation(),
                    partition: PartitionFingerprint::new([0x22; HASH_LEN]),
                }),
            ),
        ]
    }

    #[test]
    fn managed_scoped_key_bytes_are_frozen() {
        let mut rendered = String::new();
        for (name, key) in scoped_fixtures() {
            let mut encoded = Vec::with_capacity(key.encoded_len());
            key.encode_into(&mut encoded);
            assert_eq!(ScopedKey::parse_from_slice(&encoded).unwrap(), key);
            writeln!(rendered, "{name}={}", hex(&encoded)).expect("writing to String cannot fail");
        }
        insta::assert_snapshot!(rendered, @"
lifecycle.index_record=06010101000000014c0000000170
lifecycle.operation=060211111111111111111111111111111111
lifecycle.build_delta=060300000000000000010000000000000002010000000000000003
lifecycle.applied_state=060400000000000000010000000000000002020000000000000003
equality.node_nonunique=06050000000000000001000000000000000201e9cf50951f33fb140000000b04000000067368617265640000000000000003
equality.node_unique=06050000000000000001000000000000000202e9cf50951f33fb140000000b0400000006736861726564
equality.edge_nonunique=06050000000000000001000000000000000205e9cf50951f33fb140000000b04000000067368617265640000000000000003
range.node_ascending=060500000000000000010000000000000002030373686172656400000000000000000003
range.node_descending=06050000000000000001000000000000000204fc8c979e8d9a9bffff0000000000000003
range.edge_ascending=060500000000000000010000000000000002060373686172656400000000000000000003
range.edge_descending=06050000000000000001000000000000000207fc8c979e8d9a9bffff0000000000000003
text.manifest_root=0606000000000000000100000000000000022222222222222222222222222222222222222222222222222222222222222222
text.manifest_page=060700000000000000010000000000000002222222222222222222222222222222222222222222222222222222222222222200000004
text.build_artifact=060900000000000000010000000000000002222222222222222222222222222222222222222222222222222222222222222200000005
text.entity_state=060c000000000000000100000000000000022222222222222222222222222222222222222222222222222222222222222222010000000000000003
text.corpus_statistics=0610000000000000000100000000000000022222222222222222222222222222222222222222222222222222222222222222
text.term_statistics=06110000000000000001000000000000000222222222222222222222222222222222222222222222222222222222222222223333333333333333333333333333333333333333333333333333333333333333
text.statistics_entity=061200000000000000010000000000000002020000000000000003
vector.partition_mapping=060f000000000000000100000000000000022222222222222222222222222222222222222222222222222222222222222222
");
    }

    #[test]
    fn managed_scope_envelopes_are_frozen() {
        let equality = scoped_fixtures()
            .into_iter()
            .find_map(|(name, key)| (name == "equality.node_nonunique").then_some(key))
            .unwrap();
        let tenant = DataScope::Tenant(TenantId::from_u128(
            0x0102_0304_0506_0708_1112_1314_1516_1718,
        ));
        let unscoped = Key::Data {
            scope: DataScope::LegacyUnscoped,
            kind: equality.clone(),
        }
        .to_bytes();
        let scoped = Key::Data {
            scope: tenant,
            kind: equality,
        }
        .to_bytes();
        insta::assert_snapshot!(
            format!("unscoped={}\ntenant={}\n", hex(&unscoped), hex(&scoped)),
            @"
unscoped=06050000000000000001000000000000000201e9cf50951f33fb140000000b04000000067368617265640000000000000003
tenant=0102030405060708111213141516171806050000000000000001000000000000000201e9cf50951f33fb140000000b04000000067368617265640000000000000003
"
        );
    }

    #[test]
    fn managed_global_key_bytes_are_frozen() {
        let target = TextCompactionTarget::try_new(
            DataScope::Tenant(TenantId::from_u128(7)),
            identity(IndexIdentityFamily::Text),
            index_id(),
            generation(),
            PartitionFingerprint::new([0x22; HASH_LEN]),
            4,
        )
        .unwrap();
        let fixtures = [
            ("storage_version", GlobalKey::StorageVersion),
            (
                "logical_index_watermark",
                GlobalKey::LogicalIndexIdWatermark,
            ),
            (
                "vector_physical_watermark",
                GlobalKey::VectorPhysicalIdWatermark,
            ),
            (
                "operation_pointer",
                GlobalKey::OperationPointer(
                    IndexOperationId::from_bytes([0x11; UUID_LEN]).unwrap(),
                ),
            ),
            (
                "legacy_vector_reservation",
                GlobalKey::LegacyVectorPhysicalReservation(VectorPhysicalIndexId::new(9).unwrap()),
            ),
            (
                "text_compaction_pointer",
                GlobalKey::TextCompactionPointer(target),
            ),
        ];
        let mut rendered = String::new();
        for (name, key) in fixtures {
            let encoded = Key::Global { kind: key.clone() }.to_bytes();
            assert_eq!(GlobalKey::parse_from_slice(&encoded).unwrap(), key);
            writeln!(rendered, "{name}={}", hex(&encoded)).expect("writing to String cannot fail");
        }
        insta::assert_snapshot!(rendered, @"
storage_version=fefefefefefefefefefefefefefefefefe01
logical_index_watermark=fefefefefefefefefefefefefefefefefe02
vector_physical_watermark=fefefefefefefefefefefefefefefefefe03
operation_pointer=fefefefefefefefefefefefefefefefefe0411111111111111111111111111111111
legacy_vector_reservation=fefefefefefefefefefefefefefefefefe0a0000000000000009
text_compaction_pointer=fefefefefefefefefefefefefefefefefe0b01000000000000000000000000000000070401000000014c000000017000000000000000010000000000000002222222222222222222222222222222222222222222222222222222222222222200000004
");
    }
}
