//! Error types for Helix database operations

use crate::config::ConfigError;
use crate::encoding::error::EncodingError;
use crate::search::vector::{
    VectorConfigError, VectorDistanceMetric, VectorItemDecodeError, VectorValidationError,
};
use slatedb::ErrorKind;

/// Index family whose canonical lifecycle authority is unavailable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexFamily {
    /// Equality and range secondary indexes.
    Secondary,
    /// HNSW vector indexes.
    Vector,
    /// Tantivy text indexes.
    Text,
    /// All dynamic families when graph mutation maintenance cannot be proven.
    DynamicIndexes,
}

impl core::fmt::Display for IndexFamily {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(match self {
            Self::Secondary => "secondary",
            Self::Vector => "vector",
            Self::Text => "text",
            Self::DynamicIndexes => "dynamic indexes",
        })
    }
}

/// Typed reason an index operation must fail closed during the V2 cutover.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexLifecycleUnavailableReason {
    /// Canonical V2 catalog and physical-generation authority is not installed.
    CanonicalStateUnavailable,
    /// A graph write cannot prove exact same-transaction family maintenance.
    MutationMaintenanceUnavailable,
}

/// Writer-owned storage work that must complete before a reader may open.
///
/// Keeping resumable migration states separate from [`HelixDbError::MigrationRequired`]
/// lets managed runtimes expose a control-only process without treating malformed
/// storage as promotable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriterMigrationRequirement {
    /// A writer must atomically advance the durable storage version.
    StorageVersion {
        /// Durable storage version observed by the reader.
        found: u16,
        /// Storage version the current writer will install.
        target: u16,
    },
    /// The current storage version is installed, but writer-owned schema work remains.
    IncompleteStorageSchema,
}

impl core::fmt::Display for WriterMigrationRequirement {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match *self {
            Self::StorageVersion { found, target } => {
                write!(
                    formatter,
                    "storage version {found} must be upgraded to {target}"
                )
            }
            Self::IncompleteStorageSchema => {
                formatter.write_str("storage schema migration is incomplete")
            }
        }
    }
}

/// Serialized resource whose Active text-mutation preflight exceeded policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveTextMutationResource {
    /// Distinct graph entities retained by one flush epoch.
    Entities,
    /// Conservative peak bytes retained by foreground text analysis.
    AnalysisBytes,
    /// Exact database key/value bytes read by the plan.
    InputBytes,
    /// Exact database writes staged by the graph transaction.
    OutputOperations,
    /// Exact database key/value bytes staged by the graph transaction.
    OutputBytes,
    /// Immutable text split payload bytes awaiting publication.
    SplitBytes,
    /// Aggregate immutable payload bytes retained across all destinations.
    RetainedSplitBytes,
    /// Encoded V2 manifest-page value bytes after the append.
    ManifestPageBytes,
}

impl core::fmt::Display for ActiveTextMutationResource {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(match self {
            Self::Entities => "entities",
            Self::AnalysisBytes => "analysis_bytes",
            Self::InputBytes => "input_bytes",
            Self::OutputOperations => "output_operations",
            Self::OutputBytes => "output_bytes",
            Self::SplitBytes => "split_bytes",
            Self::RetainedSplitBytes => "retained_split_bytes",
            Self::ManifestPageBytes => "manifest_page_bytes",
        })
    }
}

impl core::fmt::Display for IndexLifecycleUnavailableReason {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(match self {
            Self::CanonicalStateUnavailable => "canonical V2 state is not installed",
            Self::MutationMaintenanceUnavailable => {
                "same-transaction V2 mutation maintenance is not installed"
            }
        })
    }
}

