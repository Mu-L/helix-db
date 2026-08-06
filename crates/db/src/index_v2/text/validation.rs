//! Bounded, restartable proof of prepared V2 text manifests.
//!
//! Validation runs after every build artifact has been relocated into immutable
//! manifest pages and before the canonical index can become `Active`. Each
//! call observes exactly one page or root, or proves one lane
//! exhausted. The returned preparation retains exact range and point reads so
//! the serializable operation transaction can reject stale physical state.
//!
//! Page selection deliberately performs no object-store work. The driver checks
//! immutable blob metadata after the database snapshot is dropped.

use std::collections::HashSet;
use std::ops::Bound;

use bytes::Bytes;
use slatedb::DbTransaction;

use crate::config::SearchIndexBatchLimits;
use crate::encoding::v1::keys::index_v2 as index_keys;
use crate::encoding::v1::keys::tenant::DataScope;
use crate::encoding::v1::keys::{DataKeyKind, Key};
use crate::encoding::v1::values::index_v2 as index_values;
use crate::error::{HelixDbError, Result};
use crate::index_v2::outbox::IndexOperationStepResult;
use crate::index_v2::work;
use crate::index_v2::{
    IndexCursor, IndexOperationBlocker, IndexOperationProgress, IndexOperationRecord,
    OperationCounters, PrefixScanProgress, TextBuildProgress, TextBuildStage,
    TextManifestPageValidationProgress, TextManifestPartitionValidation,
    TextManifestValidationProgress, TextPartition, ValidatedTextIndexDefinition,
};

/// One prepared validation decision that needs no external blob authority.
#[derive(Debug)]
pub(crate) struct PreparedDatabaseValidation {
    ranges: Vec<PreparedValidationRange>,
    observations: Vec<RowObservation>,
    result: IndexOperationStepResult,
}

impl PreparedDatabaseValidation {
    /// Revalidates every exact range and point observation before returning its result.
    pub(super) async fn stage(
        &self,
        transaction: &DbTransaction,
    ) -> Result<IndexOperationStepResult> {
        for range in &self.ranges {
            if !range.is_current(transaction).await? {
                return Ok(IndexOperationStepResult::TransientFailure);
            }
        }
        for observation in &self.observations {
            if transaction.get(&observation.key).await? != observation.value {
                return Ok(IndexOperationStepResult::TransientFailure);
            }
        }
        Ok(self.result.clone())
    }
}

/// One page proof whose distinct blobs still need runtime reference guards.
#[derive(Debug)]
pub(crate) struct PreparedPageValidation {
    database: PreparedDatabaseValidation,
    blobs: Vec<work::BlobRef>,
}

impl PreparedPageValidation {
    /// Borrows every distinct page blob requiring a size proof.
    pub(super) fn blobs(&self) -> &[work::BlobRef] {
        &self.blobs
    }

    /// Revalidates the database proof after object metadata validation.
    pub(super) async fn stage(
        &self,
        transaction: &DbTransaction,
    ) -> Result<IndexOperationStepResult> {
        self.database.stage(transaction).await
    }

    /// Converts a failed external proof into a range-fenced database result.
    pub(super) fn into_database_with_result(
        mut self,
        result: IndexOperationStepResult,
    ) -> PreparedDatabaseValidation {
        self.database.result = result;
        self.database
    }
}

/// Closed preparation shape for one validation checkpoint.
#[derive(Debug)]
pub(super) enum ValidationSelection {
    /// Root, exhaustion, or a durable invariant blocker.
    Database(PreparedDatabaseValidation),
    /// Valid page metadata that still needs external blob validation.
    Page(PreparedPageValidation),
}

/// Exact ordered rows retained for serializable range revalidation.
#[derive(Debug)]
struct PreparedValidationRange {
    prefix: Bytes,
    start: Bound<Bytes>,
    end: Bound<Bytes>,
    rows: Vec<(Bytes, Bytes)>,
}

