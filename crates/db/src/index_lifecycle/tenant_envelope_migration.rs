//! Private compatibility boundary for the ambiguous V3 tenant envelope.

use std::collections::BTreeSet;

use bytes::{BufMut, Bytes};
use slatedb::{Db, IsolationLevel};

use crate::encoding::v1::keys::tenant::{DataScope, TenantId};
use crate::encoding::v1::keys::Key as GraphKey;
use crate::encoding::v2::keys::{
    GlobalKey, Key, ScopedKey, SecondaryEntryLane, GLOBAL_SENTINEL, TENANT_SENTINEL,
};
use crate::encoding::v2::values::{
    decode_applied_state, decode_build_artifact, decode_build_delta, decode_corpus_statistics,
    decode_index_record, decode_manifest_page, decode_manifest_root, decode_operation_record,
    decode_partition_mapping, decode_secondary_entry, decode_statistics_entity,
    decode_term_statistics, decode_text_entity_state, encode_operation_record,
    SecondaryEqualityBitmapValue,
};
use crate::error::{HelixDbError, Result};

use super::{IndexCursor, IndexGenerationId, IndexId, IndexOperationId};

const MIGRATION_BATCH_SIZE: usize = 256;

#[derive(Debug, Default)]
pub(super) struct TenantMigrationExclusions {
    building_generations: BTreeSet<(TenantId, IndexId, IndexGenerationId)>,
    building_operations: BTreeSet<(TenantId, IndexOperationId)>,
}

impl TenantMigrationExclusions {
    pub(super) fn exclude_building_equality(
        &mut self,
        scope: DataScope,
        index_id: IndexId,
        generation: IndexGenerationId,
        operation_id: IndexOperationId,
    ) {
        let DataScope::Tenant(tenant) = scope else {
            return;
        };
        self.building_generations
            .insert((tenant, index_id, generation));
        self.building_operations.insert((tenant, operation_id));
    }

    fn excludes(&self, tenant: TenantId, kind: &ScopedKey) -> bool {
        match kind {
            ScopedKey::SecondaryEntry(entry) => {
                matches!(
                    entry.lane(),
                    SecondaryEntryLane::NodeEquality | SecondaryEntryLane::EdgeEquality
                ) && entry.entity_id().is_some()
            }
            ScopedKey::SecondaryEqualityBitmap(_) => true,
            ScopedKey::BuildDelta(key) | ScopedKey::AppliedState(key) => self
                .building_generations
                .contains(&(tenant, key.index_id, key.generation)),
            ScopedKey::Operation(key) => self
                .building_operations
                .contains(&(tenant, key.operation_id)),
            ScopedKey::IndexRecord(_)
            | ScopedKey::TextManifestRoot(_)
            | ScopedKey::TextManifestPage(_)
            | ScopedKey::TextBuildArtifact(_)
            | ScopedKey::TextEntityState(_)
            | ScopedKey::VectorPartitionMapping(_)
            | ScopedKey::TextCorpusStatistics(_)
            | ScopedKey::TextTermStatistics(_)
            | ScopedKey::TextStatisticsEntity(_) => false,
        }
    }
}

pub(super) async fn reject_unowned_v3_tenant_keys(
    db: &Db,
    tenants: &BTreeSet<TenantId>,
) -> Result<()> {
    let mut rows = db.scan(..).await?;
    while let Some(row) = rows.next().await? {
        if row.key.starts_with(&GLOBAL_SENTINEL)
            || row.key.starts_with(&TENANT_SENTINEL)
            || row.key.len() < DataScope::PREFIX_LEN + core::mem::size_of::<u8>()
            || row.key[DataScope::PREFIX_LEN] != ScopedKey::key_prefix()
        {
            continue;
        }
        let logical = &row.key
            [DataScope::PREFIX_LEN..DataScope::PREFIX_LEN + row.key.len() - DataScope::PREFIX_LEN];
        let Ok(kind) = ScopedKey::parse_from_slice(logical) else {
            continue;
        };
        if validate_legacy_value(&kind, &row.value).is_err() {
            continue;
        }
        let tenant = TenantId::from_u128(u128::from_be_bytes(
            row.key[0..DataScope::PREFIX_LEN]
                .try_into()
                .expect("validated tenant prefix is sixteen bytes"),
        ));
        if tenants.contains(&tenant) {
            continue;
        }
        let unscoped_is_valid = ScopedKey::parse_from_slice(&row.key)
            .is_ok_and(|kind| validate_legacy_value(&kind, &row.value).is_ok());
        let message = if unscoped_is_valid {
            "V3 V2 row has ambiguous unscoped and tenant interpretations"
        } else {
            "V3 tenant V2 row has no catalog root"
        };
        return Err(corruption(message));
    }
    Ok(())
}

