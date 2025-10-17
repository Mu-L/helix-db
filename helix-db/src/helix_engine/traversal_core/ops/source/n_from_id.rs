use crate::{
    helix_engine::{
        storage_core::{HelixGraphStorage, storage_methods::StorageMethods},
        traversal_core::{traversal_iter::RoTraversalIterator, traversal_value::TraversalValue},
        types::GraphError,
    },
    utils::items::Node,
};
use heed3::RoTxn;
use helix_macros::debug_trace;
use std::iter::Once;

pub struct NFromId<'db, 'arena, 'txn>
where
    'db: 'arena,
    'arena: 'txn,
{   
    storage: &'db HelixGraphStorage,
    arena: &'arena bumpalo::Bump,
    txn: &'txn RoTxn<'db>,
    iter: Once<Result<TraversalValue<'arena>, GraphError>>,
    id: u128,
}

impl<'db, 'arena, 'txn> Iterator for NFromId<'db, 'arena, 'txn> {
    type Item = Result<TraversalValue<'arena>, GraphError>;

    #[debug_trace("N_FROM_ID")]
    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|_| {
            let node: Node = match self.storage.get_node(self.txn, &self.id) {
                Ok(node) => node,
                Err(e) => return Err(e),
            };
            Ok(TraversalValue::Node(node))
        })
    }
}

pub trait NFromIdAdapter<'arena>:
    Iterator<Item = Result<TraversalValue<'arena>, GraphError>>
{
    type OutputIter: Iterator<Item = Result<TraversalValue<'arena>, GraphError>>;

    /// Returns an iterator containing the node with the given id.
    ///
    /// Note that the `id` cannot be empty and must be a valid, existing node id.
    fn n_from_id(self, id: &u128) -> Self::OutputIter;
}


impl<'db, 'arena, 'txn, I: Iterator<Item = Result<TraversalValue<'arena>, GraphError>>>
    NFromIdAdapter<'arena> for RoTraversalIterator<'db, 'arena, 'txn, I>
{
    type OutputIter = RoTraversalIterator<'db, 'arena, 'txn, NFromId<'db, 'arena, 'txn>>;

    #[inline]
    fn n_from_id(self, id: &u128) -> Self::OutputIter {
        let n_from_id = NFromId {
            storage: self.storage,
            arena: self.arena,
            txn: self.txn,
            iter: std::iter::once(Ok(TraversalValue::Empty)),
            id: *id,
        };

        RoTraversalIterator {
            storage: self.storage,
            arena: self.arena,
            txn: self.txn,
            inner: n_from_id,
        }
    }
}