impl PreparedValidationRange {
    /// Replays one selected or exhausted interval inside the commit transaction.
    async fn is_current(&self, transaction: &DbTransaction) -> Result<bool> {
        let bounds = (self.start.clone(), self.end.clone());
        let mut current = transaction.scan_prefix(&self.prefix, bounds).await?;
        for (expected_key, expected_value) in &self.rows {
            let Some(row) = current.next().await? else {
                return Ok(false);
            };
            if row.key != expected_key || row.value != expected_value {
                return Ok(false);
            }
        }
        Ok(current.next().await?.is_none())
    }
}

/// Exact point-read retained with one prepared validation result.
#[derive(Debug)]
struct RowObservation {
    key: Bytes,
    value: Option<Bytes>,
}

/// Selects one bounded validation checkpoint from the current closed lane.
pub(super) async fn select(
    transaction: &DbTransaction,
    scope: DataScope,
    operation: &IndexOperationRecord,
    definition: &ValidatedTextIndexDefinition,
    progress: &TextManifestValidationProgress,
    limits: SearchIndexBatchLimits,
) -> Result<ValidationSelection> {
    match progress {
        TextManifestValidationProgress::Pages(progress) => {
            select_page(transaction, scope, operation, progress, limits).await
        }
        TextManifestValidationProgress::Roots(progress) => {
            select_root(transaction, scope, operation, definition, progress, limits).await
        }
        TextManifestValidationProgress::EntityStates(progress) => {
            select_entity_state(transaction, scope, operation, progress, limits).await
        }
    }
}

