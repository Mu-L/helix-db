//! Vector lifecycle partition-mapping keys. HNSW keys remain V1.

use crate::index_v2::{IndexGenerationId, IndexId};

use super::text::PartitionFingerprint;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct VectorPartitionMappingKey {
    pub(crate) index_id: IndexId,
    pub(crate) generation: IndexGenerationId,
    pub(crate) partition: PartitionFingerprint,
}
