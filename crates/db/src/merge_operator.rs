//! SlateDB merge operator for Helix-owned keyspaces.

use std::io::Cursor;

use bytes::Bytes;
use roaring::RoaringTreemap;
use slatedb::{MergeOperator, MergeOperatorError};

use crate::encoding::keys::tenant::DataScope;
use crate::encoding::v1::keys::vectors::{KEY_KIND_LAYER0_VEC_KS, VECTOR_HOT_KEYSPACE_PREFIX};
use crate::encoding::v1::keys::{DataKeyKind, KeyPrefix};
use crate::encoding::v1::values::{edges, vectors};
use crate::encoding::NodeId;

const EDGE_DELTA_MIN_LEN: usize = core::mem::size_of::<u8>();
const EDGE_DELTA_NODE_LEN: usize = core::mem::size_of::<u8>() + core::mem::size_of::<NodeId>();
const LAYER0_SIMHASH_LEN: usize = core::mem::size_of::<u8>() + core::mem::size_of::<u64>();
const ADJACENCY_PREFIX: u8 = KeyPrefix::Adjacency.as_u8();
const PROPERTY_INDEX_PREFIX: u8 = KeyPrefix::PropertyIndex.as_u8();
const EDGE_PAIR_INDEX_PREFIX: u8 = KeyPrefix::EdgePairIndex.as_u8();
const METADATA_PREFIX: u8 = KeyPrefix::Metadata.as_u8();
const VECTOR_INDEX_ID_LEN: usize = core::mem::size_of::<u64>();
const VECTOR_KIND_OFFSET: usize = core::mem::size_of::<u8>() + VECTOR_INDEX_ID_LEN;
const VECTOR_LAYER0_KEY_LEN: usize = core::mem::size_of::<u8>()
    + VECTOR_INDEX_ID_LEN
    + core::mem::size_of::<u8>()
    + size_of::<NodeId>();
const BITMAP_DELTA_MAGIC: &[u8; 8] = b"HLXRBM1\0";
const BITMAP_DELTA_OP_LEN: usize = core::mem::size_of::<u8>();
const BITMAP_DELTA_ID_LEN: usize = core::mem::size_of::<u64>();
const BITMAP_DELTA_LEN: usize =
    BITMAP_DELTA_MAGIC.len() + BITMAP_DELTA_OP_LEN + BITMAP_DELTA_ID_LEN;
const BITMAP_ADD: u8 = 0x00;

pub(crate) fn encode_bitmap_add(id: u64) -> Bytes {
    let mut bytes = Vec::with_capacity(BITMAP_DELTA_LEN);
    bytes.extend_from_slice(BITMAP_DELTA_MAGIC);
    bytes.push(BITMAP_ADD);
    bytes.extend_from_slice(&id.to_be_bytes());
    Bytes::from(bytes)
}

fn decode_bitmap_add(bytes: &[u8]) -> Option<u64> {
    if bytes.len() != BITMAP_DELTA_LEN
        || &bytes[0..BITMAP_DELTA_MAGIC.len()] != BITMAP_DELTA_MAGIC
        || bytes[BITMAP_DELTA_MAGIC.len()] != BITMAP_ADD
    {
        return None;
    }
    Some(u64::from_be_bytes(
        bytes[BITMAP_DELTA_MAGIC.len() + BITMAP_DELTA_OP_LEN
            ..BITMAP_DELTA_MAGIC.len() + BITMAP_DELTA_OP_LEN + BITMAP_DELTA_ID_LEN]
            .try_into()
            .expect("validated bitmap delta has exactly eight id bytes"),
    ))
}

#[derive(Debug, Clone, Default)]
struct BitmapMergeOperator;

impl BitmapMergeOperator {
    fn apply(bitmap: &mut RoaringTreemap, bytes: &[u8]) -> Result<(), MergeOperatorError> {
        if let Some(id) = decode_bitmap_add(bytes) {
            bitmap.insert(id);
            return Ok(());
        }
        let decoded =
            RoaringTreemap::deserialize_from(Cursor::new(bytes)).map_err(merge_decode_error)?;
        *bitmap |= &decoded;
        Ok(())
    }

    fn encode(bitmap: &RoaringTreemap) -> Result<Bytes, MergeOperatorError> {
        let mut bytes = Vec::new();
        bitmap
            .serialize_into(&mut bytes)
            .map_err(merge_decode_error)?;
        Ok(Bytes::from(bytes))
    }
}

