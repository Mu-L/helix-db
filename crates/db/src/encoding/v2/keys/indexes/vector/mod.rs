//! Vector-index key codecs grouped by physical row responsibility.

mod entry_candidates;
mod items;
mod layer0;
pub(crate) mod metadata;
mod reverse_edges;
mod simhash;
mod storage_prefixes;
mod transaction_guard;
mod upper_layers;

pub(crate) use metadata::VectorPartitionMappingKey;
pub(crate) use storage_prefixes::*;