/// Validates one immutable page and its exact root relationship.
async fn select_page(
    transaction: &DbTransaction,
    scope: DataScope,
    operation: &IndexOperationRecord,
    progress: &TextManifestPageValidationProgress,
    limits: SearchIndexBatchLimits,
) -> Result<ValidationSelection> {
    let prefix = generation_prefix(
        scope,
        index_keys::IndexV2RecordKind::TextManifestPage,
        operation,
    );
    let (range, row) = select_one(transaction, prefix, progress.cursor()).await?;
    let Some((row_key, row_value)) = row else {
        let result = if progress.partition().is_some() {
            IndexOperationStepResult::Blocked(IndexOperationBlocker::InvariantViolation)
        } else {
            progressed(TextBuildStage::ValidateManifests(
                TextManifestValidationProgress::Roots(PrefixScanProgress {
                    cursor: None,
                    counters: progress.counters(),
                }),
            ))
        };
        return Ok(ValidationSelection::Database(PreparedDatabaseValidation {
            ranges: vec![range],
            observations: Vec::new(),
            result,
        }));
    };

    let page_key = match Key::parse_from_slice(scope, &row_key) {
        Ok(Key::Data {
            kind: DataKeyKind::IndexV2(index_keys::IndexV2Key::TextManifestPage(key)),
            ..
        }) => key,
        Ok(Key::Data { .. } | Key::Global { .. }) | Err(_) => {
            return Ok(blocked_database(vec![range], Vec::new()));
        }
    };
    let page = match index_values::decode_work_value(&row_value) {
        Ok(index_values::IndexV2WorkValue::TextManifestPage(page)) => page,
        Ok(_) | Err(_) => return Ok(blocked_database(vec![range], Vec::new())),
    };
    let root_key = scoped_key(
        scope,
        index_keys::IndexV2Key::TextManifestRoot(page_key.root),
    );
    let root_value = transaction.get(&root_key).await?;
    let observations = vec![RowObservation {
        key: root_key,
        value: root_value.clone(),
    }];
    let Some(root_value) = root_value.as_ref() else {
        return Ok(blocked_database(vec![range], observations));
    };
    let root = match index_values::decode_work_value(root_value) {
        Ok(index_values::IndexV2WorkValue::TextManifestRoot(root)) => root,
        Ok(_) | Err(_) => return Ok(blocked_database(vec![range], observations)),
    };

    let minimum_revision = u64::from(root.page_count()).saturating_add(1);
    if page_key.root.index_id != operation.index_id()
        || page_key.root.generation != operation.generation()
        || page.index_id() != operation.index_id()
        || page.generation() != operation.generation()
        || page_key.root.partition != page.partition().fingerprint()
        || page_key.page != page.page()
        || root.index_id() != operation.index_id()
        || root.generation() != operation.generation()
        || page_key.root.partition != root.partition().fingerprint()
        || root.partition() != page.partition()
        || root.page_count() == 0
        || root.split_count() == 0
        || root.revision().get() < minimum_revision
        || page_key.page >= root.page_count()
    {
        return Ok(blocked_database(vec![range], observations));
    }

    let observed_before = match progress.partition() {
        Some(partition)
            if partition.partition_fingerprint() == page_key.root.partition.as_bytes()
                && partition.root_revision() == root.revision()
                && partition.page_count() == root.page_count()
                && partition.split_count() == root.split_count()
                && partition.next_page() == page_key.page =>
        {
            partition.observed_split_count()
        }
        None if page_key.page == 0 => 0,
        Some(_) | None => return Ok(blocked_database(vec![range], observations)),
    };
    let page_split_count =
        u64::try_from(page.entries().len()).expect("bounded manifest-page length fits u64");
    // Both values are bounded by a valid root's page count times MAX_ENTRIES.
    let observed_split_count = observed_before + page_split_count;
    // `page < root.page_count <= u32::MAX` proves this addition cannot overflow.
    let next_page = page_key.page + 1;
    let next_partition = if next_page == root.page_count() {
        if observed_split_count != root.split_count() {
            return Ok(blocked_database(vec![range], observations));
        }
        None
    } else {
        let Ok(partition) = TextManifestPartitionValidation::try_new(
            *page_key.root.partition.as_bytes(),
            root.revision(),
            root.page_count(),
            root.split_count(),
            next_page,
            observed_split_count,
        ) else {
            return Ok(blocked_database(vec![range], observations));
        };
        Some(partition)
    };

    let mut blobs = Vec::with_capacity(page.entries().len());
    let mut distinct_blobs = HashSet::with_capacity(page.entries().len());
    for split in page.entries().iter().copied() {
        let blob = split.blob();
        if !crate::search::text::split_reference_layout_is_exact(
            split.footer_offset(),
            split.footer_length(),
            split.hot_cache_length(),
            split.total_size(),
        ) || !distinct_blobs.insert(blob)
        {
            return Ok(blocked_database(vec![range], observations));
        }
        blobs.push(blob);
    }

    let input_bytes = row_bytes(&row_key, Some(&row_value)).saturating_add(
        observations.iter().fold(0_u64, |bytes, observation| {
            bytes.saturating_add(row_bytes(&observation.key, observation.value.as_ref()))
        }),
    );
    if input_bytes > limits.max_input_bytes().get() {
        return Ok(ValidationSelection::Database(PreparedDatabaseValidation {
            ranges: vec![range],
            observations,
            result: IndexOperationStepResult::Blocked(IndexOperationBlocker::ManifestLimit {
                partition: page.partition().clone(),
                observed: input_bytes,
                limit: limits.max_input_bytes().get(),
            }),
        }));
    }
    let Some(input_bytes) = progress.counters().input_bytes.checked_add(input_bytes) else {
        return Err(corruption(
            "text manifest-validation input counter overflowed",
        ));
    };
    let counters = OperationCounters {
        input_bytes,
        ..progress.counters()
    };
    let cursor = IndexCursor::try_new(row_key)
        .map_err(|error| HelixDbError::InvariantViolation(error.to_string()))?;
    let next = TextManifestPageValidationProgress::try_new(Some(cursor), next_partition, counters)
        .map_err(|error| HelixDbError::InvariantViolation(error.to_string()))?;
    Ok(ValidationSelection::Page(PreparedPageValidation {
        database: PreparedDatabaseValidation {
            ranges: vec![range],
            observations,
            result: progressed(TextBuildStage::ValidateManifests(
                TextManifestValidationProgress::Pages(next),
            )),
        },
        blobs,
    }))
}

