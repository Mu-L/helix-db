use crate::{
    helix_engine::{
        traversal_core::{traversal_value::TraversalValue, traversal_iter::RoTraversalIterator},
        storage_core::{HelixGraphStorage, storage_methods::StorageMethods},
        types::GraphError,
    },
    protocol::value::Value,
};
use heed3::{RoTxn, byteorder::BE};
use helix_macros::debug_trace;
use serde::Serialize;
use std::sync::Arc;

pub struct NFromIndex<'a> {
    iter:
        heed3::RoPrefix<'a, heed3::types::Bytes, heed3::types::LazyDecode<heed3::types::U128<BE>>>,
    txn: &'a RoTxn<'a>,
    storage: Arc<HelixGraphStorage>,
    label: &'a str,
}

impl<'a> Iterator for NFromIndex<'a> {
    type Item = Result<TraversalValue, GraphError>;

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

pub trait NFromIndexAdapter<'a, K: Into<Value> + Serialize>:
    Iterator<Item = Result<TraversalValue, GraphError>>
{
    type OutputIter: Iterator<Item = Result<TraversalValue, GraphError>>;

    /// Returns a new iterator that will return the node from the secondary index.
    ///
    /// # Arguments
    ///
    /// * `index` - The name of the secondary index.
    /// * `key` - The key to search for in the secondary index.
    ///
    /// Note that both the `index` and `key` must be provided.
    /// The index must be a valid and existing secondary index and the key should match the type of the index.
    fn n_from_index(self, label: &'a str, index: &'a str, key: &'a K) -> Self::OutputIter
    where
        K: Into<Value> + Serialize + Clone;
}

impl<'a, I: Iterator<Item = Result<TraversalValue, GraphError>>, K: Into<Value> + Serialize + 'a>
    NFromIndexAdapter<'a, K> for RoTraversalIterator<'a, I>
{
    type OutputIter = RoTraversalIterator<'a, NFromIndex<'a>>;

    #[inline]
    fn n_from_index(self, label: &'a str, index: &'a str, key: &'a K) -> Self::OutputIter
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
            iter: res,
            txn: self.txn,
            storage: Arc::clone(&self.storage),
            label,
        };

        RoTraversalIterator {
            inner: n_from_index,
            storage: self.storage,
            txn: self.txn,
        }
    }
}
