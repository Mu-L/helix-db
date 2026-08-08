//! Codecs for scoped physical-work and text values.

use bytes::Bytes;

use crate::encoding::error::EncodingError;
use crate::index_lifecycle::work::*;
use crate::index_lifecycle::IndexEntityId;

use super::*;
use super::{INDEX_V2_VALUE_VERSION, INDEX_V3_SPLIT_VALUE_VERSION};

pub(crate) mod equality;
pub(crate) mod range;
pub(crate) mod text;
pub(crate) mod vector;

pub(crate) use equality::SecondaryEqualityBitmapValue;

pub(crate) fn encode_secondary_entry(value: &SecondaryEntryValue) -> Bytes {
    if value.lane.is_equality() {
        equality::encode_entry(value).expect("equality lane selects its typed value codec")
    } else {
        range::encode_entry(value).expect("range lane selects its typed value codec")
    }
}

pub(crate) fn decode_secondary_entry(value: &[u8]) -> Result<SecondaryEntryValue, EncodingError> {
    let WorkValue::SecondaryEntry(decoded) = decode_value(value)? else {
        return Err(EncodingError::Custom(
            "secondary entry key contains another value kind".to_string(),
        ));
    };
    if decoded.lane.is_equality() {
        equality::validate_entry(decoded)
    } else {
        range::validate_entry(decoded)
    }
}

/// Closed dispatch value for record kinds `0x03..=0x0F`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) enum WorkValue {
    CoalescedBuildDelta(CoalescedBuildDeltaValue),
    AppliedEntityState(AppliedEntityStateValue),
    SecondaryEntry(SecondaryEntryValue),
    TextManifestRoot(TextManifestRootValue),
    TextManifestPage(TextManifestPageValue),
    TextBuildArtifact(TextBuildArtifactValue),
    TextEntityState(TextEntityStateValue),
    VectorPartitionMapping(VectorPartitionMappingValue),
    TextCorpusStatistics(TextCorpusStatisticsValue),
    TextTermStatistics(TextTermStatisticsValue),
    TextStatisticsEntity(TextStatisticsEntityValue),
}

impl WorkValue {
    const fn record_kind(&self) -> u8 {
        match self {
            Self::CoalescedBuildDelta(_) => 0x03,
            Self::AppliedEntityState(_) => 0x04,
            Self::SecondaryEntry(_) => 0x05,
            Self::TextManifestRoot(_) => 0x06,
            Self::TextManifestPage(_) => 0x07,
            Self::TextBuildArtifact(_) => 0x09,
            Self::TextEntityState(_) => 0x0C,
            Self::VectorPartitionMapping(_) => 0x0F,
            Self::TextCorpusStatistics(_) => 0x10,
            Self::TextTermStatistics(_) => 0x11,
            Self::TextStatisticsEntity(_) => 0x12,
        }
    }
}