/// Validates one manifest root, including the canonical empty representation.
async fn select_root(
    transaction: &DbTransaction,
    scope: DataScope,
    operation: &IndexOperationRecord,
    definition: &ValidatedTextIndexDefinition,
    progress: &PrefixScanProgress,
    limits: SearchIndexBatchLimits,
) -> Result<ValidationSelection> {
    let prefix = generation_prefix(
        scope,
        index_keys::IndexV2RecordKind::TextManifestRoot,
        operation,
    );
    let (range, row) = select_one(transaction, prefix, progress.cursor.as_ref()).await?;
    let Some((row_key, row_value)) = row else {
        return Ok(ValidationSelection::Database(PreparedDatabaseValidation {
            ranges: vec![range],
            observations: Vec::new(),
            result: progressed(TextBuildStage::ValidateManifests(
                TextManifestValidationProgress::EntityStates(PrefixScanProgress {
                    cursor: None,
                    counters: progress.counters,
                }),
            )),
        }));
    };
    let root_key = match Key::parse_from_slice(scope, &row_key) {
        Ok(Key::Data {
            kind: DataKeyKind::IndexV2(index_keys::IndexV2Key::TextManifestRoot(key)),
            ..
        }) => key,
        Ok(Key::Data { .. } | Key::Global { .. }) | Err(_) => {
            return Ok(blocked_database(vec![range], Vec::new()));
        }
    };
    let root = match index_values::decode_work_value(&row_value) {
        Ok(index_values::IndexV2WorkValue::TextManifestRoot(root)) => root,
        Ok(_) | Err(_) => return Ok(blocked_database(vec![range], Vec::new())),
    };
    let partition_mode_is_valid = match (definition.tenant_property(), root.partition()) {
        (None, TextPartition::Unpartitioned) | (Some(_), TextPartition::TenantValue(_)) => true,
        (None, TextPartition::TenantValue(_)) | (Some(_), TextPartition::Unpartitioned) => false,
    };
    let revision_is_valid = if root.page_count() == 0 {
        root.split_count() == 0
    } else {
        root.revision().get() >= u64::from(root.page_count()).saturating_add(1)
            && root.split_count() != 0
    };
    if root_key.index_id != operation.index_id()
        || root_key.generation != operation.generation()
        || root.index_id() != operation.index_id()
        || root.generation() != operation.generation()
        || root_key.partition != root.partition().fingerprint()
        || !partition_mode_is_valid
        || !revision_is_valid
    {
        return Ok(blocked_database(vec![range], Vec::new()));
    }

    let corpus_key = super::statistics::corpus_key(
        scope,
        operation.index_id(),
        operation.generation(),
        root.partition(),
    );
    let corpus_value = transaction.get(&corpus_key).await?;
    let mut observations = vec![RowObservation {
        key: corpus_key,
        value: corpus_value.clone(),
    }];
    if super::statistics::validate_manifest_corpus(
        corpus_value.as_deref(),
        operation.index_id(),
        operation.generation(),
        root.partition(),
        root.split_count(),
    )
    .is_err()
    {
        return Ok(blocked_database(vec![range], observations));
    }
    if root.page_count() != 0 {
        let page_key = scoped_key(
            scope,
            index_keys::IndexV2Key::TextManifestPage(index_keys::TextManifestPageKey {
                root: root_key,
                page: 0,
            }),
        );
        let page_value = transaction.get(&page_key).await?;
        let exact_page_zero = page_value.as_ref().is_some_and(|value| {
            matches!(
                index_values::decode_work_value(value),
                Ok(index_values::IndexV2WorkValue::TextManifestPage(page))
                    if page.index_id() == operation.index_id()
                        && page.generation() == operation.generation()
                        && page.partition() == root.partition()
                        && page.page() == 0
            )
        });
        observations.push(RowObservation {
            key: page_key,
            value: page_value,
        });
        if !exact_page_zero {
            return Ok(blocked_database(vec![range], observations));
        }
    }
    let input_bytes = row_bytes(&row_key, Some(&row_value)).saturating_add(
        observations.iter().fold(0_u64, |bytes, observation| {
            bytes.saturating_add(row_bytes(&observation.key, observation.value.as_ref()))
        }),
    );
    if input_bytes > limits.max_input_bytes().get() {
        return Ok(ValidationSelection::Database(PreparedDatabaseValidation {
            ranges: vec![range],
            observations,
            result: IndexOperationStepResult::Blocked(IndexOperationBlocker::ManifestLimit {
                partition: root.partition().clone(),
                observed: input_bytes,
                limit: limits.max_input_bytes().get(),
            }),
        }));
    }
    let Some(input_bytes) = progress.counters.input_bytes.checked_add(input_bytes) else {
        return Err(corruption("text root-validation input counter overflowed"));
    };
    let counters = OperationCounters {
        input_bytes,
        ..progress.counters
    };
    Ok(ValidationSelection::Database(PreparedDatabaseValidation {
        ranges: vec![range],
        observations,
        result: progressed(TextBuildStage::ValidateManifests(
            TextManifestValidationProgress::Roots(PrefixScanProgress {
                cursor: Some(
                    IndexCursor::try_new(row_key)
                        .map_err(|error| HelixDbError::InvariantViolation(error.to_string()))?,
                ),
                counters,
            }),
        )),
    }))
}

