//! Durable V2 bootstrap, catalog loading, allocation, and handle validation.
//!
//! This is the sole boundary that turns typed V2 keys and values into SlateDB
//! operations. Writer bootstrap uses serializable-snapshot isolation and a
//! complete logical-keyspace scan so marker initialization cannot race another
//! metadata write.

use bytes::Bytes;
use slatedb::{Db, DbReadOps, DbTransaction, IsolationLevel};

use crate::encoding::v1::keys::index_v2::{
    GlobalIndexV2Key, IndexV2Key, IndexV2RecordKind, VectorPartitionMappingKey,
    GLOBAL_INDEX_V2_SENTINEL,
};
use crate::encoding::v1::keys::tenant::DataScope;
use crate::encoding::v1::keys::vectors::{VectorKey, VectorStorageLane};
use crate::encoding::v1::keys::{DataKeyKind, GlobalKeyKind, Key};
use crate::encoding::v1::values::index_v2::{
    decode_index_record, decode_metadata_value, decode_work_value, encode_metadata_value,
    encode_work_value, IndexV2WorkValue,
};
use crate::error::{HelixDbError, Result, WriterMigrationRequirement};

use super::work::{VectorPartitionMappingValue, VectorTenantPartition};
use super::{
    ActiveIndexHandle, IndexGenerationId, IndexId, IndexIdentity, IndexOperationId,
    IndexOperationRecord, IndexRecordV2, IndexStorageVersion, IndexV2MetadataValue,
    LegacyVectorPhysicalReservation, LoadedV2ScopeCatalog, LogicalIndexIdWatermark,
    VectorPhysicalIdWatermark, VectorPhysicalIndexId, VectorPhysicalLayout,
};

const UUID_ALLOCATION_ATTEMPTS: usize = 16;

fn global_key(key: GlobalIndexV2Key) -> Bytes {
    Key::Global {
        kind: GlobalKeyKind::IndexV2(key),
    }
    .to_bytes()
}

fn metadata_or_migration_required(
    value: &[u8],
    role: &'static str,
) -> Result<IndexV2MetadataValue> {
    decode_metadata_value(value).map_err(|error| HelixDbError::MigrationRequired {
        reason: format!("malformed V2 {role}: {error}"),
    })
}

/// Initializes missing V2 metadata on legacy storage or validates the tuple.
pub(crate) async fn bootstrap_writer(db: &Db) -> Result<()> {
    let transaction = db.begin(IsolationLevel::SerializableSnapshot).await?;
    let marker_key = global_key(GlobalIndexV2Key::StorageVersion);
    let logical_key = global_key(GlobalIndexV2Key::LogicalIndexIdWatermark);
    let vector_key = global_key(GlobalIndexV2Key::VectorPhysicalIdWatermark);
    let marker = transaction.get(&marker_key).await?;
    let logical = transaction.get(&logical_key).await?;
    let vector = transaction.get(&vector_key).await?;

    let Some(marker) = marker else {
        if logical.is_some() || vector.is_some() {
            return Err(HelixDbError::MigrationRequired {
                reason: "V2 storage bootstrap is partial".to_string(),
            });
        }
        let mut rows = transaction.scan(..).await?;
        while let Some(row) = rows.next().await? {
            let is_global_v2 = row.key.starts_with(&GLOBAL_INDEX_V2_SENTINEL);
            let is_unscoped_v2 = row.key.first().copied()
                == Some(crate::encoding::v1::keys::KeyPrefix::IndexV2.as_u8());
            if is_global_v2 || is_unscoped_v2 {
                return Err(HelixDbError::MigrationRequired {
                    reason: "V2 storage rows exist without the complete bootstrap tuple"
                        .to_string(),
                });
            }
        }
        transaction.put(
            marker_key,
            encode_metadata_value(&IndexV2MetadataValue::StorageVersion(
                IndexStorageVersion::CURRENT,
            )),
        )?;
        transaction.put(
            logical_key,
            encode_metadata_value(&IndexV2MetadataValue::LogicalIndexIdWatermark(
                LogicalIndexIdWatermark {
                    next_id: IndexId::initial(),
                },
            )),
        )?;
        transaction.put(
            vector_key,
            encode_metadata_value(&IndexV2MetadataValue::VectorPhysicalIdWatermark(
                VectorPhysicalIdWatermark {
                    next_id: VectorPhysicalIndexId::initial(),
                },
            )),
        )?;
        transaction.commit().await?;
        return Ok(());
    };

    let IndexV2MetadataValue::StorageVersion(version) =
        metadata_or_migration_required(&marker, "storage marker")?
    else {
        return Err(HelixDbError::MigrationRequired {
            reason: "V2 storage marker contains the wrong value kind".to_string(),
        });
    };
    validate_writer_bootstrap_values(&marker, logical.as_deref(), vector.as_deref())?;
    if version.get() == 0x0002 {
        transaction.put(
            marker_key,
            encode_metadata_value(&IndexV2MetadataValue::StorageVersion(
                IndexStorageVersion::CURRENT,
            )),
        )?;
        transaction.commit().await?;
    }
    Ok(())
}

