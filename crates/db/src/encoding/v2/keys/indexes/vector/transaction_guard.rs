//! Vector transaction-guard keys.

use super::*;

/// `[0x03][0x03][index_id:8][kind=txn_guard]`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VectorTxnGuardKey {
    index_id: u64,
}

impl VectorTxnGuardKey {
    pub(crate) const fn new(index_id: u64) -> Self {
        Self { index_id }
    }

    pub(crate) fn parse_from_slice(slice: &[u8]) -> Result<Self, EncodingError> {
        match slice.len().cmp(&DEFAULT_KEY_LEN) {
            core::cmp::Ordering::Less => {
                return Err(EncodingError::BufferTooShort {
                    expected: DEFAULT_KEY_LEN,
                    actual: slice.len(),
                });
            }
            core::cmp::Ordering::Equal => {}
            core::cmp::Ordering::Greater => {
                return Err(EncodingError::InvalidKey(format!(
                    "expected {DEFAULT_KEY_LEN} bytes, got {}",
                    slice.len()
                )));
            }
        }

        if slice[0] != KEY_SPACE_INDEX {
            return Err(EncodingError::InvalidKeyPrefix(slice[0]));
        }
        if slice[DEFAULT_INDEX_TYPE_OFFSET] != INDEX_TYPE_VECTOR {
            return Err(EncodingError::InvalidKey(format!(
                "expected vector index type ({INDEX_TYPE_VECTOR:#04x}), got {:#04x}",
                slice[DEFAULT_INDEX_TYPE_OFFSET]
            )));
        }
        if slice[DEFAULT_KIND_OFFSET] != KEY_KIND_TXN_GUARD {
            return Err(EncodingError::InvalidKey(format!(
                "expected vector txn guard key kind ({KEY_KIND_TXN_GUARD:#04x}), got {:#04x}",
                slice[DEFAULT_KIND_OFFSET]
            )));
        }

        Ok(Self::new(u64::from_be_bytes(
            slice[DEFAULT_INDEX_ID_OFFSET..DEFAULT_INDEX_ID_OFFSET + INDEX_ID_LEN]
                .try_into()
                .expect("index id slice is 8 bytes"),
        )))
    }

    pub(crate) const fn index_id(&self) -> u64 {
        self.index_id
    }

    pub(crate) const fn encoded_len(&self) -> usize {
        DEFAULT_KEY_LEN
    }

    pub(crate) fn encode_into<B: BufMut>(&self, buf: &mut B) {
        buf.put_u8(KEY_SPACE_INDEX);
        buf.put_u8(INDEX_TYPE_VECTOR);
        buf.put_u64(self.index_id);
        buf.put_u8(KEY_KIND_TXN_GUARD);
    }
}
