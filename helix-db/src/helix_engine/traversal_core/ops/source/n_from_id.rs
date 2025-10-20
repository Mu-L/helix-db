use crate::helix_engine::{
    storage_core::storage_methods::StorageMethods,
    traversal_core::{traversal_iter::RoTraversalIterator, traversal_value::TraversalValue},
    types::GraphError,
};

pub trait NFromIdAdapter<'arena>:
    Iterator<Item = Result<TraversalValue<'arena>, GraphError>>
{
    /// Returns an iterator containing the node with the given id.
    ///
    /// Note that the `id` cannot be empty and must be a valid, existing node id.
    fn n_from_id(
        self,
        id: &u128,
    ) -> impl Iterator<Item = Result<TraversalValue<'arena>, GraphError>>;
}

impl<'db, 'arena, 'txn, I: Iterator<Item = Result<TraversalValue<'arena>, GraphError>>>
    NFromIdAdapter<'arena> for RoTraversalIterator<'db, 'arena, 'txn, I>
{
    #[inline]
    fn n_from_id(
        self,
        id: &u128,
    ) -> impl Iterator<Item = Result<TraversalValue<'arena>, GraphError>> {
        let n_from_id = std::iter::once({
            match self.storage.get_node(self.txn, id) {
                Ok(node) => Ok(TraversalValue::Node(node)),
                Err(e) => Err(e),
            }
        });

        RoTraversalIterator {
            storage: self.storage,
            arena: self.arena,
            txn: self.txn,
            inner: n_from_id,
        }
    }
}