pub(super) async fn copy_v3_tenant_keys(
    db: &Db,
    tenants: &BTreeSet<TenantId>,
    exclusions: &TenantMigrationExclusions,
) -> Result<()> {
    for tenant in tenants {
        let scope = DataScope::Tenant(*tenant);
        let prefix = legacy_data_prefix(scope, Bytes::copy_from_slice(&[ScopedKey::key_prefix()]));
        let mut rows = db.scan_prefix(prefix, ..).await?;
        loop {
            let mut batch = Vec::with_capacity(MIGRATION_BATCH_SIZE);
            while batch.len() < MIGRATION_BATCH_SIZE {
                let Some(row) = rows.next().await? else {
                    break;
                };
                let Key::Data { kind, .. } = parse_legacy_data_key(scope, &row.key)? else {
                    return Err(corruption("legacy tenant prefix yielded a global V2 key"));
                };
                if exclusions.excludes(*tenant, &kind) {
                    validate_legacy_value(&kind, &row.value)?;
                    continue;
                }
                let destination_value = migrate_legacy_value(scope, &kind, &row.value)?;
                let destination = Key::Data { scope, kind }.to_bytes();
                batch.push((destination, destination_value));
            }
            if batch.is_empty() {
                break;
            }

            let transaction = db.begin(IsolationLevel::SerializableSnapshot).await?;
            let destination_keys = batch.iter().map(|(key, _)| key.clone()).collect::<Vec<_>>();
            let destination_values = transaction.multi_get(&destination_keys).await?;
            let mut changed = false;
            for ((key, source), destination) in batch.into_iter().zip(destination_values) {
                match destination {
                    Some(destination) if destination == source => {}
                    Some(_) => {
                        return Err(corruption(
                            "V4 tenant key conflicts with its V3 source value",
                        ));
                    }
                    None => {
                        transaction.put(key, source)?;
                        changed = true;
                    }
                }
            }
            if changed {
                super::equality_bitmap_migration::trip_batch_before()?;
                transaction.commit().await?;
                super::equality_bitmap_migration::trip_batch_after()?;
            } else {
                transaction.rollback();
            }
        }
    }
    Ok(())
}

pub(super) async fn verify_v3_tenant_keys(
    db: &Db,
    tenants: &BTreeSet<TenantId>,
    exclusions: &TenantMigrationExclusions,
) -> Result<()> {
    for tenant in tenants {
        let scope = DataScope::Tenant(*tenant);
        let prefix = legacy_data_prefix(scope, Bytes::copy_from_slice(&[ScopedKey::key_prefix()]));
        let mut rows = db.scan_prefix(prefix, ..).await?;
        loop {
            let mut batch = Vec::with_capacity(MIGRATION_BATCH_SIZE);
            while batch.len() < MIGRATION_BATCH_SIZE {
                let Some(row) = rows.next().await? else {
                    break;
                };
                let Key::Data { kind, .. } = parse_legacy_data_key(scope, &row.key)? else {
                    return Err(corruption("legacy tenant prefix yielded a global V2 key"));
                };
                if exclusions.excludes(*tenant, &kind) {
                    validate_legacy_value(&kind, &row.value)?;
                    continue;
                }
                let destination_value = migrate_legacy_value(scope, &kind, &row.value)?;
                batch.push((Key::Data { scope, kind }.to_bytes(), destination_value));
            }
            if batch.is_empty() {
                break;
            }

            let keys = batch.iter().map(|(key, _)| key.clone()).collect::<Vec<_>>();
            let values = db.multi_get(&keys).await?;
            if batch
                .into_iter()
                .zip(values)
                .any(|((_, source), destination)| destination.as_ref() != Some(&source))
            {
                return Err(corruption(
                    "V4 tenant key verification differs from its V3 source",
                ));
            }
        }
    }
    Ok(())
}

