//! Deprecated stored-value paths.

#[deprecated(note = "use encoding::v2::values::edge_endpoints")]
pub(crate) mod edge_endpoints {
    #[deprecated(note = "use encoding::v2::values::edge_endpoints")]
    pub(crate) use crate::encoding::v2::values::edge_endpoints::*;
}

#[deprecated(note = "use encoding::v2::values::adjacency")]
pub mod edges {
    #[deprecated(note = "use encoding::v2::values::adjacency")]
    pub use crate::encoding::v2::values::adjacency::*;
}

#[deprecated(note = "use encoding::v2::values::id_allocation")]
pub(crate) mod id_allocation {
    #[deprecated(note = "use encoding::v2::values::id_allocation")]
    pub(crate) use crate::encoding::v2::values::id_allocation::*;
}

#[deprecated(note = "use encoding::v2::values::indexes::equality")]
pub(crate) mod secondary {
    #[deprecated(note = "use encoding::v2::values::indexes::equality")]
    pub(crate) use crate::encoding::v2::values::indexes::SecondaryEqualityValue;
}

#[cfg(any(test, feature = "fuzzing", feature = "production-coverage"))]
#[deprecated(note = "use encoding::v2::values::indexes::text")]
pub(crate) mod text_index {
    #[deprecated(note = "use encoding::v2::values::indexes::text")]
    pub(crate) use crate::encoding::v2::values::indexes::text_legacy::*;
}

#[deprecated(note = "use encoding::v2::values::indexes::vector::generation")]
pub(crate) mod vector_generation {
    #[deprecated(note = "use encoding::v2::values::indexes::vector::generation")]
    pub(crate) use crate::encoding::v2::values::indexes::vector::*;
}

#[deprecated(note = "use encoding::v2::values::indexes::vector")]
pub mod vectors {
    #[deprecated(note = "use encoding::v2::values::indexes::vector")]
    pub use crate::encoding::v2::values::indexes::vector::{
        decode_layer0_neighbors, decode_layer0_neighbors_and_simhash, encode_layer0_neighbors,
        encode_layer0_record, ENCODING_TYPE_LAYER0_NEIGHBORS, ENCODING_TYPE_LAYER0_RECORD,
    };

    #[deprecated(note = "use encoding::v2::values::indexes::vector::entry_candidate")]
    pub mod entry {
        #[deprecated(note = "use encoding::v2::values::indexes::vector::entry_candidate")]
        pub use crate::encoding::v2::values::indexes::vector::entry_candidate::*;
    }
    #[deprecated(note = "use encoding::v2::values::indexes::vector::item")]
    pub(crate) mod item {
        #[deprecated(note = "use encoding::v2::values::indexes::vector::item")]
        pub(crate) use crate::encoding::v2::values::indexes::vector::item::*;
    }
    #[deprecated(note = "use encoding::v2::values::indexes::vector::markers")]
    pub(crate) mod markers {
        #[deprecated(note = "use encoding::v2::values::indexes::vector::markers")]
        pub(crate) use crate::encoding::v2::values::indexes::vector::markers::*;
    }
    #[deprecated(note = "use encoding::v2::values::indexes::vector::metadata")]
    pub(crate) mod metadata {
        #[deprecated(note = "use encoding::v2::values::indexes::vector::metadata")]
        pub(crate) use crate::encoding::v2::values::indexes::vector::metadata::*;
    }
    #[deprecated(note = "use encoding::v2::values::indexes::vector::neighbors")]
    pub mod neighbors {
        #[deprecated(note = "use encoding::v2::values::indexes::vector::neighbors")]
        pub use crate::encoding::v2::values::indexes::vector::neighbors::*;
    }
    #[deprecated(note = "use encoding::v2::values::indexes::vector::simhash")]
    pub(crate) mod simhash {
        #[deprecated(note = "use encoding::v2::values::indexes::vector::simhash")]
        pub(crate) use crate::encoding::v2::values::indexes::vector::simhash::*;
    }
}
