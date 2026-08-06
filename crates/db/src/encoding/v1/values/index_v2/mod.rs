//! Hand-written canonical V2 index value codecs.
//!
//! Values begin with `[version][kind]`. Existing values retain version `0x01`;
//! V3 split-bearing values use version `0x02` for pruning metadata. The codec
//! accepts only frozen discriminants and valid ADT combinations; it never uses
//! serde or a Rust-layout-dependent representation.

mod codec;
mod record;
mod work;

pub(crate) use record::{
    decode_index_record, decode_metadata_value, decode_operation_record,
    decode_operation_record_with_compatibility, encode_index_record, encode_metadata_value,
    encode_operation_record,
};
pub(crate) use work::{decode_work_value, encode_work_value, IndexV2WorkValue};

/// Frozen value framing version.
pub(crate) const INDEX_V2_VALUE_VERSION: u8 = 0x01;

/// Value framing version for V3 split-bearing rows.
pub(crate) const INDEX_V3_SPLIT_VALUE_VERSION: u8 = 0x02;