pub(super) fn encode_value(value: &WorkValue) -> Bytes {
    let version = match value {
        WorkValue::TextManifestPage(_) | WorkValue::TextBuildArtifact(_) => {
            INDEX_V3_SPLIT_VALUE_VERSION
        }
        WorkValue::CoalescedBuildDelta(_)
        | WorkValue::AppliedEntityState(_)
        | WorkValue::SecondaryEntry(_)
        | WorkValue::TextManifestRoot(_)
        | WorkValue::TextEntityState(_)
        | WorkValue::VectorPartitionMapping(_)
        | WorkValue::TextCorpusStatistics(_)
        | WorkValue::TextTermStatistics(_)
        | WorkValue::TextStatisticsEntity(_) => INDEX_V2_VALUE_VERSION,
    };
    let mut encoder = ValueEncoder::with_versioned_header(version, value.record_kind());
    match value {
        WorkValue::CoalescedBuildDelta(value) => {
            put_index_id(&mut encoder, value.index_id);
            put_generation(&mut encoder, value.generation);
            put_element_kind(&mut encoder, value.entity_kind);
            encoder.put_u64(value.entity_id.get());
        }
        WorkValue::AppliedEntityState(value) => {
            put_index_id(&mut encoder, value.index_id);
            put_generation(&mut encoder, value.generation);
            put_element_kind(&mut encoder, value.entity_kind);
            encoder.put_u64(value.entity_id.get());
            match &value.state {
                AppliedFamilyState::Secondary(state) => {
                    encoder.put_u8(0x01);
                    put_option(&mut encoder, state.as_ref(), put_secondary_value);
                }
                AppliedFamilyState::Vector(state) => {
                    encoder.put_u8(0x02);
                    put_option(&mut encoder, state.as_ref(), put_partition);
                }
                AppliedFamilyState::Text(state) => {
                    encoder.put_u8(0x03);
                    put_option(&mut encoder, state.as_ref(), |encoder, state| {
                        put_partition(encoder, &state.0);
                        encoder.put_u64(state.1.get());
                    });
                }
            }
        }
        WorkValue::SecondaryEntry(value) => {
            put_index_id(&mut encoder, value.index_id);
            put_generation(&mut encoder, value.generation);
            put_secondary_lane(&mut encoder, value.lane);
            encoder.put_u64(value.entity_id.get());
        }
        WorkValue::TextManifestRoot(value) => {
            put_index_id(&mut encoder, value.index_id());
            put_generation(&mut encoder, value.generation());
            put_partition(&mut encoder, value.partition());
            encoder.put_u64(value.revision().get());
            encoder.put_u32(value.page_count());
            encoder.put_u64(value.split_count());
        }
        WorkValue::TextManifestPage(value) => {
            put_index_id(&mut encoder, value.index_id());
            put_generation(&mut encoder, value.generation());
            put_partition(&mut encoder, value.partition());
            encoder.put_u32(value.page());
            encoder.put_u32(
                u32::try_from(value.entries().len()).expect("bounded manifest page fits u32"),
            );
            for split in value.entries() {
                put_split_ref(&mut encoder, *split);
            }
        }
        WorkValue::TextBuildArtifact(value) => {
            put_index_id(&mut encoder, value.index_id);
            put_generation(&mut encoder, value.generation);
            put_partition(&mut encoder, &value.partition);
            encoder.put_u32(value.artifact_ordinal);
            put_split_ref(&mut encoder, value.split);
        }
        WorkValue::TextEntityState(value) => {
            put_index_id(&mut encoder, value.index_id);
            put_generation(&mut encoder, value.generation);
            put_partition(&mut encoder, &value.partition);
            put_element_kind(&mut encoder, value.entity_kind);
            encoder.put_u64(value.entity_id.get());
            encoder.put_u64(value.logical_version.get());
            encoder.put_bool(value.live);
        }
        WorkValue::VectorPartitionMapping(value) => {
            put_index_id(&mut encoder, value.index_id);
            put_generation(&mut encoder, value.generation);
            put_partition(&mut encoder, value.partition.as_partition());
            encoder.put_u64(value.physical_index_id.get());
        }
        WorkValue::TextCorpusStatistics(value) => {
            put_index_id(&mut encoder, value.index_id);
            put_generation(&mut encoder, value.generation);
            put_partition(&mut encoder, &value.partition);
            encoder.put_u64(value.document_count);
            encoder.put_u64(value.total_token_count);
        }
        WorkValue::TextTermStatistics(value) => {
            put_index_id(&mut encoder, value.index_id);
            put_generation(&mut encoder, value.generation);
            put_partition(&mut encoder, &value.partition);
            encoder.put_bytes(&value.term);
            encoder.put_u64(value.document_frequency);
        }
        WorkValue::TextStatisticsEntity(value) => {
            put_index_id(&mut encoder, value.index_id);
            put_generation(&mut encoder, value.generation);
            put_element_kind(&mut encoder, value.entity_kind);
            encoder.put_u64(value.entity_id.get());
            match &value.contribution {
                TextStatisticsContribution::Absent => encoder.put_u8(0x01),
                TextStatisticsContribution::Present {
                    partition,
                    fingerprint,
                    token_count,
                    terms,
                } => {
                    encoder.put_u8(0x02);
                    put_partition(&mut encoder, partition);
                    encoder.put_raw(fingerprint);
                    encoder.put_u64(*token_count);
                    encoder.put_u32(
                        u32::try_from(terms.len())
                            .expect("bounded text statistics term count fits u32"),
                    );
                    for term in terms {
                        encoder.put_bytes(term);
                    }
                }
            }
        }
    }
    encoder.finish()
}

