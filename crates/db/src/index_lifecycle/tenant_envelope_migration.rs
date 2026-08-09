//! Private compatibility boundary for the ambiguous V3 tenant envelope.

use std::collections::BTreeSet;

use bytes::{BufMut, Bytes};
use slatedb::{Db, IsolationLevel};

use crate::encoding::v1::keys::tenant::{DataScope, TenantId};
use crate::encoding::v2::keys::{
    Key, ScopedKey, SecondaryEntryLane, GLOBAL_SENTINEL, TENANT_SENTINEL,
};
use crate::encoding::v2::values::{
    decode_applied_state, decode_build_artifact, decode_build_delta, decode_corpus_statistics,
    decode_index_record, decode_manifest_page, decode_manifest_root, decode_operation_record,
    decode_partition_mapping, decode_secondary_entry, decode_statistics_entity,
    decode_term_statistics, decode_text_entity_state, SecondaryEqualityBitmapValue,
};
use crate::error::{HelixDbError, Result};

use super::{IndexGenerationId, IndexId, IndexOperationId};

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
                validate_legacy_value(&kind, &row.value)?;
                if exclusions.excludes(*tenant, &kind) {
                    continue;
                }
                let destination = Key::Data { scope, kind }.to_bytes();
                batch.push((destination, row.value));
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
                validate_legacy_value(&kind, &row.value)?;
                if exclusions.excludes(*tenant, &kind) {
                    continue;
                }
                batch.push((Key::Data { scope, kind }.to_bytes(), row.value));
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

fn corruption(message: &str) -> HelixDbError {
    HelixDbError::IndexCatalogCorruption(message.to_string())
}
