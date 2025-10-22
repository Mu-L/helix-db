use crate::{
    helix_engine::{
        storage_core::storage_methods::StorageMethods,
        traversal_core::{traversal_iter::RoTraversalIterator, traversal_value::TraversalValue},
        types::GraphError,
    },
    protocol::value::Value,
};
use serde::Serialize;

pub trait NFromIndexAdapter<'db, 'arena, 'txn, 's, K: Into<Value> + Serialize>:
    Iterator<Item = Result<TraversalValue<'arena>, GraphError>>
{
    /// Returns a new iterator that will return the node from the secondary index.
    ///
    /// # Arguments
    ///
    /// * `index` - The name of the secondary index.
    /// * `key` - The key to search for in the secondary index.
    ///
    /// Note that both the `index` and `key` must be provided.
    /// The index must be a valid and existing secondary index and the key should match the type of the index.
    fn n_from_index(
        self,
        label: &'s str,
        index: &'s str,
        key: &'s K,
    ) -> RoTraversalIterator<
        'db,
        'arena,
        'txn,
        impl Iterator<Item = Result<TraversalValue<'arena>, GraphError>>,
    >
    where
        K: Into<Value> + Serialize + Clone;
}

impl<
    'db,
    'arena,
    'txn,
    's,
    K: Into<Value> + Serialize,
    I: Iterator<Item = Result<TraversalValue<'arena>, GraphError>>,
> NFromIndexAdapter<'db, 'arena, 'txn, 's, K> for RoTraversalIterator<'db, 'arena, 'txn, I>
{
    #[inline]
    fn n_from_index(
        self,
        label: &'s str,
        index: &'s str,
        key: &K,
    ) -> RoTraversalIterator<
        'db,
        'arena,
        'txn,
        impl Iterator<Item = Result<TraversalValue<'arena>, GraphError>>,
    >
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
            .prefix_iter(self.txn, &bincode::serialize(&Value::from(key)).unwrap())
            .unwrap()
            .filter_map(move |item| {
                if let Ok((_, value)) = item {
                    match self.storage.get_node(self.txn, &value, self.arena) {
                        Ok(node) => {
                            if node.label == label {
                                return Some(Ok(TraversalValue::Node(node)));
                            } else {
                                return None;
                            }
                        }
                        Err(e) => {
                            println!("{} Error getting node: {:?}", line!(), e);
                            return Some(Err(GraphError::ConversionError(e.to_string())));
                        }
                    }
                }
                return None;
            });

        RoTraversalIterator {
            storage: self.storage,
            arena: self.arena,
            txn: self.txn,
            inner: res,
        }
    }
}
