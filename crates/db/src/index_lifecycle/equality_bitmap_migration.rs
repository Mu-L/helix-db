//! Blocking V3-to-V4 non-unique equality migration.
//!
//! The V3 marker remains authoritative while Active generations are rebuilt
//! and verified. Publication is one final marker write; V3 rows are deleted
//! only afterward, so every committed prefix is restart-safe.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use bytes::Bytes;
use roaring::RoaringTreemap;
use slatedb::{Db, IsolationLevel};

use crate::encoding::property::decode_properties;
use crate::encoding::v1::keys::tenant::{DataScope, TenantId};
use crate::encoding::v2::keys::{
    CanonicalSecondaryValue, GlobalKey, Key, RecordKind, ScopedKey, SecondaryEqualityBitmapKey,
};
use crate::encoding::v2::values::{
    decode_index_record, decode_operation_record, encode_metadata_value, encode_operation_record,
    SecondaryEqualityBitmapValue,
};
use crate::error::{HelixDbError, Result};

use super::operation::{
    IndexOperationExecutionState, IndexOperationProgress, SecondaryBuildProgress,
    SecondaryBuildStage, SourceScanProgress,
};
use super::{
    IndexOperationRecord, IndexStateV2, IndexStorageVersion, IndexV2MetadataValue,
    OperationCounters, OperationQueuePointerValue, PhysicalGeneration,
    ValidatedDynamicIndexDefinition, ValidatedSecondaryIndexDefinition,
};

const MIGRATION_BATCH_SIZE: usize = 256;

#[derive(Debug, Clone)]
struct EqualityGeneration {
    scope: DataScope,
    definition: ValidatedSecondaryIndexDefinition,
    index_id: super::IndexId,
    generation: super::IndexGenerationId,
}

#[derive(Debug, Clone)]
struct BuildingEqualityGeneration {
    generation: EqualityGeneration,
    operation_id: super::IndexOperationId,
}

#[derive(Debug, Default)]
struct MigrationCatalog {
    all: Vec<EqualityGeneration>,
    active: Vec<EqualityGeneration>,
    building: Vec<BuildingEqualityGeneration>,
}

/// Stable restart boundaries spanning every durable migration phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EqualityBitmapMigrationFailpoint {
    InitializationBefore,
    InitializationAfter,
    BatchBefore,
    BatchAfter,
    VerificationBefore,
    VerificationAfter,
    PublicationBefore,
    PublicationAfter,
    CleanupBefore,
    CleanupAfter,
}

impl EqualityBitmapMigrationFailpoint {
    #[cfg(test)]
    pub(crate) const ALL: [Self; 10] = [
        Self::InitializationBefore,
        Self::InitializationAfter,
        Self::BatchBefore,
        Self::BatchAfter,
        Self::VerificationBefore,
        Self::VerificationAfter,
        Self::PublicationBefore,
        Self::PublicationAfter,
        Self::CleanupBefore,
        Self::CleanupAfter,
    ];

    const fn as_str(self) -> &'static str {
        match self {
            Self::InitializationBefore => "initialization_before",
            Self::InitializationAfter => "initialization_after",
            Self::BatchBefore => "batch_before",
            Self::BatchAfter => "batch_after",
            Self::VerificationBefore => "verification_before",
            Self::VerificationAfter => "verification_after",
            Self::PublicationBefore => "publication_before",
            Self::PublicationAfter => "publication_after",
            Self::CleanupBefore => "cleanup_before",
            Self::CleanupAfter => "cleanup_after",
        }
    }
}

static INJECTED_FAILPOINT: Mutex<Option<EqualityBitmapMigrationFailpoint>> = Mutex::new(None);
static FAILPOINT_TRIGGERED: AtomicBool = AtomicBool::new(false);

#[cfg(test)]
pub(crate) fn inject_once(failpoint: EqualityBitmapMigrationFailpoint) -> Result<()> {
    let mut injected = INJECTED_FAILPOINT.lock().map_err(|_| {
        HelixDbError::InvariantViolation(
            "equality bitmap migration failpoint mutex was poisoned".to_string(),
        )
    })?;
    *injected = Some(failpoint);
    FAILPOINT_TRIGGERED.store(false, Ordering::SeqCst);
    Ok(())
}

#[cfg(test)]
pub(crate) fn was_triggered() -> bool {
    FAILPOINT_TRIGGERED.load(Ordering::SeqCst)
}