pub(super) async fn cleanup_v3_tenant_keys(db: &Db, tenants: &BTreeSet<TenantId>) -> Result<()> {
    for delete_catalogs in [false, true] {
        for tenant in tenants {
            let scope = DataScope::Tenant(*tenant);
            let prefix =
                legacy_data_prefix(scope, Bytes::copy_from_slice(&[ScopedKey::key_prefix()]));
            let mut rows = db.scan_prefix(prefix, ..).await?;
            loop {
                let mut keys = Vec::with_capacity(MIGRATION_BATCH_SIZE);
                while keys.len() < MIGRATION_BATCH_SIZE {
                    let Some(row) = rows.next().await? else {
                        break;
                    };
                    let Key::Data { kind, .. } = parse_legacy_data_key(scope, &row.key)? else {
                        return Err(corruption("legacy tenant prefix yielded a global V2 key"));
                    };
                    if matches!(kind, ScopedKey::IndexRecord(_)) == delete_catalogs {
                        keys.push(row.key);
                    }
                }
                if keys.is_empty() {
                    break;
                }

                let transaction = db.begin(IsolationLevel::SerializableSnapshot).await?;
                for key in keys {
                    transaction.delete(key)?;
                }
                super::equality_bitmap_migration::trip_batch_before()?;
                transaction.commit().await?;
                super::equality_bitmap_migration::trip_batch_after()?;
            }
        }
    }
    Ok(())
}

pub(super) fn legacy_data_prefix(scope: DataScope, logical_prefix: Bytes) -> Bytes {
    match scope {
        DataScope::LegacyUnscoped => logical_prefix,
        DataScope::Tenant(tenant) => {
            let mut bytes = Vec::with_capacity(DataScope::PREFIX_LEN + logical_prefix.len());
            bytes.put_u128(tenant.as_u128());
            bytes.put_slice(&logical_prefix);
            Bytes::from(bytes)
        }
    }
}

pub(super) fn legacy_data_key(scope: DataScope, kind: ScopedKey) -> Bytes {
    let mut logical = Vec::with_capacity(kind.encoded_len());
    kind.encode_into(&mut logical);
    legacy_data_prefix(scope, Bytes::from(logical))
}

pub(super) fn parse_legacy_data_key(scope: DataScope, key: &[u8]) -> Result<Key> {
    let Some(logical) = scope.strip_key(key) else {
        return Err(corruption("legacy V2 key does not match its tenant scope"));
    };
    Ok(Key::Data {
        scope,
        kind: ScopedKey::parse_from_slice(logical)?,
    })
}

fn validate_legacy_value(kind: &ScopedKey, value: &[u8]) -> Result<()> {
    match kind {
        ScopedKey::IndexRecord(_) => {
            let _ = decode_index_record(value)?;
        }
        ScopedKey::Operation(_) => {
            let _ = decode_operation_record(value)?;
        }
        ScopedKey::BuildDelta(_) => {
            let _ = decode_build_delta(value)?;
        }
        ScopedKey::AppliedState(_) => {
            let _ = decode_applied_state(value)?;
        }
        ScopedKey::SecondaryEntry(key) => {
            let _ = decode_secondary_entry(key.lane(), value)?;
        }
        ScopedKey::SecondaryEqualityBitmap(_) => {
            let _ = SecondaryEqualityBitmapValue::decode(value)?;
        }
        ScopedKey::TextManifestRoot(_) => {
            let _ = decode_manifest_root(value)?;
        }
        ScopedKey::TextManifestPage(_) => {
            let _ = decode_manifest_page(value)?;
        }
        ScopedKey::TextBuildArtifact(_) => {
            let _ = decode_build_artifact(value)?;
        }
        ScopedKey::TextEntityState(_) => {
            let _ = decode_text_entity_state(value)?;
        }
        ScopedKey::VectorPartitionMapping(_) => {
            let _ = decode_partition_mapping(value)?;
        }
        ScopedKey::TextCorpusStatistics(_) => {
            let _ = decode_corpus_statistics(value)?;
        }
        ScopedKey::TextTermStatistics(_) => {
            let _ = decode_term_statistics(value)?;
        }
        ScopedKey::TextStatisticsEntity(_) => {
            let _ = decode_statistics_entity(value)?;
        }
    }
    Ok(())
}