/// Validates one entity-state row against its exact owning root revision.
async fn select_entity_state(
    transaction: &DbTransaction,
    scope: DataScope,
    operation: &IndexOperationRecord,
    progress: &PrefixScanProgress,
    limits: SearchIndexBatchLimits,
) -> Result<ValidationSelection> {
    let prefix = generation_prefix(
        scope,
        index_keys::IndexV2RecordKind::TextEntityState,
        operation,
    );
    let (range, row) = select_one(transaction, prefix, progress.cursor.as_ref()).await?;
    let Some((row_key, row_value)) = row else {
        return select_activation_prerequisites(
            transaction,
            scope,
            operation,
            progress.counters,
            range,
        )
        .await;
    };
    let state_key = match Key::parse_from_slice(scope, &row_key) {
        Ok(Key::Data {
            kind: DataKeyKind::IndexV2(index_keys::IndexV2Key::TextEntityState(key)),
            ..
        }) => key,
        Ok(Key::Data { .. } | Key::Global { .. }) | Err(_) => {
            return Ok(blocked_database(vec![range], Vec::new()));
        }
    };
    let state = match index_values::decode_work_value(&row_value) {
        Ok(index_values::IndexV2WorkValue::TextEntityState(state)) => state,
        Ok(_) | Err(_) => return Ok(blocked_database(vec![range], Vec::new())),
    };
    let root_key = scoped_key(
        scope,
        index_keys::IndexV2Key::TextManifestRoot(state_key.root),
    );
    let root_value = transaction.get(&root_key).await?;
    let mut observations = vec![RowObservation {
        key: root_key,
        value: root_value.clone(),
    }];
    let Some(root_value) = root_value.as_ref() else {
        return Ok(blocked_database(vec![range], observations));
    };
    let root = match index_values::decode_work_value(root_value) {
        Ok(index_values::IndexV2WorkValue::TextManifestRoot(root)) => root,
        Ok(_) | Err(_) => return Ok(blocked_database(vec![range], observations)),
    };
    if state_key.root.index_id != operation.index_id()
        || state_key.root.generation != operation.generation()
        || state.index_id != operation.index_id()
        || state.generation != operation.generation()
        || state_key.root.partition != state.partition.fingerprint()
        || state_key.entity.kind != state.entity_kind
        || state_key.entity.id != state.entity_id
        || root.index_id() != operation.index_id()
        || root.generation() != operation.generation()
        || root.partition() != &state.partition
        || state.logical_version.get() > root.revision().get()
        || (state.live && root.page_count() == 0)
    {
        return Ok(blocked_database(vec![range], observations));
    }
    let marker_key = scoped_key(
        scope,
        index_keys::IndexV2Key::TextStatisticsEntity(index_keys::TextStatisticsEntityKey {
            index_id: operation.index_id(),
            generation: operation.generation(),
            entity: state_key.entity,
        }),
    );
    let marker_value = transaction.get(&marker_key).await?;
    observations.push(RowObservation {
        key: marker_key,
        value: marker_value.clone(),
    });
    let Some(marker_value) = marker_value.as_ref() else {
        return Ok(blocked_database(vec![range], observations));
    };
    let marker = match index_values::decode_work_value(marker_value) {
        Ok(index_values::IndexV2WorkValue::TextStatisticsEntity(marker)) => marker,
        Ok(_) | Err(_) => return Ok(blocked_database(vec![range], observations)),
    };
    if marker.index_id != operation.index_id()
        || marker.generation != operation.generation()
        || marker.entity_kind != state_key.entity.kind
        || marker.entity_id != state_key.entity.id
    {
        return Ok(blocked_database(vec![range], observations));
    }
    let marker_matches = match (&marker.contribution, state.live) {
        (work::TextStatisticsContribution::Present { partition, .. }, true) => {
            partition == &state.partition
        }
        (work::TextStatisticsContribution::Absent, false) => true,
        (work::TextStatisticsContribution::Present { partition, .. }, false)
            if partition != &state.partition =>
        {
            let live_key = scoped_key(
                scope,
                index_keys::IndexV2Key::TextEntityState(index_keys::TextEntityStateKey {
                    root: index_keys::TextManifestRootKey {
                        index_id: operation.index_id(),
                        generation: operation.generation(),
                        partition: partition.fingerprint(),
                    },
                    entity: state_key.entity,
                }),
            );
            let live_value = transaction.get(&live_key).await?;
            let exact_live_state = live_value.as_ref().is_some_and(|value| {
                matches!(
                    index_values::decode_work_value(value),
                    Ok(index_values::IndexV2WorkValue::TextEntityState(live))
                        if live.index_id == operation.index_id()
                            && live.generation == operation.generation()
                            && live.partition == *partition
                            && live.entity_kind == state_key.entity.kind
                            && live.entity_id == state_key.entity.id
                            && live.live
                )
            });
            observations.push(RowObservation {
                key: live_key,
                value: live_value,
            });
            exact_live_state
        }
        (work::TextStatisticsContribution::Absent, true)
        | (work::TextStatisticsContribution::Present { .. }, false) => false,
    };
    if !marker_matches {
        return Ok(blocked_database(vec![range], observations));
    }
    let input_bytes = row_bytes(&row_key, Some(&row_value)).saturating_add(
        observations.iter().fold(0_u64, |bytes, observation| {
            bytes.saturating_add(row_bytes(&observation.key, observation.value.as_ref()))
        }),
    );
    if input_bytes > limits.max_input_bytes().get() {
        return Ok(ValidationSelection::Database(PreparedDatabaseValidation {
            ranges: vec![range],
            observations,
            result: IndexOperationStepResult::Blocked(IndexOperationBlocker::ManifestLimit {
                partition: state.partition,
                observed: input_bytes,
                limit: limits.max_input_bytes().get(),
            }),
        }));
    }
    let Some(input_bytes) = progress.counters.input_bytes.checked_add(input_bytes) else {
        return Err(corruption(
            "text entity-state validation input counter overflowed",
        ));
    };
    let counters = OperationCounters {
        input_bytes,
        ..progress.counters
    };
    Ok(ValidationSelection::Database(PreparedDatabaseValidation {
        ranges: vec![range],
        observations,
        result: progressed(TextBuildStage::ValidateManifests(
            TextManifestValidationProgress::EntityStates(PrefixScanProgress {
                cursor: Some(
                    IndexCursor::try_new(row_key)
                        .map_err(|error| HelixDbError::InvariantViolation(error.to_string()))?,
                ),
                counters,
            }),
        )),
    }))
}