impl MergeOperator for BitmapMergeOperator {
    fn merge(
        &self,
        _key: &Bytes,
        existing_value: Option<Bytes>,
        operand: Bytes,
    ) -> Result<Bytes, MergeOperatorError> {
        let mut bitmap = RoaringTreemap::new();
        if let Some(existing) = existing_value {
            Self::apply(&mut bitmap, &existing)?;
        }
        if let Some(id) = decode_bitmap_add(&operand) {
            bitmap.insert(id);
        } else {
            Self::apply(&mut bitmap, &operand)?;
        }
        Self::encode(&bitmap)
    }

    fn merge_batch(
        &self,
        _key: &Bytes,
        existing_value: Option<Bytes>,
        operands: &[Bytes],
    ) -> Result<Bytes, MergeOperatorError> {
        let mut bitmap = RoaringTreemap::new();
        if let Some(existing) = existing_value {
            Self::apply(&mut bitmap, &existing)?;
        }
        for operand in operands.iter().rev() {
            Self::apply(&mut bitmap, operand)?;
        }
        Self::encode(&bitmap)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EdgeDeltaOp {
    AddOut,
    RemoveOut,
    AddIn,
    RemoveIn,
    ResetOut,
}

impl EdgeDeltaOp {
    const fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            0x00 => Some(Self::AddOut),
            0x01 => Some(Self::RemoveOut),
            0x02 => Some(Self::AddIn),
            0x03 => Some(Self::RemoveIn),
            0x04 => Some(Self::ResetOut),
            _ => None,
        }
    }
}

fn decode_edge_delta(data: &[u8]) -> Option<(EdgeDeltaOp, NodeId)> {
    if data.len() < EDGE_DELTA_MIN_LEN {
        return None;
    }
    let op = EdgeDeltaOp::from_byte(data[0])?;
    if op == EdgeDeltaOp::ResetOut {
        return Some((op, 0));
    }
    if data.len() < EDGE_DELTA_NODE_LEN {
        return None;
    }
    Some((
        op,
        NodeId::from_be_bytes(
            data[EDGE_DELTA_MIN_LEN..EDGE_DELTA_MIN_LEN + core::mem::size_of::<NodeId>()]
                .try_into()
                .expect("edge delta node id slice is 8 bytes"),
        ),
    ))
}

fn is_edge_delta(data: &[u8]) -> bool {
    data.first()
        .is_some_and(|byte| EdgeDeltaOp::from_byte(*byte).is_some())
}

#[derive(Debug, Clone, Default)]
struct EdgeMergeOperator;

impl EdgeMergeOperator {
    fn apply_delta(edges: &mut edges::Edges, op: EdgeDeltaOp, node_id: NodeId) {
        match op {
            EdgeDeltaOp::AddOut => edges.add_out(node_id),
            EdgeDeltaOp::RemoveOut => {
                edges.remove_out(node_id);
            }
            EdgeDeltaOp::AddIn => edges.add_in(node_id),
            EdgeDeltaOp::RemoveIn => {
                edges.remove_in(node_id);
            }
            EdgeDeltaOp::ResetOut => edges.nxts_out.clear(),
        }
    }

    fn apply_operand(edges: &mut edges::Edges, operand: &[u8]) {
        if let Some((op, node_id)) = decode_edge_delta(operand) {
            Self::apply_delta(edges, op, node_id);
            return;
        }
        if let Ok(other) = edges::decode_edges(operand) {
            edges.merge(&other);
        }
    }
}

impl MergeOperator for EdgeMergeOperator {
    fn merge(
        &self,
        _key: &Bytes,
        existing_value: Option<Bytes>,
        operand: Bytes,
    ) -> Result<Bytes, MergeOperatorError> {
        let mut merged = existing_value
            .as_deref()
            .filter(|value| !is_edge_delta(value))
            .map(edges::decode_edges)
            .transpose()
            .map_err(merge_decode_error)?
            .unwrap_or_default();
        if let Some(existing) = existing_value
            .as_deref()
            .filter(|value| is_edge_delta(value))
        {
            Self::apply_operand(&mut merged, existing);
        }
        Self::apply_operand(&mut merged, &operand);
        Ok(edges::encode_edges(&merged))
    }

