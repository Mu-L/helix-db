//! Stored vector-index values grouped by physical row responsibility.

pub mod entry_candidate;
pub(crate) mod generation;
pub(crate) mod item;
pub(crate) mod layer0;
mod legacy_generation;
pub(crate) mod markers;
pub(crate) mod metadata;
pub mod neighbors;
pub(crate) mod simhash;

pub(crate) use generation::{decode_partition_mapping, encode_partition_mapping};
pub use layer0::{
    decode_layer0_neighbors, decode_layer0_neighbors_and_simhash, encode_layer0_neighbors,
    encode_layer0_record, ENCODING_TYPE_LAYER0_NEIGHBORS, ENCODING_TYPE_LAYER0_RECORD,
};
pub(crate) use legacy_generation::*;

pub(super) use super::super::codec_legacy::{
    checked_len_with_element_count, ensure_exact_len, ensure_min_len, take_u32_be, take_u64_le,
    take_u8, ENCODING_TYPE_LEN, U32_LEN, U64_LEN,
};