/// Typed secondary-value failures shared by build and active maintenance.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SecondaryIndexValueError {
    /// Equality indexing does not define a canonical row for this value type.
    #[error("unsupported equality value type {value_type}")]
    UnsupportedEqualityValue {
        /// Stable property-value variant name.
        value_type: &'static str,
    },
    /// Range indexing does not define an ordered domain for this value type.
    #[error("unsupported range value type {value_type}")]
    UnsupportedRangeValue {
        /// Stable property-value variant name.
        value_type: &'static str,
    },
    /// Runtime range bounds belong to incomparable domains.
    #[error("range bounds are not comparable: {lower_type} and {upper_type}")]
    NonComparableDynamicBounds {
        /// Stable lower-bound variant name.
        lower_type: &'static str,
        /// Stable upper-bound variant name.
        upper_type: &'static str,
    },
    /// NaN has no ordered range-index position.
    #[error("NaN is not supported by range indexes")]
    NaNRangeValue,
    /// A complete canonical secondary key would exceed the format limit.
    #[error("encoded secondary key is too large: {encoded_len} bytes exceeds {maximum}")]
    EncodedKeyTooLarge {
        /// Observed encoded length.
        encoded_len: usize,
        /// Maximum accepted encoded length.
        maximum: usize,
    },
}

