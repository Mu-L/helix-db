//! Deprecated graph-key paths.

#[deprecated(note = "use encoding::v2::keys::metadata")]
pub(crate) mod metadata {
    #[deprecated(note = "use encoding::v2::keys::metadata")]
    pub(crate) use crate::encoding::v2::keys::metadata::*;
}

#[deprecated(note = "use encoding::v2::keys::scope")]
pub mod tenant {
    #[deprecated(note = "use encoding::v2::keys::scope")]
    pub use crate::encoding::v2::keys::scope::*;
}

#[deprecated(note = "use encoding::v2::keys::indexes::vector")]
pub(crate) mod vectors {
    #[deprecated(note = "use encoding::v2::keys::indexes::vector")]
    pub(crate) use crate::encoding::v2::keys::indexes::vector::*;
}

#[deprecated(note = "use encoding::v2::keys")]
pub(crate) use crate::encoding::v2::keys::{
    AdjacencyKey, DataKey as Key, DataKeyKind, EdgeEndpointsKey, EdgePairIndexKey,
    EdgePropertyByIdKey, EdgePropertyPairKey, GlobalKeyKind, KeyPrefix, MetadataKey,
    NodePropertyKey, ID_LEN, PREFIX_LEN,
};
#[deprecated(note = "use encoding::v2::keys")]
pub use crate::encoding::v2::keys::{EdgeId, NodeId};