    fn merge_batch(
        &self,
        _key: &Bytes,
        existing_value: Option<Bytes>,
        operands: &[Bytes],
    ) -> Result<Bytes, MergeOperatorError> {
        let mut merged = existing_value
            .as_deref()
            .filter(|value| !is_edge_delta(value))
            .map(edges::decode_edges)
            .transpose()
            .map_err(merge_decode_error)?
            .unwrap_or_default();
        if let Some(existing) = existing_value
            .as_deref()
            .filter(|value| is_edge_delta(value))
        {
            Self::apply_operand(&mut merged, existing);
        }
        for operand in operands.iter().rev() {
            Self::apply_operand(&mut merged, operand);
        }
        Ok(edges::encode_edges(&merged))
    }
}

const LAYER0_SIMHASH_SET: u8 = 0x05;
const LAYER0_SIMHASH_CLEAR: u8 = 0x06;

fn decode_layer0_simhash_operand(data: &[u8]) -> Option<Option<u64>> {
    match data.first().copied() {
        Some(LAYER0_SIMHASH_SET) if data.len() == LAYER0_SIMHASH_LEN => {
            Some(Some(u64::from_le_bytes(
                data[EDGE_DELTA_MIN_LEN..EDGE_DELTA_MIN_LEN + core::mem::size_of::<u64>()]
                    .try_into()
                    .expect("simhash operand slice is 8 bytes"),
            )))
        }
        Some(LAYER0_SIMHASH_CLEAR) if data.len() == EDGE_DELTA_MIN_LEN => Some(None),
        _ => None,
    }
}

#[derive(Debug, Clone, Default)]
struct Layer0State {
    neighbors: Vec<NodeId>,
    simhash_bits: Option<u64>,
}

#[derive(Debug, Clone, Default)]
struct Layer0NeighborMergeOperator;

impl Layer0NeighborMergeOperator {
    fn apply_delta(state: &mut Layer0State, op: EdgeDeltaOp, node_id: NodeId) {
        match op {
            EdgeDeltaOp::AddOut => {
                if let Err(index) = state.neighbors.binary_search(&node_id) {
                    state.neighbors.insert(index, node_id);
                }
            }
            EdgeDeltaOp::RemoveOut => {
                if let Ok(index) = state.neighbors.binary_search(&node_id) {
                    state.neighbors.remove(index);
                }
            }
            EdgeDeltaOp::ResetOut => state.neighbors.clear(),
            EdgeDeltaOp::AddIn | EdgeDeltaOp::RemoveIn => {}
        }
    }

    fn apply_operand(state: &mut Layer0State, operand: &[u8]) -> Result<(), MergeOperatorError> {
        if let Some(simhash_bits) = decode_layer0_simhash_operand(operand) {
            state.simhash_bits = simhash_bits;
            return Ok(());
        }
        if let Some((op, node_id)) = decode_edge_delta(operand) {
            Self::apply_delta(state, op, node_id);
            return Ok(());
        }

        let (neighbors, simhash_bits) =
            vectors::decode_layer0_neighbors_and_simhash(operand).map_err(merge_decode_error)?;
        state.neighbors = neighbors;
        if operand.first().copied() == Some(vectors::ENCODING_TYPE_LAYER0_RECORD) {
            state.simhash_bits = simhash_bits;
        }
        Ok(())
    }
}

impl MergeOperator for Layer0NeighborMergeOperator {
    fn merge(
        &self,
        _key: &Bytes,
        existing_value: Option<Bytes>,
        operand: Bytes,
    ) -> Result<Bytes, MergeOperatorError> {
        let mut state = Layer0State::default();
        if let Some(existing) = existing_value {
            Self::apply_operand(&mut state, &existing)?;
        }
        Self::apply_operand(&mut state, &operand)?;
        Ok(vectors::encode_layer0_record(
            &state.neighbors,
            state.simhash_bits,
        ))
    }

    fn merge_batch(
        &self,
        _key: &Bytes,
        existing_value: Option<Bytes>,
        operands: &[Bytes],
    ) -> Result<Bytes, MergeOperatorError> {
        let mut state = Layer0State::default();
        if let Some(existing) = existing_value {
            Self::apply_operand(&mut state, &existing)?;
        }
        for operand in operands.iter().rev() {
            Self::apply_operand(&mut state, operand)?;
        }
        Ok(vectors::encode_layer0_record(
            &state.neighbors,
            state.simhash_bits,
        ))
    }
}

#[derive(Debug, Clone, Default)]
struct CounterMergeOperator;

