use crate::{
    helix_engine::{
        storage_core::{HelixGraphStorage, storage_methods::StorageMethods},
        traversal_core::{traversal_iter::RoTraversalIterator, traversal_value::TraversalValue},
        types::GraphError,
    },
    protocol::value::Value,
};
use heed3::{RoTxn, byteorder::BE};
use helix_macros::debug_trace;
use serde::Serialize;

pub struct NFromIndex<'db, 'arena, 'txn, 's>
where
    'db: 'arena,
    'arena: 'txn,
{
    storage: &'db HelixGraphStorage,
    arena: &'arena bumpalo::Bump,
    txn: &'txn RoTxn<'db>,
    iter: heed3::RoPrefix<
        'txn,
        heed3::types::Bytes,
        heed3::types::LazyDecode<heed3::types::U128<BE>>,
    >,
    label: &'s str,
}

impl<'db, 'arena, 'txn, 's> Iterator for NFromIndex<'db, 'arena, 'txn, 's> {
    type Item = Result<TraversalValue<'arena>, GraphError>;

    #[debug_trace("N_FROM_INDEX")]
    fn next(&mut self) -> Option<Self::Item> {
        for value in self.iter.by_ref() {
            let (_, value) = value.unwrap();
            match value.decode() {
                Ok(value) => match self.storage.get_node(self.txn, &value) {
                    Ok(node) => {
                        if node.label == self.label {
                            return Some(Ok(TraversalValue::Node(node)));
                        } else {
                            continue;
                        }
                    }
                    Err(e) => {
                        println!("{} Error getting node: {:?}", line!(), e);
                        return Some(Err(GraphError::ConversionError(e.to_string())));
                    }
                },

                Err(e) => return Some(Err(GraphError::ConversionError(e.to_string()))),
            }
        }
        None
    }
}

pub trait NFromIndexAdapter<'arena, 's, K: Into<Value> + Serialize>:
    Iterator<Item = Result<TraversalValue<'arena>, GraphError>>
{
    type OutputIter: Iterator<Item = Result<TraversalValue<'arena>, GraphError>>;

    /// Returns a new iterator that will return the node from the secondary index.
    ///
    /// # Arguments
    ///
    /// * `index` - The name of the secondary index.
    /// * `key` - The key to search for in the secondary index.
    ///
    /// Note that both the `index` and `key` must be provided.
    /// The index must be a valid and existing secondary index and the key should match the type of the index.
    fn n_from_index(self, label: &'s str, index: &'s str, key: &'s K) -> Self::OutputIter
    where
        K: Into<Value> + Serialize + Clone;
}

impl<
    'db,
    'arena,
    'txn,
    's,
    I: Iterator<Item = Result<TraversalValue<'arena>, GraphError>>,
    K: Into<Value> + Serialize,
> NFromIndexAdapter<'arena, 's, K> for RoTraversalIterator<'db, 'arena, 'txn, I>
{
    type OutputIter = RoTraversalIterator<'db, 'arena, 'txn, NFromIndex<'db, 'arena, 'txn, 's>>;

    #[inline]
    fn n_from_index(self, label: &'s str, index: &'s str, key: &K) -> Self::OutputIter
    where
        K: Into<Value> + Serialize + Clone,
    {
        let db = self
            .storage
            .secondary_indices
            .get(index)
            .ok_or(GraphError::New(format!(
                "Secondary Index {index} not found"
            )))
            .unwrap();
        let res = db
            .lazily_decode_data()
            .prefix_iter(self.txn, &bincode::serialize(&Value::from(key)).unwrap())
            .unwrap();

        let n_from_index = NFromIndex {
            storage: self.storage,
            arena: self.arena,
            txn: self.txn,
            iter: res,
            label,
        };

        RoTraversalIterator {
            storage: self.storage,
            arena: self.arena,
            txn: self.txn,
            inner: n_from_index,
        }
    }
}