/// Proves no late delta or artifact can cross activation.
async fn select_activation_prerequisites(
    transaction: &DbTransaction,
    scope: DataScope,
    operation: &IndexOperationRecord,
    counters: OperationCounters,
    entity_state_range: PreparedValidationRange,
) -> Result<ValidationSelection> {
    let delta_prefix =
        generation_prefix(scope, index_keys::IndexV2RecordKind::BuildDelta, operation);
    let (delta_range, delta) = select_one(transaction, delta_prefix, None).await?;
    let artifact_prefix = generation_prefix(
        scope,
        index_keys::IndexV2RecordKind::TextBuildArtifact,
        operation,
    );
    let (artifact_range, artifact) = select_one(transaction, artifact_prefix, None).await?;
    let result = if delta.is_some() {
        progressed(TextBuildStage::CatchUp(PrefixScanProgress {
            cursor: None,
            counters,
        }))
    } else if artifact.is_some() {
        IndexOperationStepResult::Blocked(IndexOperationBlocker::InvariantViolation)
    } else {
        progressed(TextBuildStage::Activate(
            crate::index_v2::NoCursorProgress { counters },
        ))
    };
    Ok(ValidationSelection::Database(PreparedDatabaseValidation {
        ranges: vec![entity_state_range, delta_range, artifact_range],
        observations: Vec::new(),
        result,
    }))
}