impl CounterMergeOperator {
    fn decode(data: &[u8]) -> i64 {
        data.get(0..core::mem::size_of::<i64>())
            .and_then(|bytes| bytes.try_into().ok())
            .map(i64::from_be_bytes)
            .unwrap_or_default()
    }

    fn encode(value: i64) -> Bytes {
        Bytes::copy_from_slice(&value.to_be_bytes())
    }
}

impl MergeOperator for CounterMergeOperator {
    fn merge(
        &self,
        _key: &Bytes,
        existing_value: Option<Bytes>,
        operand: Bytes,
    ) -> Result<Bytes, MergeOperatorError> {
        let existing = existing_value
            .as_deref()
            .map(Self::decode)
            .unwrap_or_default();
        let delta = Self::decode(&operand);
        Ok(Self::encode((existing + delta).max(0)))
    }

    fn merge_batch(
        &self,
        _key: &Bytes,
        existing_value: Option<Bytes>,
        operands: &[Bytes],
    ) -> Result<Bytes, MergeOperatorError> {
        let mut total = existing_value
            .as_deref()
            .map(Self::decode)
            .unwrap_or_default();
        for operand in operands {
            total += Self::decode(operand);
        }
        Ok(Self::encode(total.max(0)))
    }
}

/// Combined merge operator for durable Helix storage keyspaces.
#[derive(Debug, Clone, Default)]
pub(crate) struct HelixMergeOperator {
    edge: EdgeMergeOperator,
    bitmap: BitmapMergeOperator,
    layer0: Layer0NeighborMergeOperator,
    counter: CounterMergeOperator,
}

impl HelixMergeOperator {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    fn is_hnsw_layer0_key(key: &[u8]) -> bool {
        key.len() == VECTOR_LAYER0_KEY_LEN
            && key[0] == VECTOR_HOT_KEYSPACE_PREFIX
            && key[VECTOR_KIND_OFFSET] == KEY_KIND_LAYER0_VEC_KS
    }

    fn key_type(key: &[u8]) -> MergeKeyType {
        let logical = logical_key(key);
        if Self::is_hnsw_layer0_key(logical) {
            return MergeKeyType::Layer0;
        }
        match logical.first().copied() {
            Some(prefix) if prefix == KeyPrefix::Adjacency.as_u8() => MergeKeyType::Edge,
            Some(prefix) if prefix == KeyPrefix::EdgePairIndex.as_u8() => MergeKeyType::Bitmap,
            Some(prefix)
                if prefix == KeyPrefix::PropertyIndex.as_u8()
                    && matches!(
                        DataKeyKind::parse_from_slice(logical),
                        Ok(DataKeyKind::PropertyIndex(_))
                    ) =>
            {
                MergeKeyType::Bitmap
            }
            Some(prefix) if prefix == KeyPrefix::Metadata.as_u8() => MergeKeyType::Counter,
            _ => MergeKeyType::Other,
        }
    }
}

impl MergeOperator for HelixMergeOperator {
    fn merge(
        &self,
        key: &Bytes,
        existing_value: Option<Bytes>,
        operand: Bytes,
    ) -> Result<Bytes, MergeOperatorError> {
        match Self::key_type(key) {
            MergeKeyType::Edge => self.edge.merge(key, existing_value, operand),
            MergeKeyType::Bitmap => self.bitmap.merge(key, existing_value, operand),
            MergeKeyType::Layer0 => self.layer0.merge(key, existing_value, operand),
            MergeKeyType::Counter => self.counter.merge(key, existing_value, operand),
            MergeKeyType::Other => Ok(operand),
        }
    }