pub(super) fn decode_value(value: &[u8]) -> Result<WorkValue, EncodingError> {
    let mut decoder = ValueDecoder::with_supported_versions(
        value,
        &[INDEX_V2_VALUE_VERSION, INDEX_V3_SPLIT_VALUE_VERSION],
    )?;
    let pruning_version = decoder.version() == INDEX_V3_SPLIT_VALUE_VERSION;
    if pruning_version && !matches!(decoder.kind(), 0x07 | 0x09) {
        return Err(EncodingError::Custom(format!(
            "value version {:#04x} is unsupported for work value kind {:#04x}",
            decoder.version(),
            decoder.kind()
        )));
    }
    let decoded = match decoder.kind() {
        0x03 => WorkValue::CoalescedBuildDelta(CoalescedBuildDeltaValue {
            index_id: take_index_id(&mut decoder)?,
            generation: take_generation(&mut decoder)?,
            entity_kind: take_element_kind(&mut decoder)?,
            entity_id: IndexEntityId::new(decoder.take_u64()?),
        }),
        0x04 => WorkValue::AppliedEntityState(take_applied_state(&mut decoder)?),
        0x05 => WorkValue::SecondaryEntry(SecondaryEntryValue {
            index_id: take_index_id(&mut decoder)?,
            generation: take_generation(&mut decoder)?,
            lane: take_secondary_lane(&mut decoder)?,
            entity_id: IndexEntityId::new(decoder.take_u64()?),
        }),
        0x06 => WorkValue::TextManifestRoot(
            TextManifestRootValue::try_new(
                take_index_id(&mut decoder)?,
                take_generation(&mut decoder)?,
                take_partition(&mut decoder)?,
                take_manifest_revision(&mut decoder)?,
                decoder.take_u32()?,
                decoder.take_u64()?,
            )
            .map_err(work_model_error)?,
        ),
        0x07 => WorkValue::TextManifestPage(take_manifest_page(&mut decoder, pruning_version)?),
        0x09 => WorkValue::TextBuildArtifact(TextBuildArtifactValue {
            index_id: take_index_id(&mut decoder)?,
            generation: take_generation(&mut decoder)?,
            partition: take_partition(&mut decoder)?,
            artifact_ordinal: decoder.take_u32()?,
            split: take_split_ref(&mut decoder, pruning_version)?,
        }),
        0x0C => WorkValue::TextEntityState(TextEntityStateValue {
            index_id: take_index_id(&mut decoder)?,
            generation: take_generation(&mut decoder)?,
            partition: take_partition(&mut decoder)?,
            entity_kind: take_element_kind(&mut decoder)?,
            entity_id: IndexEntityId::new(decoder.take_u64()?),
            logical_version: take_logical_version(&mut decoder)?,
            live: decoder.take_bool()?,
        }),
        0x0F => WorkValue::VectorPartitionMapping(VectorPartitionMappingValue {
            index_id: take_index_id(&mut decoder)?,
            generation: take_generation(&mut decoder)?,
            partition: VectorTenantPartition::try_from_partition(take_partition(&mut decoder)?)
                .map_err(work_model_error)?,
            physical_index_id: crate::index_lifecycle::VectorPhysicalIndexId::new(
                decoder.take_u64()?,
            )
            .map_err(model_error)?,
        }),
        0x10 => WorkValue::TextCorpusStatistics(
            TextCorpusStatisticsValue::try_new(
                take_index_id(&mut decoder)?,
                take_generation(&mut decoder)?,
                take_partition(&mut decoder)?,
                decoder.take_u64()?,
                decoder.take_u64()?,
            )
            .map_err(work_model_error)?,
        ),
        0x11 => WorkValue::TextTermStatistics(
            TextTermStatisticsValue::try_new(
                take_index_id(&mut decoder)?,
                take_generation(&mut decoder)?,
                take_partition(&mut decoder)?,
                decoder.take_bytes(MAX_LENGTH_DELIMITED_FIELD)?,
                decoder.take_u64()?,
            )
            .map_err(work_model_error)?,
        ),
        0x12 => {
            let index_id = take_index_id(&mut decoder)?;
            let generation = take_generation(&mut decoder)?;
            let entity_kind = take_element_kind(&mut decoder)?;
            let entity_id = IndexEntityId::new(decoder.take_u64()?);
            let contribution = match decoder.take_u8()? {
                0x01 => TextStatisticsContribution::Absent,
                0x02 => {
                    let partition = take_partition(&mut decoder)?;
                    let fingerprint = decoder.take_array::<HASH_LEN>()?;
                    let token_count = decoder.take_u64()?;
                    let term_count = decoder.take_u32()? as usize;
                    const MAX_TERM_COUNT: usize = MAX_LENGTH_DELIMITED_FIELD / U32_LEN;
                    if term_count > MAX_TERM_COUNT {
                        return Err(EncodingError::Custom(format!(
                            "text statistics term count {term_count} exceeds maximum {MAX_TERM_COUNT}"
                        )));
                    }
                    let mut terms = Vec::with_capacity(term_count);
                    for _ in 0..term_count {
                        terms.push(decoder.take_bytes(MAX_LENGTH_DELIMITED_FIELD)?);
                    }
                    TextStatisticsContribution::try_present(
                        partition,
                        fingerprint,
                        token_count,
                        terms,
                    )
                    .map_err(work_model_error)?
                }
                unknown => {
                    return Err(unknown_discriminant(
                        "text statistics contribution",
                        unknown,
                    ));
                }
            };
            WorkValue::TextStatisticsEntity(TextStatisticsEntityValue {
                index_id,
                generation,
                entity_kind,
                entity_id,
                contribution,
            })
        }
        unknown => return Err(unknown_discriminant("work value kind", unknown)),
    };
    decoder.finish()?;
    Ok(decoded)
}