/// Selects one exact row or one exact exhausted suffix from a typed prefix.
async fn select_one(
    transaction: &DbTransaction,
    prefix: Bytes,
    cursor: Option<&IndexCursor>,
) -> Result<(PreparedValidationRange, Option<(Bytes, Bytes)>)> {
    let start = match cursor {
        Some(cursor) => {
            let Some(suffix) = cursor.as_bytes().strip_prefix(prefix.as_ref()) else {
                return Err(corruption(
                    "text manifest-validation cursor is outside its exact prefix",
                ));
            };
            Bound::Excluded(Bytes::copy_from_slice(suffix))
        }
        None => Bound::Unbounded,
    };
    let bounds = (start.clone(), Bound::<Bytes>::Unbounded);
    let mut rows = transaction.scan_prefix(&prefix, bounds).await?;
    let Some(row) = rows.next().await? else {
        return Ok((
            PreparedValidationRange {
                prefix,
                start,
                end: Bound::Unbounded,
                rows: Vec::new(),
            },
            None,
        ));
    };
    let suffix = row
        .key
        .strip_prefix(prefix.as_ref())
        .expect("scan_prefix returns only keys with the requested prefix");
    let end = Bound::Included(Bytes::copy_from_slice(suffix));
    let selected = (row.key, row.value);
    Ok((
        PreparedValidationRange {
            prefix,
            start,
            end,
            rows: vec![selected.clone()],
        },
        Some(selected),
    ))
}

/// Constructs a range-fenced durable invariant blocker.
fn blocked_database(
    ranges: Vec<PreparedValidationRange>,
    observations: Vec<RowObservation>,
) -> ValidationSelection {
    ValidationSelection::Database(PreparedDatabaseValidation {
        ranges,
        observations,
        result: IndexOperationStepResult::Blocked(IndexOperationBlocker::InvariantViolation),
    })
}

/// Encodes a complete generation prefix through the canonical V1 key codec.
fn generation_prefix(
    scope: DataScope,
    kind: index_keys::IndexV2RecordKind,
    operation: &IndexOperationRecord,
) -> Bytes {
    Key::data_prefix(
        scope,
        index_keys::IndexV2Key::generation_prefix(
            kind,
            operation.index_id(),
            operation.generation(),
        ),
    )
}

/// Encodes one scoped logical key through the canonical V1 key codec.
fn scoped_key(scope: DataScope, key: index_keys::IndexV2Key) -> Bytes {
    Key::Data {
        scope,
        kind: DataKeyKind::IndexV2(key),
    }
    .to_bytes()
}

/// Measures one exact observed key/value row without allocation.
fn row_bytes(key: &Bytes, value: Option<&Bytes>) -> u64 {
    u64::try_from(key.len().saturating_add(value.map_or(0, Bytes::len))).unwrap_or(u64::MAX)
}

/// Wraps a validation stage in the only legal constructing progress shape.
fn progressed(stage: TextBuildStage) -> IndexOperationStepResult {
    IndexOperationStepResult::Progressed(IndexOperationProgress::TextBuild(
        TextBuildProgress::Constructing(stage),
    ))
}

fn corruption(message: impl Into<String>) -> HelixDbError {
    HelixDbError::IndexCatalogCorruption(message.into())
}
