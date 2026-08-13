//! Equality-index key codecs.

mod edge;
mod node;
mod scans;

pub(crate) use node::{
    EdgeDirection, EdgeEqualityIndexKey, EqualityIndexKey, GlobalEdgeEqualityIndexKey,
};