fn trip(failpoint: EqualityBitmapMigrationFailpoint) -> Result<()> {
    let mut injected = INJECTED_FAILPOINT.lock().map_err(|_| {
        HelixDbError::InvariantViolation(
            "equality bitmap migration failpoint mutex was poisoned".to_string(),
        )
    })?;
    if *injected == Some(failpoint) {
        *injected = None;
        FAILPOINT_TRIGGERED.store(true, Ordering::SeqCst);
        return Err(injected_error(failpoint));
    }
    drop(injected);

    if std::env::var("HELIX_EQUALITY_BITMAP_MIGRATION_FAILPOINT").as_deref()
        != Ok(failpoint.as_str())
    {
        return Ok(());
    }
    if std::env::var("HELIX_EQUALITY_BITMAP_MIGRATION_FAIL_ACTION").as_deref() == Ok("abort") {
        std::process::abort();
    }
    Err(injected_error(failpoint))
}

fn injected_error(failpoint: EqualityBitmapMigrationFailpoint) -> HelixDbError {
    HelixDbError::InvariantViolation(format!(
        "injected equality bitmap migration failpoint {}",
        failpoint.as_str()
    ))
}

pub(super) async fn migrate_v3_to_v4(db: &Db) -> Result<()> {
    trip(EqualityBitmapMigrationFailpoint::InitializationBefore)?;
    let catalog = discover_catalog(db).await?;
    trip(EqualityBitmapMigrationFailpoint::InitializationAfter)?;

    for generation in &catalog.active {
        rebuild_active_generation(db, generation).await?;
    }
    for generation in &catalog.building {
        restart_building_generation(db, generation).await?;
    }

    trip(EqualityBitmapMigrationFailpoint::PublicationBefore)?;
    let transaction = db.begin(IsolationLevel::SerializableSnapshot).await?;
    transaction.put(
        Key::Global {
            kind: GlobalKey::StorageVersion,
        }
        .to_bytes(),
        encode_metadata_value(&IndexV2MetadataValue::StorageVersion(
            IndexStorageVersion::CURRENT,
        )),
    )?;
    transaction.commit().await?;
    trip(EqualityBitmapMigrationFailpoint::PublicationAfter)?;

    cleanup_v3_nonunique_equality_rows(db).await
}

pub(super) async fn cleanup_v3_nonunique_equality_rows(db: &Db) -> Result<()> {
    trip(EqualityBitmapMigrationFailpoint::CleanupBefore)?;
    let catalog = discover_catalog(db).await?;
    for generation in &catalog.all {
        clear_v3_equality_generation(db, generation).await?;
    }
    trip(EqualityBitmapMigrationFailpoint::CleanupAfter)
}

async fn discover_catalog(db: &Db) -> Result<MigrationCatalog> {
    let mut catalog = MigrationCatalog::default();
    let mut rows = db.scan(..).await?;
    while let Some(row) = rows.next().await? {
        let Some(scope) = index_record_scope(&row.key) else {
            continue;
        };
        let Key::Data {
            kind: ScopedKey::IndexRecord(key),
            ..
        } = Key::parse_from_slice(scope, &row.key)?
        else {
            return Err(corruption(
                "index-record migration prefix yielded another key",
            ));
        };
        let record = decode_index_record(&row.value)?;
        if key.identity != *record.identity() {
            return Err(corruption(
                "index-record migration key/value identities disagree",
            ));
        }
        let ValidatedDynamicIndexDefinition::Secondary(definition) = record.definition() else {
            continue;
        };
        if !super::secondary::definition_uses_equality_bitmap(definition) {
            continue;
        }
        let generation = EqualityGeneration {
            scope,
            definition: definition.clone(),
            index_id: record.index_id(),
            generation: record.state().generation(),
        };
        catalog.all.push(generation.clone());
        match record.state() {
            IndexStateV2::Active {
                physical: PhysicalGeneration::Secondary { .. },
                ..
            } => catalog.active.push(generation),
            IndexStateV2::Building {
                physical: PhysicalGeneration::Secondary { .. },
                build_operation_id,
            } => catalog.building.push(BuildingEqualityGeneration {
                generation,
                operation_id: *build_operation_id,
            }),
            IndexStateV2::Aborting { .. }
            | IndexStateV2::Dropping { .. }
            | IndexStateV2::Dropped { .. } => {}
            IndexStateV2::Active { .. } | IndexStateV2::Building { .. } => {
                return Err(corruption(
                    "secondary equality record owns another physical family",
                ));
            }
        }
    }
    Ok(catalog)
}