fn take_applied_state(
    decoder: &mut ValueDecoder<'_>,
) -> Result<AppliedEntityStateValue, EncodingError> {
    let index_id = take_index_id(decoder)?;
    let generation = take_generation(decoder)?;
    let entity_kind = take_element_kind(decoder)?;
    let entity_id = IndexEntityId::new(decoder.take_u64()?);
    let state = match decoder.take_u8()? {
        0x01 => AppliedFamilyState::Secondary(decoder.take_option(take_secondary_value)?),
        0x02 => AppliedFamilyState::Vector(decoder.take_option(take_partition)?),
        0x03 => AppliedFamilyState::Text(decoder.take_option(|decoder| {
            Ok((take_partition(decoder)?, take_logical_version(decoder)?))
        })?),
        unknown => return Err(unknown_discriminant("applied-state family", unknown)),
    };
    Ok(AppliedEntityStateValue {
        index_id,
        generation,
        entity_kind,
        entity_id,
        state,
    })
}

fn take_manifest_page(
    decoder: &mut ValueDecoder<'_>,
    pruning_version: bool,
) -> Result<TextManifestPageValue, EncodingError> {
    let index_id = take_index_id(decoder)?;
    let generation = take_generation(decoder)?;
    let partition = take_partition(decoder)?;
    let page = decoder.take_u32()?;
    let count = decoder.take_u32()? as usize;
    if count > MAX_COLLECTION_ITEMS {
        return Err(EncodingError::Custom(format!(
            "manifest entry count {count} exceeds maximum {MAX_COLLECTION_ITEMS}"
        )));
    }
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        entries.push(take_split_ref(decoder, pruning_version)?);
    }
    TextManifestPageValue::try_new(index_id, generation, partition, page, entries)
        .map_err(work_model_error)
}

#[cfg(test)]
mod tests {
    use std::fmt::Write;

    use super::*;
    use crate::encoding::v2::keys::{CanonicalSecondaryValue, SecondaryEntryLane};

    fn index_id() -> crate::index_lifecycle::IndexId {
        crate::index_lifecycle::IndexId::new(1).unwrap()
    }

    fn generation() -> crate::index_lifecycle::IndexGenerationId {
        crate::index_lifecycle::IndexGenerationId::new(2).unwrap()
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().fold(String::new(), |mut output, byte| {
            write!(output, "{byte:02x}").expect("writing to String cannot fail");
            output
        })
    }

    fn split(pruning: SplitPruning) -> SplitRef {
        SplitRef::try_new(BlobRef::new([7; 32], 100), 80, 20, 0, 100, pruning).unwrap()
    }

    fn page(entries: Vec<SplitRef>) -> WorkValue {
        WorkValue::TextManifestPage(
            TextManifestPageValue::try_new(
                crate::index_lifecycle::IndexId::new(1).unwrap(),
                crate::index_lifecycle::IndexGenerationId::new(2).unwrap(),
                TextPartition::Unpartitioned,
                0,
                entries,
            )
            .unwrap(),
        )
    }

    #[test]
    fn v3_split_values_round_trip_mixed_pruning() {
        let value = page(vec![
            split(SplitPruning::Unavailable),
            split(SplitPruning::from_terms([b"alpha".as_slice()])),
        ]);
        let encoded = encode_value(&value);

        assert_eq!(encoded[0], INDEX_V3_SPLIT_VALUE_VERSION);
        assert_eq!(decode_value(&encoded).unwrap(), value);
    }

