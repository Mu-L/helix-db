//! Vector lifecycle partition-mapping keys. HNSW keys remain V1.

use crate::index_lifecycle::{IndexGenerationId, IndexId};

use super::super::text::PartitionFingerprint;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct VectorPartitionMappingKey {
    pub(crate) index_id: IndexId,
    pub(crate) generation: IndexGenerationId,
    pub(crate) partition: PartitionFingerprint,
}