fn migrate_legacy_value(scope: DataScope, kind: &ScopedKey, value: &[u8]) -> Result<Bytes> {
    let ScopedKey::Operation(_) = kind else {
        validate_legacy_value(kind, value)?;
        return Ok(Bytes::copy_from_slice(value));
    };
    let operation = decode_operation_record(value)?;
    let mut changed = false;
    let migrated = operation.try_map_cursors(|cursor| {
        let migrated = migrate_legacy_cursor(scope, cursor)?;
        changed |= &migrated != cursor;
        Ok::<IndexCursor, HelixDbError>(migrated)
    })?;
    if changed {
        Ok(encode_operation_record(&migrated))
    } else {
        Ok(Bytes::copy_from_slice(value))
    }
}

fn migrate_legacy_cursor(scope: DataScope, cursor: &IndexCursor) -> Result<IndexCursor> {
    if GlobalKey::parse_from_slice(cursor.as_bytes()).is_ok()
        || Key::parse_from_slice(scope, cursor.as_bytes()).is_ok()
        || GraphKey::parse_from_slice(scope, cursor.as_bytes()).is_ok()
    {
        return Ok(cursor.clone());
    }
    let Key::Data { kind, .. } = parse_legacy_data_key(scope, cursor.as_bytes())? else {
        return Err(corruption(
            "legacy tenant cursor decoded as a global V2 key",
        ));
    };
    IndexCursor::try_new(Key::Data { scope, kind }.to_bytes())
        .map_err(|error| corruption(&error.to_string()))
}