async fn rebuild_active_generation(db: &Db, generation: &EqualityGeneration) -> Result<()> {
    clear_prefix(db, &bitmap_prefix(generation)).await?;
    let expected = authoritative_bitmaps(db, generation).await?;
    for chunk in expected
        .iter()
        .collect::<Vec<_>>()
        .chunks(MIGRATION_BATCH_SIZE)
    {
        trip(EqualityBitmapMigrationFailpoint::BatchBefore)?;
        let transaction = db.begin(IsolationLevel::SerializableSnapshot).await?;
        for (key, ids) in chunk {
            transaction.put(
                (*key).clone(),
                SecondaryEqualityBitmapValue::new((*ids).clone()).encode(),
            )?;
        }
        transaction.commit().await?;
        trip(EqualityBitmapMigrationFailpoint::BatchAfter)?;
    }

    trip(EqualityBitmapMigrationFailpoint::VerificationBefore)?;
    let authoritative = authoritative_bitmaps(db, generation).await?;
    let mut remaining = authoritative;
    let prefix = bitmap_prefix(generation);
    let mut rows = db.scan_prefix(prefix, ..).await?;
    while let Some(row) = rows.next().await? {
        let Key::Data {
            kind: ScopedKey::SecondaryEqualityBitmap(key),
            ..
        } = Key::parse_from_slice(generation.scope, &row.key)?
        else {
            return Err(corruption("V4 equality prefix yielded another key kind"));
        };
        if key.index_id != generation.index_id
            || key.generation != generation.generation
            || key.element_kind != generation.definition.element_kind()
        {
            return Err(corruption("V4 equality row escaped its generation prefix"));
        }
        let Some(expected) = remaining.remove(&row.key) else {
            return Err(corruption(
                "V4 equality bitmap has no authoritative graph value",
            ));
        };
        let actual = SecondaryEqualityBitmapValue::decode(&row.value)?.into_ids();
        if actual != expected {
            return Err(corruption(
                "V4 equality bitmap membership differs from authoritative graph state",
            ));
        }
    }
    if !remaining.is_empty() {
        return Err(corruption(
            "authoritative graph values are absent from V4 equality bitmaps",
        ));
    }
    trip(EqualityBitmapMigrationFailpoint::VerificationAfter)
}

async fn authoritative_bitmaps(
    db: &Db,
    generation: &EqualityGeneration,
) -> Result<BTreeMap<Bytes, RoaringTreemap>> {
    let prefix =
        super::secondary::source_prefix(generation.scope, generation.definition.element_kind());
    let mut rows = db.scan_prefix(prefix, ..).await?;
    let mut bitmaps = BTreeMap::<Bytes, RoaringTreemap>::new();
    while let Some(row) = rows.next().await? {
        let Some(entity_id) = super::secondary::source_entity(
            generation.scope,
            generation.definition.element_kind(),
            &row.key,
        )?
        else {
            continue;
        };
        let properties = decode_properties(&row.value)?;
        let canonical =
            super::secondary::canonical_value(&generation.definition, &properties, entity_id)
                .map_err(|_| corruption("authoritative equality value cannot be indexed"))?;
        let Some(CanonicalSecondaryValue::Equality(value)) = canonical else {
            continue;
        };
        let key = Key::Data {
            scope: generation.scope,
            kind: ScopedKey::SecondaryEqualityBitmap(SecondaryEqualityBitmapKey::try_new(
                generation.index_id,
                generation.generation,
                generation.definition.element_kind(),
                value,
            )?),
        }
        .to_bytes();
        bitmaps.entry(key).or_default().insert(entity_id.get());
    }
    Ok(bitmaps)
}