/// Errors that can occur in Helix database operations
#[derive(Debug, thiserror::Error)]
pub enum HelixDbError {
    /// Error from the underlying SlateDB storage
    #[error("Storage error: {0}")]
    Storage(#[from] slatedb::Error),

    /// Error encoding/decoding graph data
    #[error("Encoding error: {0}")]
    Encoding(#[from] EncodingError),

    /// Transaction conflict during commit
    #[error("Transaction conflict: {0}")]
    TransactionConflict(String),

    /// A standalone reader advanced while one request was executing.
    #[error("Request read view changed during execution; retry the request")]
    RequestReadViewChanged,

    /// A transport-owned monotonic query deadline elapsed.
    #[error("Query execution deadline exceeded")]
    QueryDeadlineExceeded,

    /// Invalid node ID
    #[error("Invalid node ID: {0}")]
    InvalidNodeId(u64),

    /// Node not found
    #[error("Node not found: {0}")]
    NodeNotFound(u64),

    /// Edge not found
    #[error("Edge not found: {from} -> {to}")]
    EdgeNotFound {
        /// Source node ID
        from: u64,
        /// Target node ID
        to: u64,
    },

    /// Database is closed
    #[error("Database is closed")]
    DatabaseClosed,

    /// Configuration error
    #[error("Configuration error: {0}")]
    Config(String),

    /// Dynamic-index work is disabled until its canonical V2 contract exists.
    #[error("Index lifecycle unavailable for {family}: {reason}")]
    IndexLifecycleUnavailable {
        /// Family whose authority cannot be proven.
        family: IndexFamily,
        /// Missing contract that requires fail-closed behavior.
        reason: IndexLifecycleUnavailableReason,
    },

    /// Explicit secondary stepping is only valid under Disabled scheduling.
    #[error("Explicit secondary lifecycle stepping requires Disabled worker mode")]
    SecondaryLifecycleSteppingRequiresDisabledMode,

    /// A request-owned Active text mutation exceeded exact serialized admission.
    #[error("Active text mutation exceeds {resource}: observed {observed}, limit {limit}")]
    ActiveTextMutationLimitExceeded {
        /// Resource rejected before intent creation or object I/O.
        resource: ActiveTextMutationResource,
        /// Exact serialized or operation count measured by preflight.
        observed: u64,
        /// Positive configured ceiling.
        limit: u64,
    },

    /// Present graph data cannot satisfy an index source contract.
    #[error("Invalid index source data: {reason}")]
    InvalidIndexSourceData {
        /// Stable human-readable source validation failure.
        reason: String,
    },

    /// A value could not satisfy the canonical V2 index model.
    #[error("Invalid V2 index model: {0}")]
    InvalidIndexV2Model(#[from] crate::index_v2::IndexV2ModelError),

    /// A graph or query value cannot satisfy the secondary-index contract.
    #[error("Invalid secondary index value: {0}")]
    SecondaryIndexValue(#[from] SecondaryIndexValueError),

    /// Existing storage needs an explicit external migration before V2 opens.
    #[error("Migration required: {reason}")]
    MigrationRequired {
        /// Stable reason the runtime refused to initialize or interpret rows.
        reason: String,
    },

    /// Storage is valid and can be resumed only through the writer-open path.
    #[error("Writer migration required: {requirement}")]
    WriterMigrationRequired {
        /// Exact writer-owned work required before readers may open.
        requirement: WriterMigrationRequirement,
    },

    /// Storage was written by a newer index format than this binary supports.
    #[error("Unsupported index storage version {found}; this binary supports {supported}")]
    UnsupportedIndexStorageVersion {
        /// Durable format encountered at open.
        found: u16,
        /// Current format supported by this binary.
        supported: u16,
    },

    /// A bounded durable numeric namespace has no allocatable IDs remaining.
    #[error("Identifier exhausted: {0}")]
    IdentifierExhausted(&'static str),

    /// Bounded random-ID collision retries could not find an unused identity.
    #[error("Identifier allocation failed for {kind} after {attempts} attempts")]
    IdentifierAllocationFailed {
        /// Durable identity namespace being allocated.
        kind: &'static str,
        /// Checked maximum candidates considered.
        attempts: usize,
    },

    /// Canonical rows could not form one trustworthy scoped runtime catalog.
    #[error("V2 index catalog corruption: {0}")]
    IndexCatalogCorruption(String),

    /// A retained active handle no longer names the canonical active revision.
    #[error(
        "stale index generation: index {index_id}, generation {generation}, revision {record_revision}"
    )]
    StaleIndexGeneration {
        /// Logical index ID retained by the caller.
        index_id: u64,
        /// Physical generation retained by the caller.
        generation: u64,
        /// Canonical record revision retained by the caller.
        record_revision: u64,
    },

    /// A newer writer fenced the request before proof absence became authoritative.
    #[error("writer fencing prevented the Active text commit outcome from being proven")]
    WriterFencedCommitOutcomeUnknown,

    /// Invalid vector index configuration.
    #[error("Invalid vector configuration: {0}")]
    InvalidVectorConfig(#[from] VectorConfigError),

    /// Stored vector row bytes do not satisfy their validated index contract.
    #[error("Invalid vector item: {0}")]
    InvalidVectorItem(#[from] VectorItemDecodeError),

    /// Object store error
    #[error("Object store error: {0}")]
    ObjectStore(#[from] slatedb::object_store::Error),

    /// Query/traversal error
    #[error("Query error: {0}")]
    Query(String),

    /// Operation requires a writer handle.
    #[error("writer mode required, current mode is {actual}")]
    WriterModeRequired {
        /// Current database mode.
        actual: &'static str,
    },

    /// Operation requires a standalone reader handle.
    #[error("reader mode required, current mode is {actual}")]
    ReaderModeRequired {
        /// Current database mode.
        actual: &'static str,
    },

    /// Vector index already exists
    #[error("Vector index already exists: {0}")]
    IndexAlreadyExists(String),

    /// A create request reused a logical index identity with different semantics.
    #[error("Index definition conflicts in fields: {differing_fields}")]
    IndexDefinitionConflict {
        /// Authoritative definition already owning the logical identity.
        existing: Box<crate::index_v2::ValidatedDynamicIndexDefinition>,
        /// Validated definition requested by the caller.
        requested: Box<crate::index_v2::ValidatedDynamicIndexDefinition>,
        /// Canonical, non-empty set of incompatible fields.
        differing_fields: crate::config::NonEmptyDefinitionDifferences,
    },

    /// The logical identity is already changing lifecycle state.
    #[error("Index is busy in lifecycle state {state}")]
    IndexBusy {
        /// Canonical state that prevents this request.
        state: &'static str,
    },

    /// No retained operation with this ID exists in the requested scope.
    #[error("Index operation not found: {operation_id}")]
    IndexOperationNotFound {
        /// Canonical lowercase UUID supplied by the caller.
        operation_id: String,
    },

    /// The retained operation cannot be converted into build-abort cleanup.
    #[error("Index operation {operation_id} is not abortable: {reason}")]
    IndexOperationNotAbortable {
        /// Canonical lowercase UUID supplied by the caller.
        operation_id: String,
        /// Stable diagnostic reason.
        reason: &'static str,
    },

    /// Requested logical or physical index does not exist.
    #[error("index_not_found: {0}")]
    IndexNotFound(String),

    /// Unique node equality constraint violation.
    #[error(
        "Unique constraint violated for {label}.{property} on value {value}: existing node {existing_node_id}, attempted node {attempted_node_id}"
    )]
    UniqueConstraintViolation {
        /// Label scope for the unique index.
        label: String,
        /// Indexed property name.
        property: String,
        /// Conflicting value.
        value: String,
        /// Existing node already owning the value.
        existing_node_id: u64,
        /// Node attempting to claim the value.
        attempted_node_id: u64,
    },

    /// Unsupported property type for unique node equality enforcement.
    #[error(
        "Unique node equality index {label}.{property} does not support value type {value_type} on node {node_id}"
    )]
    UnsupportedUniqueIndexValueType {
        /// Label scope for the unique index.
        label: String,
        /// Indexed property name.
        property: String,
        /// Node carrying the unsupported value.
        node_id: u64,
        /// Unsupported property value type.
        value_type: String,
    },

    /// Invalid vector dimension
    #[error("Invalid vector dimension: expected {expected}, got {got}")]
    InvalidDimension {
        /// Expected dimension
        expected: usize,
        /// Actual dimension
        got: usize,
    },

    /// A vector component was NaN or infinite at a public boundary.
    #[error("Invalid vector component at index {index}: value must be finite")]
    InvalidVectorComponent {
        /// Zero-based component offset.
        index: usize,
    },

    /// A finite vector component exceeded its metric/dimension score-safe domain.
    #[error(
        "{metric:?} vector dimension {dimension} component {component_index} magnitude {observed_magnitude} exceeds inclusive maximum {inclusive_maximum}"
    )]
    VectorComponentMagnitudeExceeded {
        /// Bound distance metric.
        metric: VectorDistanceMetric,
        /// Authoritative component count.
        dimension: usize,
        /// Zero-based component offset.
        component_index: usize,
        /// Absolute observed component value.
        observed_magnitude: f32,
        /// Inclusive accepted maximum.
        inclusive_maximum: f32,
    },