/// Accepts either a current, complete bootstrap tuple or a pristine legacy store.
///
/// This is intentionally validation-only. Valid writer-resumable storage is
/// reported through [`HelixDbError::WriterMigrationRequired`], while partial or
/// malformed metadata remains a fatal migration error.
pub(crate) async fn require_reader_bootstrap_or_legacy(
    reader: &(impl DbReadOps + Send + Sync),
) -> Result<()> {
    let marker_key = global_key(GlobalIndexV2Key::StorageVersion);
    let logical_key = global_key(GlobalIndexV2Key::LogicalIndexIdWatermark);
    let vector_key = global_key(GlobalIndexV2Key::VectorPhysicalIdWatermark);
    let marker = reader.get(marker_key).await?;
    let logical = reader.get(logical_key).await?;
    let vector = reader.get(vector_key).await?;
    if let Some(marker) = marker {
        let bootstrap = validate_bootstrap_values(&marker, logical.as_deref(), vector.as_deref())?;
        let progress =
            crate::migrations::storage_schema_progress(reader, DataScope::LegacyUnscoped).await?;
        return match (bootstrap, progress) {
            (
                ValidatedReaderBootstrap::WriterMigration(requirement),
                crate::migrations::StorageSchemaProgress::NotStarted
                | crate::migrations::StorageSchemaProgress::GraphReady
                | crate::migrations::StorageSchemaProgress::IndexReady
                | crate::migrations::StorageSchemaProgress::Complete,
            ) => Err(HelixDbError::WriterMigrationRequired { requirement }),
            (
                ValidatedReaderBootstrap::Current,
                crate::migrations::StorageSchemaProgress::Complete,
            ) => Ok(()),
            (
                ValidatedReaderBootstrap::Current,
                crate::migrations::StorageSchemaProgress::NotStarted
                | crate::migrations::StorageSchemaProgress::GraphReady
                | crate::migrations::StorageSchemaProgress::IndexReady,
            ) => Err(HelixDbError::WriterMigrationRequired {
                requirement: WriterMigrationRequirement::IncompleteStorageSchema,
            }),
        };
    }
    if logical.is_some() || vector.is_some() {
        return Err(HelixDbError::MigrationRequired {
            reason: "read-only storage has a partial V2 bootstrap tuple".to_string(),
        });
    }
    let mut rows = reader.scan(..).await?;
    while let Some(row) = rows.next().await? {
        let is_global_v2 = row.key.starts_with(&GLOBAL_INDEX_V2_SENTINEL);
        let is_unscoped_v2 =
            row.key.first().copied() == Some(crate::encoding::v1::keys::KeyPrefix::IndexV2.as_u8());
        if is_global_v2 || is_unscoped_v2 {
            return Err(HelixDbError::MigrationRequired {
                reason: "read-only storage has V2 rows without the complete bootstrap tuple"
                    .to_string(),
            });
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ValidatedReaderBootstrap {
    Current,
    WriterMigration(WriterMigrationRequirement),
}

fn validate_bootstrap_values(
    marker: &[u8],
    logical: Option<&[u8]>,
    vector: Option<&[u8]>,
) -> Result<ValidatedReaderBootstrap> {
    let IndexV2MetadataValue::StorageVersion(version) =
        metadata_or_migration_required(marker, "storage marker")?
    else {
        return Err(HelixDbError::MigrationRequired {
            reason: "V2 storage marker contains the wrong value kind".to_string(),
        });
    };
    let minimum_writer_version =
        IndexStorageVersion::new(0x0002).expect("the V2 storage version is non-zero");
    if version < minimum_writer_version {
        return Err(HelixDbError::MigrationRequired {
            reason: format!(
                "index storage version {} predates required version {}; recreate this development database",
                version.get(),
                IndexStorageVersion::CURRENT.get()
            ),
        });
    }
    if version > IndexStorageVersion::CURRENT {
        return Err(HelixDbError::UnsupportedIndexStorageVersion {
            found: version.get(),
            supported: IndexStorageVersion::CURRENT.get(),
        });
    }
    let Some(logical) = logical else {
        return Err(HelixDbError::MigrationRequired {
            reason: "V2 logical index watermark is missing".to_string(),
        });
    };
    let Some(vector) = vector else {
        return Err(HelixDbError::MigrationRequired {
            reason: "V2 vector physical watermark is missing".to_string(),
        });
    };
    if !matches!(
        metadata_or_migration_required(logical, "logical index watermark")?,
        IndexV2MetadataValue::LogicalIndexIdWatermark(_)
    ) || !matches!(
        metadata_or_migration_required(vector, "vector physical watermark")?,
        IndexV2MetadataValue::VectorPhysicalIdWatermark(_)
    ) {
        return Err(HelixDbError::MigrationRequired {
            reason: "V2 allocator record contains the wrong value kind".to_string(),
        });
    }
    if version < IndexStorageVersion::CURRENT {
        return Ok(ValidatedReaderBootstrap::WriterMigration(
            WriterMigrationRequirement::StorageVersion {
                found: version.get(),
                target: IndexStorageVersion::CURRENT.get(),
            },
        ));
    }
    Ok(ValidatedReaderBootstrap::Current)
}

fn validate_writer_bootstrap_values(
    marker: &[u8],
    logical: Option<&[u8]>,
    vector: Option<&[u8]>,
) -> Result<()> {
    let IndexV2MetadataValue::StorageVersion(version) =
        metadata_or_migration_required(marker, "storage marker")?
    else {
        return Err(HelixDbError::MigrationRequired {
            reason: "V2 storage marker contains the wrong value kind".to_string(),
        });
    };
    let minimum_writer_version =
        IndexStorageVersion::new(0x0002).expect("the V2 storage version is non-zero");
    if version < minimum_writer_version {
        return Err(HelixDbError::MigrationRequired {
            reason: format!(
                "index storage version {} predates migratable version {}; recreate this development database",
                version.get(),
                minimum_writer_version.get()
            ),
        });
    }
    if version > IndexStorageVersion::CURRENT {
        return Err(HelixDbError::UnsupportedIndexStorageVersion {
            found: version.get(),
            supported: IndexStorageVersion::CURRENT.get(),
        });
    }
    let Some(logical) = logical else {
        return Err(HelixDbError::MigrationRequired {
            reason: "V2 logical index watermark is missing".to_string(),
        });
    };
    let Some(vector) = vector else {
        return Err(HelixDbError::MigrationRequired {
            reason: "V2 vector physical watermark is missing".to_string(),
        });
    };
    if !matches!(
        metadata_or_migration_required(logical, "logical index watermark")?,
        IndexV2MetadataValue::LogicalIndexIdWatermark(_)
    ) || !matches!(
        metadata_or_migration_required(vector, "vector physical watermark")?,
        IndexV2MetadataValue::VectorPhysicalIdWatermark(_)
    ) {
        return Err(HelixDbError::MigrationRequired {
            reason: "V2 allocator record contains the wrong value kind".to_string(),
        });
    }
    Ok(())
}

/// Loads and key/value-cross-validates every canonical record for one scope.
pub(crate) async fn load_scope_catalog(
    reader: &(impl DbReadOps + Sync),
    scope: DataScope,
) -> Result<LoadedV2ScopeCatalog> {
    let logical_prefix = IndexV2Key::logical_prefix(IndexV2RecordKind::IndexRecord);
    let physical_prefix = Key::data_prefix(scope, logical_prefix);
    let mut rows = reader.scan_prefix(&physical_prefix, ..).await?;
    let mut loaded = LoadedV2ScopeCatalog::new(scope);
    while let Some(row) = rows.next().await? {
        let parsed = Key::parse_from_slice(scope, &row.key)?;
        let Key::Data {
            kind: DataKeyKind::IndexV2(IndexV2Key::IndexRecord(key)),
            ..
        } = parsed
        else {
            return Err(HelixDbError::IndexCatalogCorruption(
                "index-record prefix yielded a different typed key".to_string(),
            ));
        };
        let record = decode_index_record(&row.value)?;
        if key.identity != *record.identity() {
            return Err(HelixDbError::IndexCatalogCorruption(
                "canonical index key identity differs from its value".to_string(),
            ));
        }
        loaded.insert_active(&record)?;
    }
    Ok(loaded)
}

/// Point-loads one canonical identity through the caller's stable view.
///
/// Keeping absence distinct from a present non-Active record lets secondary
/// serving retain configured legacy indexes while failing closed for a V2
/// identity that is building, aborting, dropping, or dropped.
pub(crate) async fn load_index_record(
    reader: &(impl DbReadOps + Sync),
    scope: DataScope,
    identity: &IndexIdentity,
) -> Result<Option<IndexRecordV2>> {
    let key = Key::Data {
        scope,
        kind: DataKeyKind::IndexV2(IndexV2Key::index_record(identity.clone())),
    }
    .to_bytes();
    let Some(value) = reader.get(key).await? else {
        return Ok(None);
    };
    let record = decode_index_record(&value)?;
    if record.identity() != identity {
        return Err(HelixDbError::IndexCatalogCorruption(
            "canonical index point-read returned a different logical identity".to_string(),
        ));
    }
    Ok(Some(record))
}

/// Point-loads one canonical identity and projects only exact active state.
///
/// Request paths use this boundary through their stable SlateDB view so worker
/// activation and DDL retirement cannot be hidden behind a stale process-local
/// catalog snapshot. Non-active and absent records deliberately return `None`.
pub(crate) async fn load_active_handle(
    reader: &(impl DbReadOps + Sync),
    scope: DataScope,
    identity: &IndexIdentity,
) -> Result<Option<ActiveIndexHandle>> {
    Ok(load_index_record(reader, scope, identity)
        .await?
        .as_ref()
        .and_then(|record| ActiveIndexHandle::try_from_record(scope, record)))
}

/// Re-reads the canonical record and rejects a stale physical authorization.
pub(crate) async fn revalidate_active_handle(
    reader: &(impl DbReadOps + Sync),
    handle: &ActiveIndexHandle,
) -> Result<()> {
    revalidate_active_handle_row(reader, handle)
        .await
        .map(|_| ())
}

/// Re-reads one exact Active record and returns its canonical serialized row.
///
/// Bounded mutation preflight uses the returned key/value bytes for exact input
/// accounting. Callers that need only stale-generation validation should use
/// [`revalidate_active_handle`].
pub(crate) async fn revalidate_active_handle_row(
    reader: &(impl DbReadOps + Sync),
    handle: &ActiveIndexHandle,
) -> Result<(Bytes, Bytes)> {
    let logical = IndexV2Key::index_record(handle.identity().clone());
    let key = Key::Data {
        scope: handle.scope(),
        kind: DataKeyKind::IndexV2(logical),
    }
    .to_bytes();
    let Some(value) = reader.get(&key).await? else {
        return Err(stale_generation(handle));
    };
    let record = decode_index_record(&value)?;
    if !handle.matches_record(handle.scope(), &record) {
        return Err(stale_generation(handle));
    }
    Ok((key, value))
}

fn complete_cursor_is_valid(scope: DataScope, cursor: &[u8]) -> bool {
    const SENTINEL_OFFSET: usize = 0;
    let is_global = cursor.len() >= GLOBAL_INDEX_V2_SENTINEL.len()
        && cursor[SENTINEL_OFFSET..SENTINEL_OFFSET + GLOBAL_INDEX_V2_SENTINEL.len()]
            == GLOBAL_INDEX_V2_SENTINEL;
    if is_global {
        GlobalIndexV2Key::parse_from_slice(cursor).is_ok()
    } else {
        Key::parse_from_slice(scope, cursor).is_ok()
    }
}

/// Validates every persisted operation cursor against an exact V1 scoped or
/// global key parser before a lifecycle transaction accepts it.
pub(super) fn operation_cursors_are_valid(
    scope: DataScope,
    progress: &super::IndexOperationProgress,
) -> bool {
    progress.cursors_are_valid(|cursor| complete_cursor_is_valid(scope, cursor.as_bytes()))
}

/// Validates generic resume keys plus the exact owner-bound text artifact key.
pub(super) fn operation_record_cursors_are_valid(
    scope: DataScope,
    operation: &IndexOperationRecord,
) -> bool {
    if let super::IndexOperationProgress::VectorBuild(super::VectorBuildProgress::Constructing(
        super::VectorBuildStage::AdoptLegacy(progress),
    )) = operation.progress()
    {
        let Some(cursor) = progress.cursor.as_ref() else {
            return true;
        };
        let Ok(Key::Data {
            kind: DataKeyKind::Vector(key),
            ..
        }) = Key::parse_from_slice(scope, cursor.as_bytes())
        else {
            return false;
        };
        let expected_lane = match progress.lane {
            super::LegacyVectorValidationLane::Core => VectorStorageLane::Core,
            super::LegacyVectorValidationLane::Hot => VectorStorageLane::Hot,
            super::LegacyVectorValidationLane::Layer0 => VectorStorageLane::Layer0,
        };
        return key.storage_lane() == expected_lane
            && !matches!(
                key,
                VectorKey::IndexPrefix(_)
                    | VectorKey::MemoryPrefix(_)
                    | VectorKey::L0Prefix(_)
                    | VectorKey::EntryCandidatePrefix(_)
                    | VectorKey::ReverseEdgePrefix(_)
            );
    }
    if let super::IndexOperationProgress::VectorBuild(super::VectorBuildProgress::Constructing(
        super::VectorBuildStage::ValidateAdoptedDirectory(progress),
    )) = operation.progress()
    {
        let Some(cursor) = progress.cursor.as_ref() else {
            return progress.verified_markers == 0;
        };
        let Ok(Key::Data {
            kind: DataKeyKind::Vector(VectorKey::SimHashDirectory(_)),
            ..
        }) = Key::parse_from_slice(scope, cursor.as_bytes())
        else {
            return false;
        };
        return progress.verified_markers <= progress.expected_markers;
    }
    let super::IndexOperationProgress::TextBuild(super::TextBuildProgress::Constructing(stage)) =
        operation.progress()
    else {
        return operation_cursors_are_valid(scope, operation.progress());
    };
    if let super::TextBuildStage::ValidateManifests(progress) = stage {
        let (cursor, lane) = match progress {
            super::TextManifestValidationProgress::Pages(progress) => {
                (progress.cursor(), IndexV2RecordKind::TextManifestPage)
            }
            super::TextManifestValidationProgress::Roots(progress) => (
                progress.cursor.as_ref(),
                IndexV2RecordKind::TextManifestRoot,
            ),
            super::TextManifestValidationProgress::EntityStates(progress) => {
                (progress.cursor.as_ref(), IndexV2RecordKind::TextEntityState)
            }
        };
        let Some(cursor) = cursor else {
            return true;
        };
        let Ok(Key::Data {
            kind: DataKeyKind::IndexV2(key),
            ..
        }) = Key::parse_from_slice(scope, cursor.as_bytes())
        else {
            return false;
        };
        let (kind, index_id, generation, partition, page) = match key {
            IndexV2Key::TextManifestPage(key) => (
                IndexV2RecordKind::TextManifestPage,
                key.root.index_id,
                key.root.generation,
                Some(key.root.partition),
                Some(key.page),
            ),
            IndexV2Key::TextManifestRoot(key) => (
                IndexV2RecordKind::TextManifestRoot,
                key.index_id,
                key.generation,
                Some(key.partition),
                None,
            ),
            IndexV2Key::TextEntityState(key) => (
                IndexV2RecordKind::TextEntityState,
                key.root.index_id,
                key.root.generation,
                Some(key.root.partition),
                None,
            ),
            IndexV2Key::IndexRecord(_)
            | IndexV2Key::Operation(_)
            | IndexV2Key::BuildDelta(_)
            | IndexV2Key::AppliedState(_)
            | IndexV2Key::SecondaryEntry(_)
            | IndexV2Key::TextBuildArtifact(_)
            | IndexV2Key::VectorPartitionMapping(_)
            | IndexV2Key::TextCorpusStatistics(_)
            | IndexV2Key::TextTermStatistics(_)
            | IndexV2Key::TextStatisticsEntity(_) => return false,
        };
        let partition_matches = match progress {
            super::TextManifestValidationProgress::Pages(progress) => {
                progress.partition().is_none_or(|expected| {
                    partition
                        .is_some_and(|actual| actual.as_bytes() == expected.partition_fingerprint())
                        && page.is_some_and(|actual| {
                            actual.checked_add(1) == Some(expected.next_page())
                        })
                })
            }
            super::TextManifestValidationProgress::Roots(_)
            | super::TextManifestValidationProgress::EntityStates(_) => true,
        };
        return kind == lane
            && index_id == operation.index_id()
            && generation == operation.generation()
            && partition_matches;
    }
    if !operation_cursors_are_valid(scope, operation.progress()) {
        return false;
    }
    let artifact_cursor = match stage {
        super::TextBuildStage::PrepareManifests(progress) => {
            let Some(cursor) = progress.cursor.as_ref() else {
                return true;
            };
            cursor
        }
        super::TextBuildStage::ScanSource(_)
        | super::TextBuildStage::ScanPartitions(_)
        | super::TextBuildStage::CatchUp(_)
        | super::TextBuildStage::Compact(_)
        | super::TextBuildStage::ValidateManifests(_)
        | super::TextBuildStage::Activate(_) => return true,
    };
    let Ok(Key::Data {
        kind: DataKeyKind::IndexV2(IndexV2Key::TextBuildArtifact(artifact)),
        ..
    }) = Key::parse_from_slice(scope, artifact_cursor.as_bytes())
    else {
        return false;
    };
    if artifact.root.index_id != operation.index_id()
        || artifact.root.generation != operation.generation()
    {
        return false;
    }
    true
}

fn stale_generation(handle: &ActiveIndexHandle) -> HelixDbError {
    HelixDbError::StaleIndexGeneration {
        index_id: handle.index_id().get(),
        generation: handle.generation().get(),
        record_revision: handle.record_revision().get(),
    }
}

/// Reserves the current logical ID and advances its watermark in `transaction`.
pub(crate) async fn allocate_index_id(transaction: &DbTransaction) -> Result<IndexId> {
    let key = global_key(GlobalIndexV2Key::LogicalIndexIdWatermark);
    let Some(value) = transaction.get(&key).await? else {
        return Err(HelixDbError::MigrationRequired {
            reason: "V2 logical index watermark is missing".to_string(),
        });
    };
    let IndexV2MetadataValue::LogicalIndexIdWatermark(watermark) =
        metadata_or_migration_required(&value, "logical index watermark")?
    else {
        return Err(HelixDbError::MigrationRequired {
            reason: "V2 logical index watermark contains the wrong value kind".to_string(),
        });
    };
    if watermark.next_id.get() == u64::MAX {
        return Err(HelixDbError::IdentifierExhausted("logical index ID"));
    }
    let allocated = watermark.next_id;
    let next_id = allocated.checked_next()?;
    transaction.put(
        key,
        encode_metadata_value(&IndexV2MetadataValue::LogicalIndexIdWatermark(
            LogicalIndexIdWatermark { next_id },
        )),
    )?;
    Ok(allocated)
}

/// Reserves the current vector physical ID and advances its watermark.
pub(crate) async fn allocate_vector_physical_id(
    transaction: &DbTransaction,
) -> Result<VectorPhysicalIndexId> {
    let key = global_key(GlobalIndexV2Key::VectorPhysicalIdWatermark);
    let mut candidate = peek_vector_physical_id(transaction).await?;
    loop {
        let next_id = candidate.checked_next()?;
        transaction.put(
            &key,
            encode_metadata_value(&IndexV2MetadataValue::VectorPhysicalIdWatermark(
                VectorPhysicalIdWatermark { next_id },
            )),
        )?;
        if load_legacy_vector_physical_reservation(transaction, candidate)
            .await?
            .is_none()
        {
            return Ok(candidate);
        }
        candidate = next_id;
    }
}

/// Loads one exact imported-namespace reservation and rejects another value kind.
pub(crate) async fn load_legacy_vector_physical_reservation(
    read: &(impl DbReadOps + Sync),
    physical_index_id: VectorPhysicalIndexId,
) -> Result<Option<LegacyVectorPhysicalReservation>> {
    let key = global_key(GlobalIndexV2Key::LegacyVectorPhysicalReservation(
        physical_index_id,
    ));
    let Some(value) = read.get(key).await? else {
        return Ok(None);
    };
    let IndexV2MetadataValue::LegacyVectorPhysicalReservation(reservation) =
        metadata_or_migration_required(&value, "legacy vector physical reservation")?
    else {
        return Err(HelixDbError::MigrationRequired {
            reason: "legacy vector physical reservation contains the wrong value kind".to_string(),
        });
    };
    Ok(Some(reservation))
}

/// Loads the raw next-ID watermark before reservation skipping.
pub(crate) async fn load_vector_physical_watermark(
    read: &(impl DbReadOps + Sync),
) -> Result<VectorPhysicalIdWatermark> {
    let key = global_key(GlobalIndexV2Key::VectorPhysicalIdWatermark);
    let Some(value) = read.get(key).await? else {
        return Err(HelixDbError::MigrationRequired {
            reason: "V2 vector physical watermark is missing".to_string(),
        });
    };
    let IndexV2MetadataValue::VectorPhysicalIdWatermark(watermark) =
        metadata_or_migration_required(&value, "vector physical watermark")?
    else {
        return Err(HelixDbError::MigrationRequired {
            reason: "V2 vector physical watermark contains the wrong value kind".to_string(),
        });
    };
    Ok(watermark)
}

/// Reads the exact physical ID that the caller's transaction can next reserve.
///
/// Vector builders use this non-mutating preview in a disposable HNSW planning
/// transaction. Only after the complete write set passes admission does the
/// lifecycle transaction call [`allocate_vector_physical_id`] and assert that
/// it received this same ID. Serializable conflict tracking prevents a
/// concurrent allocator from invalidating that proof silently.
pub(crate) async fn peek_vector_physical_id(
    reader: &(impl DbReadOps + Sync),
) -> Result<VectorPhysicalIndexId> {
    let watermark = load_vector_physical_watermark(reader).await?;
    let mut candidate = watermark.next_id;
    loop {
        if candidate.get() == u64::MAX {
            return Err(HelixDbError::IdentifierExhausted(
                "vector physical index ID",
            ));
        }
        if load_legacy_vector_physical_reservation(reader, candidate)
            .await?
            .is_none()
        {
            return Ok(candidate);
        }
        candidate = candidate.checked_next()?;
    }
}

/// Resolves one exact tenant partition through a validated partitioned layout.
///
/// Reads never allocate. The key fingerprint and every repeated value field are
/// cross-checked before the physical ID can authorize HNSW access.
pub(crate) async fn load_vector_partition_mapping(
    reader: &(impl DbReadOps + Sync),
    scope: DataScope,
    index_id: IndexId,
    generation: IndexGenerationId,
    layout: VectorPhysicalLayout,
    partition: &VectorTenantPartition,
) -> Result<Option<VectorPhysicalIndexId>> {
    if layout != VectorPhysicalLayout::Partitioned {
        return Err(HelixDbError::IndexCatalogCorruption(
            "vector partition mapping requested for an unpartitioned generation".to_string(),
        ));
    }
    let key = vector_partition_mapping_key(scope, index_id, generation, partition);
    let Some(value) = reader.get(key).await? else {
        return Ok(None);
    };
    let IndexV2WorkValue::VectorPartitionMapping(mapping) = decode_work_value(&value)? else {
        return Err(HelixDbError::IndexCatalogCorruption(
            "vector partition mapping key contains another V2 value kind".to_string(),
        ));
    };
    if mapping.index_id != index_id
        || mapping.generation != generation
        || &mapping.partition != partition
        || mapping.partition.fingerprint() != partition.fingerprint()
    {
        return Err(HelixDbError::IndexCatalogCorruption(
            "vector partition mapping key and value disagree".to_string(),
        ));
    }
    Ok(Some(mapping.physical_index_id))
}

/// Resolves or atomically creates one tenant partition mapping.
///
/// The mapping and physical-ID watermark are staged in the caller's graph or
/// builder transaction. Concurrent first writers therefore conflict and retry
/// instead of publishing two physical namespaces for one partition.
pub(crate) async fn stage_vector_partition_mapping(
    transaction: &DbTransaction,
    scope: DataScope,
    index_id: IndexId,
    generation: IndexGenerationId,
    layout: VectorPhysicalLayout,
    partition: &VectorTenantPartition,
) -> Result<VectorPhysicalIndexId> {
    match load_vector_partition_mapping(transaction, scope, index_id, generation, layout, partition)
        .await?
    {
        Some(physical_index_id) => Ok(physical_index_id),
        None => {
            let physical_index_id = allocate_vector_physical_id(transaction).await?;
            let key = vector_partition_mapping_key(scope, index_id, generation, partition);
            transaction.put(
                key,
                encode_work_value(&IndexV2WorkValue::VectorPartitionMapping(
                    VectorPartitionMappingValue {
                        index_id,
                        generation,
                        partition: partition.clone(),
                        physical_index_id,
                    },
                )),
            )?;
            Ok(physical_index_id)
        }
    }
}

/// Deletes one exact empty tenant mapping in its owning graph transaction.
///
/// Re-reading and cross-checking the typed value makes a stale physical ID or
/// partition fingerprint fail closed. The caller must first prove every
/// physical lane empty except the metadata and optional transaction guard that
/// it deletes in the same transaction.
pub(crate) async fn stage_delete_vector_partition_mapping(
    transaction: &DbTransaction,
    scope: DataScope,
    index_id: IndexId,
    generation: IndexGenerationId,
    layout: VectorPhysicalLayout,
    partition: &VectorTenantPartition,
    physical_index_id: VectorPhysicalIndexId,
) -> Result<()> {
    let Some(mapped_physical_id) =
        load_vector_partition_mapping(transaction, scope, index_id, generation, layout, partition)
            .await?
    else {
        return Err(HelixDbError::IndexCatalogCorruption(
            "empty vector partition lost its mapping before reclamation".to_string(),
        ));
    };
    if mapped_physical_id != physical_index_id {
        return Err(HelixDbError::IndexCatalogCorruption(
            "empty vector partition mapping changed physical ownership".to_string(),
        ));
    }
    transaction.delete(vector_partition_mapping_key(
        scope, index_id, generation, partition,
    ))?;
    Ok(())
}

fn vector_partition_mapping_key(
    scope: DataScope,
    index_id: IndexId,
    generation: IndexGenerationId,
    partition: &VectorTenantPartition,
) -> Bytes {
    Key::Data {
        scope,
        kind: DataKeyKind::IndexV2(IndexV2Key::VectorPartitionMapping(
            VectorPartitionMappingKey {
                index_id,
                generation,
                partition: partition.fingerprint(),
            },
        )),
    }
    .to_bytes()
}

/// Finds an unused operation ID without writing outside the caller's transaction.
pub(crate) async fn allocate_operation_id(
    transaction: &DbTransaction,
    scope: DataScope,
) -> Result<IndexOperationId> {
    allocate_operation_id_from(
        transaction,
        scope,
        std::iter::repeat_with(IndexOperationId::new_v4).take(UUID_ALLOCATION_ATTEMPTS),
        UUID_ALLOCATION_ATTEMPTS,
    )
    .await
}

async fn allocate_operation_id_from(
    transaction: &DbTransaction,
    scope: DataScope,
    candidates: impl IntoIterator<Item = IndexOperationId>,
    attempts: usize,
) -> Result<IndexOperationId> {
    for candidate in candidates {
        let scoped = Key::Data {
            scope,
            kind: DataKeyKind::IndexV2(IndexV2Key::operation(candidate)),
        }
        .to_bytes();
        let pointer = global_key(GlobalIndexV2Key::OperationPointer(candidate));
        if transaction.get(scoped).await?.is_none() && transaction.get(pointer).await?.is_none() {
            return Ok(candidate);
        }
    }
    Err(HelixDbError::IdentifierAllocationFailed {
        kind: "index operation ID",
        attempts,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use slatedb::object_store::memory::InMemory;

    use super::*;

    #[test]
    fn storage_version_three_is_current() {
        assert_eq!(IndexStorageVersion::CURRENT.get(), 0x0003);
        let marker = encode_metadata_value(&IndexV2MetadataValue::StorageVersion(
            IndexStorageVersion::CURRENT,
        ));
        let logical = encode_metadata_value(&IndexV2MetadataValue::LogicalIndexIdWatermark(
            LogicalIndexIdWatermark {
                next_id: IndexId::initial(),
            },
        ));
        let vector = encode_metadata_value(&IndexV2MetadataValue::VectorPhysicalIdWatermark(
            VectorPhysicalIdWatermark {
                next_id: VectorPhysicalIndexId::initial(),
            },
        ));
        assert_eq!(
            validate_bootstrap_values(&marker, Some(&logical), Some(&vector))
                .expect("storage version 3 is accepted"),
            ValidatedReaderBootstrap::Current
        );
    }

    #[test]
    fn storage_version_one_requires_migration() {
        let version_one =
            IndexStorageVersion::new(0x0001).expect("storage version one remains representable");
        let marker = encode_metadata_value(&IndexV2MetadataValue::StorageVersion(version_one));
        let error = validate_bootstrap_values(&marker, None, None)
            .expect_err("storage version one requires migration");
        let HelixDbError::MigrationRequired { reason } = error else {
            panic!("storage version one returns MigrationRequired");
        };
        assert_eq!(
            reason,
            format!(
                "index storage version {} predates required version {}; recreate this development database",
                version_one.get(),
                IndexStorageVersion::CURRENT.get()
            )
        );
    }

    #[test]
    fn storage_version_four_is_unsupported() {
        let version_four =
            IndexStorageVersion::new(0x0004).expect("storage version four remains representable");
        let marker = encode_metadata_value(&IndexV2MetadataValue::StorageVersion(version_four));
        assert!(matches!(
            validate_bootstrap_values(&marker, None, None),
            Err(HelixDbError::UnsupportedIndexStorageVersion {
                found: 0x0004,
                supported: 0x0003,
            })
        ));
    }

    async fn put_bootstrap_tuple(db: &Db, version: IndexStorageVersion) {
        db.put(
            global_key(GlobalIndexV2Key::StorageVersion),
            encode_metadata_value(&IndexV2MetadataValue::StorageVersion(version)),
        )
        .await
        .unwrap();
        db.put(
            global_key(GlobalIndexV2Key::LogicalIndexIdWatermark),
            encode_metadata_value(&IndexV2MetadataValue::LogicalIndexIdWatermark(
                LogicalIndexIdWatermark {
                    next_id: IndexId::initial(),
                },
            )),
        )
        .await
        .unwrap();
        db.put(
            global_key(GlobalIndexV2Key::VectorPhysicalIdWatermark),
            encode_metadata_value(&IndexV2MetadataValue::VectorPhysicalIdWatermark(
                VectorPhysicalIdWatermark {
                    next_id: VectorPhysicalIndexId::initial(),
                },
            )),
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn complete_v2_reader_requires_writer_migration_without_writing() {
        let db = Db::builder("index-v2-reader-migration", Arc::new(InMemory::new()))
            .build()
            .await
            .unwrap();
        let version_two = IndexStorageVersion::new(0x0002).unwrap();
        put_bootstrap_tuple(&db, version_two).await;
        let marker_key = global_key(GlobalIndexV2Key::StorageVersion);
        let marker_before = db.get(&marker_key).await.unwrap().unwrap();

        let error = require_reader_bootstrap_or_legacy(&db)
            .await
            .expect_err("a complete V2 store requires writer-owned migration");

        assert!(matches!(
            error,
            HelixDbError::WriterMigrationRequired {
                requirement: WriterMigrationRequirement::StorageVersion {
                    found: 0x0002,
                    target: 0x0003,
                },
            }
        ));
        assert_eq!(db.get(marker_key).await.unwrap().unwrap(), marker_before);
    }

    #[tokio::test]
    async fn current_incomplete_reader_requires_writer_migration() {
        let db = Db::builder("index-v3-reader-migration", Arc::new(InMemory::new()))
            .build()
            .await
            .unwrap();
        bootstrap_writer(&db).await.unwrap();

        assert!(matches!(
            require_reader_bootstrap_or_legacy(&db).await,
            Err(HelixDbError::WriterMigrationRequired {
                requirement: WriterMigrationRequirement::IncompleteStorageSchema,
            })
        ));
    }

    #[test]
    fn partial_v2_tuple_is_not_writer_promotable() {
        let version_two = IndexStorageVersion::new(0x0002).unwrap();
        let marker = encode_metadata_value(&IndexV2MetadataValue::StorageVersion(version_two));

        assert!(matches!(
            validate_bootstrap_values(&marker, None, None),
            Err(HelixDbError::MigrationRequired { .. })
        ));
    }

    #[tokio::test]
    async fn writer_atomically_upgrades_a_complete_v2_tuple() {
        let db = Db::builder("index-v3-storage-upgrade", Arc::new(InMemory::new()))
            .build()
            .await
            .unwrap();
        put_bootstrap_tuple(&db, IndexStorageVersion::new(0x0002).unwrap()).await;

        bootstrap_writer(&db).await.unwrap();

        let marker = db
            .get(global_key(GlobalIndexV2Key::StorageVersion))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            decode_metadata_value(&marker).unwrap(),
            IndexV2MetadataValue::StorageVersion(IndexStorageVersion::CURRENT)
        );
    }
}