async fn restart_building_generation(db: &Db, building: &BuildingEqualityGeneration) -> Result<()> {
    for prefix in [
        Key::data_prefix(
            building.generation.scope,
            ScopedKey::generation_prefix(
                RecordKind::SecondaryEntry,
                building.generation.index_id,
                building.generation.generation,
            ),
        ),
        bitmap_prefix(&building.generation),
        Key::data_prefix(
            building.generation.scope,
            ScopedKey::generation_prefix(
                RecordKind::BuildDelta,
                building.generation.index_id,
                building.generation.generation,
            ),
        ),
        Key::data_prefix(
            building.generation.scope,
            ScopedKey::generation_prefix(
                RecordKind::AppliedState,
                building.generation.index_id,
                building.generation.generation,
            ),
        ),
    ] {
        clear_prefix(db, &prefix).await?;
    }

    let upper_bound = super::lifecycle::capture_source_upper_bound(
        db,
        building.generation.scope,
        building.generation.definition.element_kind(),
    )
    .await?;
    let operation_key = Key::Data {
        scope: building.generation.scope,
        kind: ScopedKey::operation(building.operation_id),
    }
    .to_bytes();
    let transaction = db.begin(IsolationLevel::SerializableSnapshot).await?;
    let Some(operation_bytes) = transaction.get(&operation_key).await? else {
        return Err(corruption(
            "Building equality generation has no lifecycle operation",
        ));
    };
    let operation = decode_operation_record(&operation_bytes)?;
    if operation.operation_id() != building.operation_id
        || operation.index_id() != building.generation.index_id
        || operation.generation() != building.generation.generation
    {
        return Err(corruption(
            "Building equality generation operation ownership mismatch",
        ));
    }
    let restarted = IndexOperationRecord::try_new(
        operation.operation_id(),
        operation.index_id(),
        operation.identity().clone(),
        operation.generation(),
        operation.index_record_revision(),
        operation.operation_revision().checked_next()?,
        operation.kind(),
        operation.family(),
        IndexOperationProgress::SecondaryBuild(SecondaryBuildProgress::Constructing(
            SecondaryBuildStage::Scan(SourceScanProgress {
                inclusive_upper_bound: upper_bound,
                cursor: None,
                counters: OperationCounters::default(),
            }),
        )),
        operation.attempt(),
        IndexOperationExecutionState::Queued {
            not_before_unix_millis: None,
        },
    )
    .map_err(|error| corruption(&error.to_string()))?;
    transaction.put(operation_key, encode_operation_record(&restarted))?;
    transaction.put(
        Key::Global {
            kind: GlobalKey::OperationPointer(building.operation_id),
        }
        .to_bytes(),
        encode_metadata_value(&IndexV2MetadataValue::OperationQueuePointer(
            OperationQueuePointerValue {
                scope: building.generation.scope,
                index_id: building.generation.index_id,
                generation: building.generation.generation,
                record_revision: restarted.operation_revision(),
            },
        )),
    )?;
    transaction.commit().await?;
    Ok(())
}

async fn clear_prefix(db: &Db, prefix: &Bytes) -> Result<()> {
    loop {
        let transaction = db.begin(IsolationLevel::SerializableSnapshot).await?;
        let mut rows = transaction.scan_prefix(prefix, ..).await?;
        let mut keys = Vec::with_capacity(MIGRATION_BATCH_SIZE);
        while keys.len() < MIGRATION_BATCH_SIZE {
            let Some(row) = rows.next().await? else {
                break;
            };
            keys.push(row.key);
        }
        if keys.is_empty() {
            transaction.rollback();
            return Ok(());
        }
        for key in keys {
            transaction.delete(key)?;
        }
        trip(EqualityBitmapMigrationFailpoint::BatchBefore)?;
        transaction.commit().await?;
        trip(EqualityBitmapMigrationFailpoint::BatchAfter)?;
    }
}

async fn clear_v3_equality_generation(db: &Db, generation: &EqualityGeneration) -> Result<()> {
    let prefix = Key::data_prefix(
        generation.scope,
        ScopedKey::generation_prefix(
            RecordKind::SecondaryEntry,
            generation.index_id,
            generation.generation,
        ),
    );
    loop {
        let transaction = db.begin(IsolationLevel::SerializableSnapshot).await?;
        let mut rows = transaction.scan_prefix(&prefix, ..).await?;
        let mut keys = Vec::with_capacity(MIGRATION_BATCH_SIZE);
        while keys.len() < MIGRATION_BATCH_SIZE {
            let Some(row) = rows.next().await? else {
                break;
            };
            let Key::Data {
                kind: ScopedKey::SecondaryEntry(entry),
                ..
            } = Key::parse_from_slice(generation.scope, &row.key)?
            else {
                return Err(corruption("V3 equality prefix yielded another key kind"));
            };
            let expected_lane = match generation.definition.element_kind() {
                super::IndexElementKind::Node => {
                    crate::encoding::v2::keys::SecondaryEntryLane::NodeEquality
                }
                super::IndexElementKind::Edge => {
                    crate::encoding::v2::keys::SecondaryEntryLane::EdgeEquality
                }
            };
            if entry.lane != expected_lane || entry.entity_id.is_none() {
                return Err(corruption(
                    "V3 non-unique equality generation contains another row shape",
                ));
            }
            keys.push(row.key);
        }
        if keys.is_empty() {
            transaction.rollback();
            return Ok(());
        }
        for key in keys {
            transaction.delete(key)?;
        }
        trip(EqualityBitmapMigrationFailpoint::BatchBefore)?;
        transaction.commit().await?;
        trip(EqualityBitmapMigrationFailpoint::BatchAfter)?;
    }
}