    #[test]
    fn v2_split_values_decode_without_pruning() {
        let value = page(vec![split(SplitPruning::TermBloom256([42, 43, 44, 45]))]);
        let mut encoded = encode_value(&value).to_vec();
        encoded[0] = INDEX_V2_VALUE_VERSION;
        encoded.truncate(encoded.len() - U8_LEN - U64_LEN * SPLIT_PRUNING_BLOOM_WORDS);

        let WorkValue::TextManifestPage(decoded) = decode_value(&encoded).unwrap() else {
            panic!("manifest page decodes as its exact kind");
        };
        assert_eq!(decoded.entries()[0].pruning(), SplitPruning::Unavailable);
    }

    #[test]
    fn split_value_versions_and_pruning_tags_are_closed() {
        let mut malformed = encode_value(&page(vec![split(SplitPruning::Unavailable)])).to_vec();
        *malformed.last_mut().unwrap() = 0xff;
        assert!(decode_value(&malformed).is_err());

        let mut future = encode_value(&page(vec![split(SplitPruning::Unavailable)])).to_vec();
        future[0] = INDEX_V3_SPLIT_VALUE_VERSION + 1;
        assert!(decode_value(&future).is_err());
    }

    #[test]
    fn managed_work_value_bytes_are_frozen() {
        let tenant_partition = TextPartition::try_tenant_value(Bytes::from_static(b"tenant"))
            .expect("fixture partition is valid");
        let vector_partition = VectorTenantPartition::try_new(Bytes::from_static(b"tenant"))
            .expect("fixture vector partition is valid");
        let split = split(SplitPruning::TermBloom256([1, 2, 3, 4]));
        let values = vec![
            (
                "lifecycle.build_delta",
                WorkValue::CoalescedBuildDelta(CoalescedBuildDeltaValue {
                    index_id: index_id(),
                    generation: generation(),
                    entity_kind: crate::index_lifecycle::IndexElementKind::Node,
                    entity_id: IndexEntityId::new(3),
                }),
            ),
            (
                "lifecycle.applied_secondary",
                WorkValue::AppliedEntityState(AppliedEntityStateValue {
                    index_id: index_id(),
                    generation: generation(),
                    entity_kind: crate::index_lifecycle::IndexElementKind::Node,
                    entity_id: IndexEntityId::new(3),
                    state: AppliedFamilyState::Secondary(Some(
                        CanonicalSecondaryValue::equality_string("shared"),
                    )),
                }),
            ),
            (
                "lifecycle.applied_vector",
                WorkValue::AppliedEntityState(AppliedEntityStateValue {
                    index_id: index_id(),
                    generation: generation(),
                    entity_kind: crate::index_lifecycle::IndexElementKind::Node,
                    entity_id: IndexEntityId::new(3),
                    state: AppliedFamilyState::Vector(Some(tenant_partition.clone())),
                }),
            ),
            (
                "lifecycle.applied_text",
                WorkValue::AppliedEntityState(AppliedEntityStateValue {
                    index_id: index_id(),
                    generation: generation(),
                    entity_kind: crate::index_lifecycle::IndexElementKind::Node,
                    entity_id: IndexEntityId::new(3),
                    state: AppliedFamilyState::Text(Some((
                        tenant_partition.clone(),
                        crate::index_lifecycle::TextLogicalVersion::new(4).unwrap(),
                    ))),
                }),
            ),
            (
                "equality.secondary_entry",
                WorkValue::SecondaryEntry(SecondaryEntryValue {
                    index_id: index_id(),
                    generation: generation(),
                    lane: SecondaryEntryLane::NodeEquality,
                    entity_id: IndexEntityId::new(3),
                }),
            ),
            (
                "text.manifest_root",
                WorkValue::TextManifestRoot(
                    TextManifestRootValue::try_new(
                        index_id(),
                        generation(),
                        tenant_partition.clone(),
                        crate::index_lifecycle::TextManifestRevision::new(4).unwrap(),
                        1,
                        1,
                    )
                    .unwrap(),
                ),
            ),
            (
                "text.manifest_page",
                WorkValue::TextManifestPage(
                    TextManifestPageValue::try_new(
                        index_id(),
                        generation(),
                        tenant_partition.clone(),
                        4,
                        vec![split],
                    )
                    .unwrap(),
                ),
            ),
            (
                "text.build_artifact",
                WorkValue::TextBuildArtifact(TextBuildArtifactValue {
                    index_id: index_id(),
                    generation: generation(),
                    partition: tenant_partition.clone(),
                    artifact_ordinal: 4,
                    split,
                }),
            ),
            (
                "text.entity_state",
                WorkValue::TextEntityState(TextEntityStateValue {
                    index_id: index_id(),
                    generation: generation(),
                    partition: tenant_partition.clone(),
                    entity_kind: crate::index_lifecycle::IndexElementKind::Node,
                    entity_id: IndexEntityId::new(3),
                    logical_version: crate::index_lifecycle::TextLogicalVersion::new(4).unwrap(),
                    live: true,
                }),
            ),
            (
                "text.corpus_statistics",
                WorkValue::TextCorpusStatistics(
                    TextCorpusStatisticsValue::try_new(
                        index_id(),
                        generation(),
                        tenant_partition.clone(),
                        5,
                        9,
                    )
                    .unwrap(),
                ),
            ),
            (
                "text.term_statistics",
                WorkValue::TextTermStatistics(
                    TextTermStatisticsValue::try_new(
                        index_id(),
                        generation(),
                        tenant_partition.clone(),
                        Bytes::from_static(b"term"),
                        5,
                    )
                    .unwrap(),
                ),
            ),
            (
                "text.statistics_entity_absent",
                WorkValue::TextStatisticsEntity(TextStatisticsEntityValue {
                    index_id: index_id(),
                    generation: generation(),
                    entity_kind: crate::index_lifecycle::IndexElementKind::Edge,
                    entity_id: IndexEntityId::new(3),
                    contribution: TextStatisticsContribution::Absent,
                }),
            ),
            (
                "text.statistics_entity_present",
                WorkValue::TextStatisticsEntity(TextStatisticsEntityValue {
                    index_id: index_id(),
                    generation: generation(),
                    entity_kind: crate::index_lifecycle::IndexElementKind::Node,
                    entity_id: IndexEntityId::new(3),
                    contribution: TextStatisticsContribution::try_present(
                        tenant_partition,
                        [0x44; 32],
                        9,
                        vec![Bytes::from_static(b"alpha"), Bytes::from_static(b"beta")],
                    )
                    .unwrap(),
                }),
            ),
            (
                "vector.partition_mapping",
                WorkValue::VectorPartitionMapping(VectorPartitionMappingValue {
                    index_id: index_id(),
                    generation: generation(),
                    partition: vector_partition,
                    physical_index_id: crate::index_lifecycle::VectorPhysicalIndexId::new(4)
                        .unwrap(),
                }),
            ),
        ];

        let mut rendered = String::new();
        for (name, value) in values {
            let encoded = encode_value(&value);
            assert_eq!(decode_value(&encoded).unwrap(), value);
            writeln!(rendered, "{name}={}", hex(&encoded)).expect("writing to String cannot fail");
        }
        insta::assert_snapshot!(rendered, @"
lifecycle.build_delta=010300000000000000010000000000000002010000000000000003
lifecycle.applied_secondary=010400000000000000010000000000000002010000000000000003010101e9cf50951f33fb140000000b0400000006736861726564
lifecycle.applied_vector=0104000000000000000100000000000000020100000000000000030201020000000674656e616e74
lifecycle.applied_text=0104000000000000000100000000000000020100000000000000030301020000000674656e616e740000000000000004
equality.secondary_entry=010500000000000000010000000000000002010000000000000003
text.manifest_root=010600000000000000010000000000000002020000000674656e616e740000000000000004000000010000000000000001
text.manifest_page=020700000000000000010000000000000002020000000674656e616e74000000040000000107070707070707070707070707070707070707070707070707070707070707070000000000000064000000000000005000000014000000000000000000000064010000000000000001000000000000000200000000000000030000000000000004
text.build_artifact=020900000000000000010000000000000002020000000674656e616e740000000407070707070707070707070707070707070707070707070707070707070707070000000000000064000000000000005000000014000000000000000000000064010000000000000001000000000000000200000000000000030000000000000004
text.entity_state=010c00000000000000010000000000000002020000000674656e616e74010000000000000003000000000000000401
text.corpus_statistics=011000000000000000010000000000000002020000000674656e616e7400000000000000050000000000000009
text.term_statistics=011100000000000000010000000000000002020000000674656e616e74000000047465726d0000000000000005
text.statistics_entity_absent=01120000000000000001000000000000000202000000000000000301
text.statistics_entity_present=01120000000000000001000000000000000201000000000000000302020000000674656e616e74444444444444444444444444444444444444444444444444444444444444444400000000000000090000000200000005616c7068610000000462657461
vector.partition_mapping=010f00000000000000010000000000000002020000000674656e616e740000000000000004
");
    }
}
