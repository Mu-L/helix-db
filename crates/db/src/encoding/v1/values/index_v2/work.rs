//! Codecs for scoped physical-work and text values.

use bytes::Bytes;

use crate::encoding::error::EncodingError;
use crate::index_v2::work::*;
use crate::index_v2::IndexEntityId;

use super::codec::*;
use super::{INDEX_V2_VALUE_VERSION, INDEX_V3_SPLIT_VALUE_VERSION};

/// Closed dispatch value for record kinds `0x03..=0x0F`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum IndexV2WorkValue {
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

impl IndexV2WorkValue {
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

pub(crate) fn encode_work_value(value: &IndexV2WorkValue) -> Bytes {
    let version = match value {
        IndexV2WorkValue::TextManifestPage(_) | IndexV2WorkValue::TextBuildArtifact(_) => {
            INDEX_V3_SPLIT_VALUE_VERSION
        }
        IndexV2WorkValue::CoalescedBuildDelta(_)
        | IndexV2WorkValue::AppliedEntityState(_)
        | IndexV2WorkValue::SecondaryEntry(_)
        | IndexV2WorkValue::TextManifestRoot(_)
        | IndexV2WorkValue::TextEntityState(_)
        | IndexV2WorkValue::VectorPartitionMapping(_)
        | IndexV2WorkValue::TextCorpusStatistics(_)
        | IndexV2WorkValue::TextTermStatistics(_)
        | IndexV2WorkValue::TextStatisticsEntity(_) => INDEX_V2_VALUE_VERSION,
    };
    let mut encoder = ValueEncoder::with_versioned_header(version, value.record_kind());
    match value {
        IndexV2WorkValue::CoalescedBuildDelta(value) => {
            put_index_id(&mut encoder, value.index_id);
            put_generation(&mut encoder, value.generation);
            put_element_kind(&mut encoder, value.entity_kind);
            encoder.put_u64(value.entity_id.get());
        }
        IndexV2WorkValue::AppliedEntityState(value) => {
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
        IndexV2WorkValue::SecondaryEntry(value) => {
            put_index_id(&mut encoder, value.index_id);
            put_generation(&mut encoder, value.generation);
            put_secondary_lane(&mut encoder, value.lane);
            encoder.put_u64(value.entity_id.get());
        }
        IndexV2WorkValue::TextManifestRoot(value) => {
            put_index_id(&mut encoder, value.index_id());
            put_generation(&mut encoder, value.generation());
            put_partition(&mut encoder, value.partition());
            encoder.put_u64(value.revision().get());
            encoder.put_u32(value.page_count());
            encoder.put_u64(value.split_count());
        }
        IndexV2WorkValue::TextManifestPage(value) => {
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
        IndexV2WorkValue::TextBuildArtifact(value) => {
            put_index_id(&mut encoder, value.index_id);
            put_generation(&mut encoder, value.generation);
            put_partition(&mut encoder, &value.partition);
            encoder.put_u32(value.artifact_ordinal);
            put_split_ref(&mut encoder, value.split);
        }
        IndexV2WorkValue::TextEntityState(value) => {
            put_index_id(&mut encoder, value.index_id);
            put_generation(&mut encoder, value.generation);
            put_partition(&mut encoder, &value.partition);
            put_element_kind(&mut encoder, value.entity_kind);
            encoder.put_u64(value.entity_id.get());
            encoder.put_u64(value.logical_version.get());
            encoder.put_bool(value.live);
        }
        IndexV2WorkValue::VectorPartitionMapping(value) => {
            put_index_id(&mut encoder, value.index_id);
            put_generation(&mut encoder, value.generation);
            put_partition(&mut encoder, value.partition.as_partition());
            encoder.put_u64(value.physical_index_id.get());
        }
        IndexV2WorkValue::TextCorpusStatistics(value) => {
            put_index_id(&mut encoder, value.index_id);
            put_generation(&mut encoder, value.generation);
            put_partition(&mut encoder, &value.partition);
            encoder.put_u64(value.document_count);
            encoder.put_u64(value.total_token_count);
        }
        IndexV2WorkValue::TextTermStatistics(value) => {
            put_index_id(&mut encoder, value.index_id);
            put_generation(&mut encoder, value.generation);
            put_partition(&mut encoder, &value.partition);
            encoder.put_bytes(&value.term);
            encoder.put_u64(value.document_frequency);
        }
        IndexV2WorkValue::TextStatisticsEntity(value) => {
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

pub(crate) fn decode_work_value(value: &[u8]) -> Result<IndexV2WorkValue, EncodingError> {
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
        0x03 => IndexV2WorkValue::CoalescedBuildDelta(CoalescedBuildDeltaValue {
            index_id: take_index_id(&mut decoder)?,
            generation: take_generation(&mut decoder)?,
            entity_kind: take_element_kind(&mut decoder)?,
            entity_id: IndexEntityId::new(decoder.take_u64()?),
        }),
        0x04 => IndexV2WorkValue::AppliedEntityState(take_applied_state(&mut decoder)?),
        0x05 => IndexV2WorkValue::SecondaryEntry(SecondaryEntryValue {
            index_id: take_index_id(&mut decoder)?,
            generation: take_generation(&mut decoder)?,
            lane: take_secondary_lane(&mut decoder)?,
            entity_id: IndexEntityId::new(decoder.take_u64()?),
        }),
        0x06 => IndexV2WorkValue::TextManifestRoot(
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
        0x07 => {
            IndexV2WorkValue::TextManifestPage(take_manifest_page(&mut decoder, pruning_version)?)
        }
        0x09 => IndexV2WorkValue::TextBuildArtifact(TextBuildArtifactValue {
            index_id: take_index_id(&mut decoder)?,
            generation: take_generation(&mut decoder)?,
            partition: take_partition(&mut decoder)?,
            artifact_ordinal: decoder.take_u32()?,
            split: take_split_ref(&mut decoder, pruning_version)?,
        }),
        0x0C => IndexV2WorkValue::TextEntityState(TextEntityStateValue {
            index_id: take_index_id(&mut decoder)?,
            generation: take_generation(&mut decoder)?,
            partition: take_partition(&mut decoder)?,
            entity_kind: take_element_kind(&mut decoder)?,
            entity_id: IndexEntityId::new(decoder.take_u64()?),
            logical_version: take_logical_version(&mut decoder)?,
            live: decoder.take_bool()?,
        }),
        0x0F => IndexV2WorkValue::VectorPartitionMapping(VectorPartitionMappingValue {
            index_id: take_index_id(&mut decoder)?,
            generation: take_generation(&mut decoder)?,
            partition: VectorTenantPartition::try_from_partition(take_partition(&mut decoder)?)
                .map_err(work_model_error)?,
            physical_index_id: crate::index_v2::VectorPhysicalIndexId::new(decoder.take_u64()?)
                .map_err(model_error)?,
        }),
        0x10 => IndexV2WorkValue::TextCorpusStatistics(
            TextCorpusStatisticsValue::try_new(
                take_index_id(&mut decoder)?,
                take_generation(&mut decoder)?,
                take_partition(&mut decoder)?,
                decoder.take_u64()?,
                decoder.take_u64()?,
            )
            .map_err(work_model_error)?,
        ),
        0x11 => IndexV2WorkValue::TextTermStatistics(
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
            IndexV2WorkValue::TextStatisticsEntity(TextStatisticsEntityValue {
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
    use super::*;

    fn split(pruning: SplitPruning) -> SplitRef {
        SplitRef::try_new(BlobRef::new([7; 32], 100), 80, 20, 0, 100, pruning).unwrap()
    }

    fn page(entries: Vec<SplitRef>) -> IndexV2WorkValue {
        IndexV2WorkValue::TextManifestPage(
            TextManifestPageValue::try_new(
                crate::index_v2::IndexId::new(1).unwrap(),
                crate::index_v2::IndexGenerationId::new(2).unwrap(),
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
        let encoded = encode_work_value(&value);

        assert_eq!(encoded[0], INDEX_V3_SPLIT_VALUE_VERSION);
        assert_eq!(decode_work_value(&encoded).unwrap(), value);
    }

    #[test]
    fn v2_split_values_decode_without_pruning() {
        let value = page(vec![split(SplitPruning::TermBloom256([42, 43, 44, 45]))]);
        let mut encoded = encode_work_value(&value).to_vec();
        encoded[0] = INDEX_V2_VALUE_VERSION;
        encoded.truncate(encoded.len() - U8_LEN - U64_LEN * SPLIT_PRUNING_BLOOM_WORDS);

        let IndexV2WorkValue::TextManifestPage(decoded) = decode_work_value(&encoded).unwrap()
        else {
            panic!("manifest page decodes as its exact kind");
        };
        assert_eq!(decoded.entries()[0].pruning(), SplitPruning::Unavailable);
    }

    #[test]
    fn split_value_versions_and_pruning_tags_are_closed() {
        let mut malformed =
            encode_work_value(&page(vec![split(SplitPruning::Unavailable)])).to_vec();
        *malformed.last_mut().unwrap() = 0xff;
        assert!(decode_work_value(&malformed).is_err());

        let mut future = encode_work_value(&page(vec![split(SplitPruning::Unavailable)])).to_vec();
        future[0] = INDEX_V3_SPLIT_VALUE_VERSION + 1;
        assert!(decode_work_value(&future).is_err());
    }
}