fn bitmap_prefix(generation: &EqualityGeneration) -> Bytes {
    Key::data_prefix(
        generation.scope,
        ScopedKey::secondary_equality_bitmap_prefix(
            generation.index_id,
            generation.generation,
            generation.definition.element_kind(),
        ),
    )
}

fn index_record_scope(key: &[u8]) -> Option<DataScope> {
    const PREFIX_LEN: usize = core::mem::size_of::<u8>();
    const KIND_LEN: usize = core::mem::size_of::<u8>();
    if key.len() >= PREFIX_LEN + KIND_LEN
        && key[0] == ScopedKey::key_prefix()
        && key[PREFIX_LEN] == RecordKind::IndexRecord.as_u8()
    {
        return Some(DataScope::LegacyUnscoped);
    }
    if key.len() < DataScope::PREFIX_LEN + PREFIX_LEN + KIND_LEN
        || key[DataScope::PREFIX_LEN] != ScopedKey::key_prefix()
        || key[DataScope::PREFIX_LEN + PREFIX_LEN] != RecordKind::IndexRecord.as_u8()
    {
        return None;
    }
    let tenant = u128::from_be_bytes(
        key[0..DataScope::PREFIX_LEN]
            .try_into()
            .expect("validated tenant prefix is sixteen bytes"),
    );
    Some(DataScope::Tenant(TenantId::from_u128(tenant)))
}