    fn merge_batch(
        &self,
        key: &Bytes,
        existing_value: Option<Bytes>,
        operands: &[Bytes],
    ) -> Result<Bytes, MergeOperatorError> {
        match Self::key_type(key) {
            MergeKeyType::Edge => self.edge.merge_batch(key, existing_value, operands),
            MergeKeyType::Bitmap => self.bitmap.merge_batch(key, existing_value, operands),
            MergeKeyType::Layer0 => self.layer0.merge_batch(key, existing_value, operands),
            MergeKeyType::Counter => self.counter.merge_batch(key, existing_value, operands),
            MergeKeyType::Other => operands
                .first()
                .cloned()
                .or(existing_value)
                .ok_or(MergeOperatorError::EmptyBatch),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MergeKeyType {
    Edge,
    Bitmap,
    Layer0,
    Counter,
    Other,
}

fn logical_key(key: &[u8]) -> &[u8] {
    if matches!(
        key.first().copied(),
        Some(METADATA_PREFIX | VECTOR_HOT_KEYSPACE_PREFIX)
    ) || key.len() == core::mem::size_of::<u8>() + core::mem::size_of::<NodeId>()
        && key.first().copied() == Some(ADJACENCY_PREFIX)
        || key.len()
            == core::mem::size_of::<u8>()
                + core::mem::size_of::<NodeId>()
                + core::mem::size_of::<NodeId>()
            && key.first().copied() == Some(EDGE_PAIR_INDEX_PREFIX)
        || key.first().copied() == Some(PROPERTY_INDEX_PREFIX)
            && matches!(
                DataKeyKind::parse_from_slice(key),
                Ok(DataKeyKind::PropertyIndex(_))
            )
    {
        return key;
    }

    if key.len() > DataScope::PREFIX_LEN {
        let tenant_logical = &key[DataScope::PREFIX_LEN..];
        if matches!(
            tenant_logical.first().copied(),
            Some(
                ADJACENCY_PREFIX
                    | METADATA_PREFIX
                    | VECTOR_HOT_KEYSPACE_PREFIX
                    | PROPERTY_INDEX_PREFIX
                    | EDGE_PAIR_INDEX_PREFIX
            )
        ) {
            return tenant_logical;
        }
    }
    key
}

fn merge_decode_error(error: impl std::fmt::Display) -> MergeOperatorError {
    MergeOperatorError::Callback {
        message: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encoding::v1::keys::{AdjacencyKey, DataKeyKind, EdgePairIndexKey, Key};

    fn edge_delta(op: u8, node_id: NodeId) -> Bytes {
        let mut bytes = vec![op];
        bytes.extend_from_slice(&node_id.to_be_bytes());
        Bytes::from(bytes)
    }

    #[test]
    fn edge_merge_batch_applies_oldest_to_newest() {
        let key = Key::Data {
            scope: crate::encoding::keys::tenant::DataScope::LegacyUnscoped,
            kind: DataKeyKind::Adjacency(AdjacencyKey::new(1)),
        }
        .to_bytes();
        let operator = HelixMergeOperator::new();
        let merged = operator
            .merge_batch(
                &key,
                None,
                &[
                    edge_delta(0x00, 7),
                    edge_delta(0x01, 5),
                    edge_delta(0x00, 5),
                ],
            )
            .expect("merge succeeds");
        let edges = edges::decode_edges(&merged).expect("edges decode");
        assert_eq!(edges.iter_out().collect::<Vec<_>>(), vec![7]);
    }

    #[test]
    fn bitmap_merge_preserves_base_and_deduplicates_additions() {
        let key = Key::Data {
            scope: DataScope::LegacyUnscoped,
            kind: DataKeyKind::EdgePairIndex(EdgePairIndexKey::new(1, 3)),
        }
        .to_bytes();
        let mut base = RoaringTreemap::new();
        base.insert(41);
        let base = BitmapMergeOperator::encode(&base).expect("base bitmap encodes");
        let operator = HelixMergeOperator::new();

        let merged = operator
            .merge_batch(
                &key,
                None,
                &[base, encode_bitmap_add(42), encode_bitmap_add(42)],
            )
            .expect("bitmap operands merge");
        let bitmap =
            RoaringTreemap::deserialize_from(Cursor::new(merged)).expect("merged bitmap decodes");

        assert_eq!(bitmap.iter().collect::<Vec<_>>(), vec![41, 42]);
    }

    #[test]
    fn layer0_merge_preserves_simhash_across_neighbor_delta() {
        let mut key = vec![VECTOR_HOT_KEYSPACE_PREFIX];
        key.extend_from_slice(&9_u64.to_be_bytes());
        key.push(KEY_KIND_LAYER0_VEC_KS);
        key.extend_from_slice(&3_u64.to_be_bytes());
        let operator = HelixMergeOperator::new();
        let mut simhash = vec![LAYER0_SIMHASH_SET];
        simhash.extend_from_slice(&0x0102_0304_0506_0708_u64.to_le_bytes());
        let merged = operator
            .merge_batch(
                &Bytes::from(key),
                None,
                &[edge_delta(0x00, 7), Bytes::from(simhash)],
            )
            .expect("merge succeeds");
        assert_eq!(
            vectors::decode_layer0_neighbors_and_simhash(&merged).expect("layer0 decodes"),
            (vec![7], Some(0x0102_0304_0506_0708))
        );
    }
}