    /// Cosine distance is undefined for a true zero vector.
    #[error("Invalid cosine vector: norm must be non-zero")]
    ZeroNormCosineVector,

    /// A legacy cosine payload cannot be materialized into authoritative graph state.
    #[error(
        "legacy cosine vector index {element_kind:?} {label}.{property} contains a zero-norm vector for entity {entity_id}"
    )]
    LegacyZeroNormCosineVector {
        /// Node or edge namespace containing the entity.
        element_kind: crate::index_v2::IndexElementKind,
        /// Label component of the exact legacy definition.
        label: String,
        /// Property component of the exact legacy definition.
        property: String,
        /// Entity whose HNSW payload is incompatible with V2 cosine semantics.
        entity_id: u64,
    },

    /// Internal storage invariant violation.
    #[error("Storage invariant violated: {0}")]
    InvariantViolation(String),
}

impl From<ConfigError> for HelixDbError {
    fn from(value: ConfigError) -> Self {
        Self::Config(value.to_string())
    }
}

impl HelixDbError {
    /// Stable public compatibility code when this error belongs to the index
    /// lifecycle API.
    pub fn index_error_code(&self) -> Option<&'static str> {
        match self {
            Self::IndexLifecycleUnavailable { .. } => Some("index_lifecycle_unavailable"),
            Self::SecondaryLifecycleSteppingRequiresDisabledMode => {
                Some("secondary_lifecycle_stepping_requires_disabled_mode")
            }
            Self::ActiveTextMutationLimitExceeded { .. } => {
                Some("active_text_mutation_limit_exceeded")
            }
            Self::InvalidIndexSourceData { .. } => Some("invalid_index_source_data"),
            Self::IndexAlreadyExists(_) => Some("index_already_exists"),
            Self::IndexDefinitionConflict { .. } => Some("index_definition_conflict"),
            Self::IndexBusy { .. } => Some("index_busy"),
            Self::IndexNotFound(_) => Some("index_not_found"),
            Self::IndexOperationNotFound { .. } => Some("index_operation_not_found"),
            Self::IndexOperationNotAbortable { .. } => Some("index_operation_not_abortable"),
            Self::IdentifierExhausted("logical index ID") => Some("index_id_exhausted"),
            Self::IdentifierExhausted("vector physical index ID") => {
                Some("vector_physical_id_exhausted")
            }
            Self::InvalidIndexV2Model(
                crate::index_v2::IndexV2ModelError::IdentifierExhausted {
                    kind: "index generation ID",
                },
            ) => Some("index_generation_exhausted"),
            Self::InvalidIndexV2Model(
                crate::index_v2::IndexV2ModelError::IdentifierExhausted {
                    kind: "index revision",
                },
            ) => Some("index_revision_exhausted"),
            Self::InvalidIndexV2Model(
                crate::index_v2::IndexV2ModelError::IdentifierExhausted {
                    kind: "index operation revision",
                },
            ) => Some("index_operation_revision_exhausted"),
            Self::StaleIndexGeneration { .. } => Some("stale_index_generation"),
            Self::WriterFencedCommitOutcomeUnknown => Some("writer_fenced_commit_outcome_unknown"),
            Self::Storage(_)
            | Self::Encoding(_)
            | Self::TransactionConflict(_)
            | Self::RequestReadViewChanged
            | Self::InvalidNodeId(_)
            | Self::NodeNotFound(_)
            | Self::EdgeNotFound { .. }
            | Self::DatabaseClosed
            | Self::Config(_)
            | Self::InvalidIndexV2Model(_)
            | Self::SecondaryIndexValue(_)
            | Self::MigrationRequired { .. }
            | Self::WriterMigrationRequired { .. }
            | Self::UnsupportedIndexStorageVersion { .. }
            | Self::IdentifierExhausted(_)
            | Self::IdentifierAllocationFailed { .. }
            | Self::IndexCatalogCorruption(_)
            | Self::InvalidVectorConfig(_)
            | Self::InvalidVectorItem(_)
            | Self::ObjectStore(_)
            | Self::Query(_)
            | Self::QueryDeadlineExceeded
            | Self::WriterModeRequired { .. }
            | Self::ReaderModeRequired { .. }
            | Self::UniqueConstraintViolation { .. }
            | Self::UnsupportedUniqueIndexValueType { .. }
            | Self::InvalidDimension { .. }
            | Self::InvalidVectorComponent { .. }
            | Self::VectorComponentMagnitudeExceeded { .. }
            | Self::ZeroNormCosineVector
            | Self::LegacyZeroNormCosineVector { .. }
            | Self::InvariantViolation(_) => None,
        }
    }

    /// Returns true when the error represents a retryable transaction conflict.
    #[must_use]
    pub fn is_transaction_conflict(&self) -> bool {
        matches!(
            self,
            Self::TransactionConflict(_) | Self::WriterFencedCommitOutcomeUnknown
        ) || matches!(self, Self::Storage(storage_err) if storage_err.kind() == ErrorKind::Transaction)
    }

    /// Returns true when a public vector failed caller-controlled validation.
    ///
    /// Invalid persisted vector rows use [`Self::InvalidVectorItem`] or a
    /// legacy-specific error and deliberately remain internal failures.
    #[must_use]
    pub fn is_invalid_vector_input(&self) -> bool {
        matches!(
            self,
            Self::InvalidDimension { .. }
                | Self::InvalidVectorComponent { .. }
                | Self::VectorComponentMagnitudeExceeded { .. }
                | Self::ZeroNormCosineVector
        )
    }
}