fn corruption(message: &str) -> HelixDbError {
    HelixDbError::IndexCatalogCorruption(message.to_string())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use slatedb::object_store::memory::InMemory;

    use super::*;
    use crate::config::SecondaryIndexDefinition;
    use crate::encoding::property::{encode_properties, Property};
    use crate::encoding::v1::keys::{DataKeyKind, Key as GraphKey, NodePropertyKey};
    use crate::encoding::v2::keys::{
        CanonicalSecondaryValue, SecondaryEntryKey, SecondaryEntryLane,
    };
    use crate::encoding::v2::values::{
        decode_metadata_value, encode_index_record, encode_secondary_entry,
    };
    use crate::index_lifecycle::work::SecondaryEntryValue;
    use crate::index_lifecycle::{
        IndexGenerationId, IndexId, IndexOperationFamily, IndexOperationKind,
        IndexOperationRevision, IndexRecordV2, IndexRevision, LogicalIndexIdWatermark,
        PrefixScanProgress, VectorPhysicalIdWatermark, VectorPhysicalIndexId,
    };

    static TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    async fn v3_db(name: &str) -> Db {
        let db = Db::builder(name, Arc::new(InMemory::new()))
            .with_merge_operator(Arc::new(crate::merge_operator::HelixMergeOperator::new()))
            .build()
            .await
            .unwrap();
        db.put(
            Key::Global {
                kind: GlobalKey::StorageVersion,
            }
            .to_bytes(),
            encode_metadata_value(&IndexV2MetadataValue::StorageVersion(
                IndexStorageVersion::new(0x0003).unwrap(),
            )),
        )
        .await
        .unwrap();
        db.put(
            Key::Global {
                kind: GlobalKey::LogicalIndexIdWatermark,
            }
            .to_bytes(),
            encode_metadata_value(&IndexV2MetadataValue::LogicalIndexIdWatermark(
                LogicalIndexIdWatermark {
                    next_id: IndexId::new(2).unwrap(),
                },
            )),
        )
        .await
        .unwrap();
        db.put(
            Key::Global {
                kind: GlobalKey::VectorPhysicalIdWatermark,
            }
            .to_bytes(),
            encode_metadata_value(&IndexV2MetadataValue::VectorPhysicalIdWatermark(
                VectorPhysicalIdWatermark {
                    next_id: VectorPhysicalIndexId::initial(),
                },
            )),
        )
        .await
        .unwrap();
        db
    }

    fn equality_definition() -> ValidatedDynamicIndexDefinition {
        SecondaryIndexDefinition::node_equality("User", "email")
            .unwrap()
            .try_into()
            .unwrap()
    }

    async fn put_graph_entity(db: &Db, scope: DataScope, entity_id: u64, email: &str) {
        db.put(
            GraphKey::Data {
                scope,
                kind: DataKeyKind::NodeProperty(NodePropertyKey::new(entity_id)),
            }
            .to_bytes(),
            encode_properties(&[
                Property::string("$label", "User"),
                Property::string("email", email),
            ]),
        )
        .await
        .unwrap();
    }

    async fn put_v3_entry(
        db: &Db,
        scope: DataScope,
        index_id: IndexId,
        generation: IndexGenerationId,
        entity_id: u64,
        email: &str,
    ) {
        let entity_id = super::super::IndexEntityId::new(entity_id);
        let key = Key::Data {
            scope,
            kind: ScopedKey::SecondaryEntry(
                SecondaryEntryKey::try_new(
                    index_id,
                    generation,
                    SecondaryEntryLane::NodeEquality,
                    CanonicalSecondaryValue::equality_string(email),
                    Some(entity_id),
                )
                .unwrap(),
            ),
        }
        .to_bytes();
        db.put(
            key,
            encode_secondary_entry(&SecondaryEntryValue {
                index_id,
                generation,
                lane: SecondaryEntryLane::NodeEquality,
                entity_id,
            }),
        )
        .await
        .unwrap();
    }

    async fn put_active_index(db: &Db, scope: DataScope) -> EqualityGeneration {
        let definition = equality_definition();
        let index_id = IndexId::initial();
        let generation = IndexGenerationId::initial();
        let building = IndexRecordV2::building(
            index_id,
            definition.clone(),
            IndexRevision::initial(),
            PhysicalGeneration::Secondary { generation },
            super::super::IndexOperationId::new_v4(),
        )
        .unwrap();
        let active = building
            .transition(super::super::IndexStateTransition::Activate)
            .unwrap();
        db.put(
            Key::Data {
                scope,
                kind: ScopedKey::index_record(definition.identity()),
            }
            .to_bytes(),
            encode_index_record(&active),
        )
        .await
        .unwrap();
        let ValidatedDynamicIndexDefinition::Secondary(definition) = definition else {
            unreachable!()
        };
        EqualityGeneration {
            scope,
            definition,
            index_id,
            generation,
        }
    }

    async fn setup_active_fixture(name: &str) -> (Db, EqualityGeneration) {
        let db = v3_db(name).await;
        let generation = put_active_index(&db, DataScope::LegacyUnscoped).await;
        for entity_id in 0..2 {
            put_graph_entity(&db, generation.scope, entity_id, "shared").await;
            put_v3_entry(
                &db,
                generation.scope,
                generation.index_id,
                generation.generation,
                entity_id,
                "shared",
            )
            .await;
        }
        (db, generation)
    }

    async fn assert_active_migrated(db: &Db, generation: &EqualityGeneration) {
        let marker = db
            .get(
                Key::Global {
                    kind: GlobalKey::StorageVersion,
                }
                .to_bytes(),
            )
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            decode_metadata_value(&marker).unwrap(),
            IndexV2MetadataValue::StorageVersion(IndexStorageVersion::CURRENT)
        );

        let mut rows = db.scan_prefix(bitmap_prefix(generation), ..).await.unwrap();
        let row = rows.next().await.unwrap().unwrap();
        assert!(rows.next().await.unwrap().is_none());
        assert_eq!(
            SecondaryEqualityBitmapValue::decode(&row.value)
                .unwrap()
                .into_ids(),
            RoaringTreemap::from_iter([0, 1])
        );
        let v3_prefix = Key::data_prefix(
            generation.scope,
            ScopedKey::generation_prefix(
                RecordKind::SecondaryEntry,
                generation.index_id,
                generation.generation,
            ),
        );
        assert!(db
            .scan_prefix(v3_prefix, ..)
            .await
            .unwrap()
            .next()
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn active_v3_rows_rebuild_to_one_verified_v4_bitmap() {
        let _guard = TEST_LOCK.lock().await;
        let (db, generation) = setup_active_fixture("v4-equality-active-migration").await;

        super::super::repository::bootstrap_writer(&db)
            .await
            .unwrap();

        assert_active_migrated(&db, &generation).await;
        db.close().await.unwrap();
    }

    #[tokio::test]
    async fn every_migration_failpoint_restarts_deterministically() {
        let _guard = TEST_LOCK.lock().await;
        for (ordinal, failpoint) in EqualityBitmapMigrationFailpoint::ALL
            .into_iter()
            .enumerate()
        {
            let (db, generation) =
                setup_active_fixture(&format!("v4-equality-restart-{ordinal}")).await;
            inject_once(failpoint).unwrap();
            assert!(super::super::repository::bootstrap_writer(&db)
                .await
                .is_err());
            assert!(was_triggered());

            super::super::repository::bootstrap_writer(&db)
                .await
                .unwrap();
            assert_active_migrated(&db, &generation).await;
            db.close().await.unwrap();
        }
    }

    #[tokio::test]
    async fn incomplete_building_generation_restarts_its_existing_operation() {
        let _guard = TEST_LOCK.lock().await;
        let db = v3_db("v4-equality-building-reset").await;
        let scope = DataScope::LegacyUnscoped;
        let definition = equality_definition();
        let index_id = IndexId::initial();
        let generation = IndexGenerationId::initial();
        let operation_id = super::super::IndexOperationId::new_v4();
        let record = IndexRecordV2::building(
            index_id,
            definition.clone(),
            IndexRevision::initial(),
            PhysicalGeneration::Secondary { generation },
            operation_id,
        )
        .unwrap();
        db.put(
            Key::Data {
                scope,
                kind: ScopedKey::index_record(definition.identity()),
            }
            .to_bytes(),
            encode_index_record(&record),
        )
        .await
        .unwrap();
        let operation = IndexOperationRecord::try_new(
            operation_id,
            index_id,
            definition.identity(),
            generation,
            record.revision(),
            IndexOperationRevision::initial(),
            IndexOperationKind::Build,
            IndexOperationFamily::Secondary,
            IndexOperationProgress::SecondaryBuild(SecondaryBuildProgress::Constructing(
                SecondaryBuildStage::CatchUp(PrefixScanProgress {
                    cursor: None,
                    counters: OperationCounters {
                        entities: 9,
                        ..OperationCounters::default()
                    },
                }),
            )),
            3,
            IndexOperationExecutionState::Queued {
                not_before_unix_millis: None,
            },
        )
        .unwrap();
        db.put(
            Key::Data {
                scope,
                kind: ScopedKey::operation(operation_id),
            }
            .to_bytes(),
            encode_operation_record(&operation),
        )
        .await
        .unwrap();
        db.put(
            Key::Global {
                kind: GlobalKey::OperationPointer(operation_id),
            }
            .to_bytes(),
            encode_metadata_value(&IndexV2MetadataValue::OperationQueuePointer(
                OperationQueuePointerValue {
                    scope,
                    index_id,
                    generation,
                    record_revision: operation.operation_revision(),
                },
            )),
        )
        .await
        .unwrap();
        put_graph_entity(&db, scope, 0, "shared").await;
        put_v3_entry(&db, scope, index_id, generation, 0, "shared").await;

        super::super::repository::bootstrap_writer(&db)
            .await
            .unwrap();

        let restarted = db
            .get(
                Key::Data {
                    scope,
                    kind: ScopedKey::operation(operation_id),
                }
                .to_bytes(),
            )
            .await
            .unwrap()
            .map(|bytes| decode_operation_record(&bytes).unwrap())
            .unwrap();
        assert_eq!(restarted.operation_id(), operation_id);
        assert_eq!(restarted.attempt(), 3);
        assert_eq!(restarted.operation_revision().get(), 2);
        assert!(matches!(
            restarted.progress(),
            IndexOperationProgress::SecondaryBuild(SecondaryBuildProgress::Constructing(
                SecondaryBuildStage::Scan(SourceScanProgress {
                    cursor: None,
                    counters: OperationCounters { entities: 0, .. },
                    ..
                })
            ))
        ));
        let v3_prefix = Key::data_prefix(
            scope,
            ScopedKey::generation_prefix(RecordKind::SecondaryEntry, index_id, generation),
        );
        assert!(db
            .scan_prefix(v3_prefix, ..)
            .await
            .unwrap()
            .next()
            .await
            .unwrap()
            .is_none());
        db.close().await.unwrap();
    }
}