fn corruption(message: &str) -> HelixDbError {
    HelixDbError::IndexCatalogCorruption(message.to_string())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use slatedb::object_store::memory::InMemory;

    use super::*;
    use crate::encoding::indexes::range::RangeIndexDirection;
    use crate::encoding::v1::keys::{DataKeyKind, NodePropertyKey};
    use crate::encoding::v2::keys::{
        CanonicalSecondaryValue, IndexEntity, IndexEntityStateKey, PartitionFingerprint,
        SecondaryEntryKey, SecondaryEntryLane, TextBuildArtifactKey, TextEntityStateKey,
        TextManifestRootKey,
    };
    use crate::index_lifecycle::{
        IndexComponent, IndexElementKind, IndexEntityId, IndexIdentity, IndexIdentityFamily,
        IndexOperationExecutionState, IndexOperationFamily, IndexOperationProgress,
        IndexOperationRecord, IndexOperationRevision, IndexRevision, OperationCounters,
        PrefixScanProgress, SecondaryBuildProgress, SecondaryBuildStage, SecondaryCleanupProgress,
        TextBuildProgress, TextBuildStage, TextCleanupProgress, VectorBuildProgress,
        VectorBuildStage, VectorCleanupProgress,
    };

    fn index_id() -> IndexId {
        IndexId::new(7).unwrap()
    }

    fn generation() -> IndexGenerationId {
        IndexGenerationId::new(9).unwrap()
    }

    fn identity(family: IndexOperationFamily) -> IndexIdentity {
        let family = match family {
            IndexOperationFamily::Secondary => IndexIdentityFamily::SecondaryRange,
            IndexOperationFamily::Vector => IndexIdentityFamily::Vector,
            IndexOperationFamily::Text => IndexIdentityFamily::Text,
        };
        IndexIdentity::new(
            family,
            IndexElementKind::Node,
            IndexComponent::try_new("label", "Document").unwrap(),
            IndexComponent::try_new("property", "value").unwrap(),
        )
    }

    fn operation(ordinal: u8, progress: IndexOperationProgress) -> IndexOperationRecord {
        let family = progress.family();
        let kind = progress.kind();
        IndexOperationRecord::try_new(
            IndexOperationId::from_bytes([ordinal; 16]).unwrap(),
            index_id(),
            identity(family),
            generation(),
            IndexRevision::initial(),
            IndexOperationRevision::initial(),
            kind,
            family,
            progress,
            1,
            IndexOperationExecutionState::Queued {
                not_before_unix_millis: None,
            },
        )
        .unwrap()
    }

    fn migrated_cursor(scope: DataScope, kind: ScopedKey) -> (IndexCursor, Bytes) {
        let legacy = IndexCursor::try_new(legacy_data_key(scope, kind.clone())).unwrap();
        let current = Key::Data { scope, kind }.to_bytes();
        (legacy, current)
    }

    fn prefix(cursor: IndexCursor) -> PrefixScanProgress {
        PrefixScanProgress {
            cursor: Some(cursor),
            counters: OperationCounters::default(),
        }
    }

    fn operation_cases(scope: DataScope) -> Vec<(IndexOperationRecord, Bytes)> {
        let entity = IndexEntity {
            kind: IndexElementKind::Node,
            id: IndexEntityId::new(11),
        };
        let delta = || {
            ScopedKey::BuildDelta(IndexEntityStateKey {
                index_id: index_id(),
                generation: generation(),
                entity,
            })
        };
        let (secondary_build, secondary_build_current) = migrated_cursor(scope, delta());
        let (vector_build, vector_build_current) = migrated_cursor(scope, delta());
        let (vector_cleanup, vector_cleanup_current) = migrated_cursor(scope, delta());

        let range = ScopedKey::SecondaryEntry(
            SecondaryEntryKey::try_new(
                index_id(),
                generation(),
                SecondaryEntryLane::NodeRangeAscending,
                CanonicalSecondaryValue::range_string(RangeIndexDirection::Asc, "shared"),
                Some(entity.id),
            )
            .unwrap(),
        );
        let (secondary_cleanup, secondary_cleanup_current) = migrated_cursor(scope, range);

        let root = TextManifestRootKey {
            index_id: index_id(),
            generation: generation(),
            partition: PartitionFingerprint::new([0x22; 32]),
        };
        let (text_build, text_build_current) = migrated_cursor(
            scope,
            ScopedKey::TextBuildArtifact(TextBuildArtifactKey { root, ordinal: 3 }),
        );
        let (text_cleanup, text_cleanup_current) = migrated_cursor(
            scope,
            ScopedKey::TextEntityState(TextEntityStateKey { root, entity }),
        );

        vec![
            (
                operation(
                    1,
                    IndexOperationProgress::SecondaryBuild(SecondaryBuildProgress::Constructing(
                        SecondaryBuildStage::CatchUp(prefix(secondary_build)),
                    )),
                ),
                secondary_build_current,
            ),
            (
                operation(
                    2,
                    IndexOperationProgress::SecondaryCleanup(
                        SecondaryCleanupProgress::DeleteEntries(prefix(secondary_cleanup)),
                    ),
                ),
                secondary_cleanup_current,
            ),
            (
                operation(
                    3,
                    IndexOperationProgress::VectorBuild(VectorBuildProgress::Constructing(
                        VectorBuildStage::CatchUp(prefix(vector_build)),
                    )),
                ),
                vector_build_current,
            ),
            (
                operation(
                    4,
                    IndexOperationProgress::VectorCleanup(VectorCleanupProgress::DeleteDeltas(
                        prefix(vector_cleanup),
                    )),
                ),
                vector_cleanup_current,
            ),
            (
                operation(
                    5,
                    IndexOperationProgress::TextBuild(TextBuildProgress::Constructing(
                        TextBuildStage::PrepareManifests(prefix(text_build)),
                    )),
                ),
                text_build_current,
            ),
            (
                operation(
                    6,
                    IndexOperationProgress::TextCleanup(TextCleanupProgress::DeleteMetadata(
                        prefix(text_cleanup),
                    )),
                ),
                text_cleanup_current,
            ),
        ]
    }

    #[test]
    fn managed_cursors_are_reframed_for_every_lifecycle_family() {
        let scope = DataScope::Tenant(TenantId::from_u128(0xABCD));
        for (operation, expected_cursor) in operation_cases(scope) {
            assert!(
                !super::super::repository::operation_record_cursors_are_valid(scope, &operation)
            );
            let encoded = encode_operation_record(&operation);
            let migrated = migrate_legacy_value(
                scope,
                &ScopedKey::operation(operation.operation_id()),
                &encoded,
            )
            .unwrap();
            let migrated = decode_operation_record(&migrated).unwrap();

            assert_eq!(
                migrated.operation_revision(),
                operation.operation_revision()
            );
            assert_eq!(migrated.execution_state(), operation.execution_state());
            assert!(super::super::repository::operation_record_cursors_are_valid(scope, &migrated));
            assert!(migrated
                .progress()
                .cursors_are_valid(|cursor| { cursor.as_bytes() == &expected_cursor }));
        }
    }

    #[test]
    fn graph_cursors_remain_byte_identical() {
        let scope = DataScope::Tenant(TenantId::from_u128(0xABCD));
        let graph_cursor = IndexCursor::try_new(
            GraphKey::Data {
                scope,
                kind: DataKeyKind::NodeProperty(NodePropertyKey::new(11)),
            }
            .to_bytes(),
        )
        .unwrap();
        let operation = operation(
            7,
            IndexOperationProgress::SecondaryBuild(SecondaryBuildProgress::Constructing(
                SecondaryBuildStage::Scan(super::super::SourceScanProgress {
                    inclusive_upper_bound: graph_cursor.clone(),
                    cursor: Some(graph_cursor),
                    counters: OperationCounters::default(),
                }),
            )),
        );
        let encoded = encode_operation_record(&operation);

        assert_eq!(
            migrate_legacy_value(
                scope,
                &ScopedKey::operation(operation.operation_id()),
                &encoded,
            )
            .unwrap(),
            encoded
        );
    }

    #[tokio::test]
    async fn copy_and_verification_use_the_reframed_operation_value() {
        let db = Db::builder("v4-tenant-operation-cursor-copy", Arc::new(InMemory::new()))
            .build()
            .await
            .unwrap();
        let scope = DataScope::Tenant(TenantId::from_u128(0xABCD));
        let (operation, expected_cursor) = operation_cases(scope).remove(1);
        let kind = ScopedKey::operation(operation.operation_id());
        db.put(
            legacy_data_key(scope, kind.clone()),
            encode_operation_record(&operation),
        )
        .await
        .unwrap();
        let DataScope::Tenant(tenant) = scope else {
            unreachable!()
        };
        let tenants = BTreeSet::from([tenant]);
        let exclusions = TenantMigrationExclusions::default();

        copy_v3_tenant_keys(&db, &tenants, &exclusions)
            .await
            .unwrap();
        // Replaying the copy after a committed batch is safe and compares the
        // same transformed value during verification.
        copy_v3_tenant_keys(&db, &tenants, &exclusions)
            .await
            .unwrap();
        verify_v3_tenant_keys(&db, &tenants, &exclusions)
            .await
            .unwrap();

        let migrated = db
            .get(Key::Data { scope, kind }.to_bytes())
            .await
            .unwrap()
            .map(|value| decode_operation_record(&value).unwrap())
            .unwrap();
        assert!(migrated
            .progress()
            .cursors_are_valid(|cursor| { cursor.as_bytes() == &expected_cursor }));
        assert!(super::super::repository::operation_record_cursors_are_valid(scope, &migrated));
        db.close().await.unwrap();
    }

    #[test]
    fn malformed_operation_cursor_fails_closed() {
        let scope = DataScope::Tenant(TenantId::from_u128(0xABCD));
        let invalid = IndexCursor::try_new(Bytes::from_static(b"not-an-index-key")).unwrap();
        let operation = operation(
            8,
            IndexOperationProgress::SecondaryCleanup(SecondaryCleanupProgress::DeleteEntries(
                prefix(invalid),
            )),
        );

        assert!(matches!(
            migrate_legacy_value(
                scope,
                &ScopedKey::operation(operation.operation_id()),
                &encode_operation_record(&operation),
            ),
            Err(HelixDbError::IndexCatalogCorruption(_))
        ));
    }
}