impl From<VectorValidationError> for HelixDbError {
    fn from(error: VectorValidationError) -> Self {
        match error {
            VectorValidationError::DimensionMismatch { expected, actual } => {
                Self::InvalidDimension {
                    expected,
                    got: actual,
                }
            }
            VectorValidationError::NonFiniteComponent { index } => {
                Self::InvalidVectorComponent { index }
            }
            VectorValidationError::ZeroNormCosineVector => Self::ZeroNormCosineVector,
            VectorValidationError::ComponentMagnitudeExceeded {
                metric,
                dimension,
                component_index,
                observed_magnitude,
                inclusive_maximum,
            } => Self::VectorComponentMagnitudeExceeded {
                metric,
                dimension,
                component_index,
                observed_magnitude,
                inclusive_maximum,
            },
            VectorValidationError::MagnitudeDomain(error) => {
                Self::InvariantViolation(error.to_string())
            }
        }
    }
}

/// Result type alias for Helix operations
pub type Result<T> = std::result::Result<T, HelixDbError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_errors_convert_to_database_errors_with_display_context() {
        let error = HelixDbError::from(ConfigError::new("bad config"));
        assert_eq!(error.to_string(), "Configuration error: bad config");
    }

    #[test]
    fn retryable_conflict_classification_is_explicit() {
        assert!(HelixDbError::TransactionConflict("retry".to_string()).is_transaction_conflict());
        assert!(HelixDbError::WriterFencedCommitOutcomeUnknown.is_transaction_conflict());
        assert!(
            HelixDbError::Storage(slatedb::Error::transaction("retry".to_string()))
                .is_transaction_conflict()
        );
        assert!(
            !HelixDbError::Storage(slatedb::Error::invalid("not retryable".to_string()))
                .is_transaction_conflict()
        );
        assert!(!HelixDbError::Query("not retryable".to_string()).is_transaction_conflict());
    }

    #[test]
    fn invalid_vector_input_classification_excludes_physical_corruption() {
        let invalid_inputs = [
            HelixDbError::InvalidDimension {
                expected: 3,
                got: 2,
            },
            HelixDbError::InvalidVectorComponent { index: 1 },
            HelixDbError::VectorComponentMagnitudeExceeded {
                metric: VectorDistanceMetric::Euclidean,
                dimension: 3,
                component_index: 1,
                observed_magnitude: 4.0,
                inclusive_maximum: 3.0,
            },
            HelixDbError::ZeroNormCosineVector,
        ];
        assert!(invalid_inputs
            .iter()
            .all(HelixDbError::is_invalid_vector_input));

        assert!(
            !HelixDbError::InvalidVectorItem(VectorItemDecodeError::HeaderMismatch)
                .is_invalid_vector_input()
        );
        assert!(!HelixDbError::LegacyZeroNormCosineVector {
            element_kind: crate::index_v2::IndexElementKind::Node,
            label: "Document".to_string(),
            property: "embedding".to_string(),
            entity_id: 7,
        }
        .is_invalid_vector_input());
        assert!(
            !HelixDbError::InvariantViolation("stored row is corrupt".to_string())
                .is_invalid_vector_input()
        );
    }

    #[test]
    fn rich_error_variants_render_contract_fields() {
        assert_eq!(
            HelixDbError::UniqueConstraintViolation {
                label: "User".to_string(),
                property: "email".to_string(),
                value: "\"a@example.com\"".to_string(),
                existing_node_id: 1,
                attempted_node_id: 2,
            }
            .to_string(),
            "Unique constraint violated for User.email on value \"a@example.com\": existing node 1, attempted node 2"
        );
        assert_eq!(
            HelixDbError::UnsupportedUniqueIndexValueType {
                label: "User".to_string(),
                property: "email".to_string(),
                node_id: 3,
                value_type: "F64".to_string(),
            }
            .to_string(),
            "Unique node equality index User.email does not support value type F64 on node 3"
        );
        assert_eq!(
            HelixDbError::InvalidDimension {
                expected: 3,
                got: 2,
            }
            .to_string(),
            "Invalid vector dimension: expected 3, got 2"
        );
        assert_eq!(
            HelixDbError::IndexLifecycleUnavailable {
                family: IndexFamily::Vector,
                reason: IndexLifecycleUnavailableReason::CanonicalStateUnavailable,
            }
            .to_string(),
            "Index lifecycle unavailable for vector: canonical V2 state is not installed"
        );
        assert_eq!(
            HelixDbError::WriterMigrationRequired {
                requirement: WriterMigrationRequirement::StorageVersion {
                    found: 2,
                    target: 3,
                },
            }
            .to_string(),
            "Writer migration required: storage version 2 must be upgraded to 3"
        );
        assert_eq!(
            HelixDbError::WriterMigrationRequired {
                requirement: WriterMigrationRequirement::IncompleteStorageSchema,
            }
            .to_string(),
            "Writer migration required: storage schema migration is incomplete"
        );
    }

    #[test]
    fn lifecycle_errors_expose_frozen_machine_codes() {
        assert_eq!(
            HelixDbError::IndexBusy { state: "building" }.index_error_code(),
            Some("index_busy")
        );
        assert_eq!(
            HelixDbError::IndexOperationNotFound {
                operation_id: "018f0c58-6bc7-7c56-8d3d-9c5f18a0f001".to_string(),
            }
            .index_error_code(),
            Some("index_operation_not_found")
        );
        assert_eq!(
            HelixDbError::InvalidIndexV2Model(
                crate::index_v2::IndexV2ModelError::IdentifierExhausted {
                    kind: "index operation revision",
                },
            )
            .index_error_code(),
            Some("index_operation_revision_exhausted")
        );
        assert_eq!(HelixDbError::NodeNotFound(1).index_error_code(), None);
        assert_eq!(
            HelixDbError::WriterFencedCommitOutcomeUnknown.index_error_code(),
            Some("writer_fenced_commit_outcome_unknown")
        );
    }

    #[test]
    fn active_text_limit_resources_render_stable_fields_and_machine_code() {
        let resources = [
            (ActiveTextMutationResource::Entities, "entities"),
            (ActiveTextMutationResource::AnalysisBytes, "analysis_bytes"),
            (ActiveTextMutationResource::InputBytes, "input_bytes"),
            (
                ActiveTextMutationResource::OutputOperations,
                "output_operations",
            ),
            (ActiveTextMutationResource::OutputBytes, "output_bytes"),
            (ActiveTextMutationResource::SplitBytes, "split_bytes"),
            (
                ActiveTextMutationResource::RetainedSplitBytes,
                "retained_split_bytes",
            ),
            (
                ActiveTextMutationResource::ManifestPageBytes,
                "manifest_page_bytes",
            ),
        ];
        for (resource, expected) in resources {
            assert_eq!(resource.to_string(), expected);
        }

        let error = HelixDbError::ActiveTextMutationLimitExceeded {
            resource: ActiveTextMutationResource::OutputBytes,
            observed: 11,
            limit: 10,
        };
        assert_eq!(
            error.to_string(),
            "Active text mutation exceeds output_bytes: observed 11, limit 10"
        );
        assert_eq!(
            error.index_error_code(),
            Some("active_text_mutation_limit_exceeded")
        );
    }
}
