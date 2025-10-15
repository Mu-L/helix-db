use crate::{
    helix_engine::{
        storage_core::{storage_core_arena::HelixGraphStorageArena, storage_methods::StorageMethods, HelixGraphStorage},
        traversal_core::{
            traversal_iter::RoTraversalIterator, traversal_value::TraversalValue,
            traversal_value_arena::{RoArenaTraversalIterator, TraversalValueArena},
        },
        types::GraphError,
    },
    utils::items::Node,
};
use heed3::RoTxn;
use helix_macros::debug_trace;
use std::{iter::Once, sync::Arc};

pub struct NFromId<'a, T> {
    iter: Once<Result<TraversalValue, GraphError>>,
    storage: Arc<HelixGraphStorage>,
    txn: &'a T,
    id: u128,
}

impl<'a> Iterator for NFromId<'a, RoTxn<'a>> {
    type Item = Result<TraversalValue, GraphError>;

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

pub trait NFromIdAdapter<'a>: Iterator<Item = Result<TraversalValue, GraphError>> {
    type OutputIter: Iterator<Item = Result<TraversalValue, GraphError>>;

    /// Returns an iterator containing the node with the given id.
    ///
    /// Note that the `id` cannot be empty and must be a valid, existing node id.
    fn n_from_id(self, id: &u128) -> Self::OutputIter;
}

impl<'a, I: Iterator<Item = Result<TraversalValue, GraphError>>> NFromIdAdapter<'a>
    for RoTraversalIterator<'a, I>
{
    type OutputIter = RoTraversalIterator<'a, NFromId<'a, RoTxn<'a>>>;

    #[inline]
    fn n_from_id(self, id: &u128) -> Self::OutputIter {
        let n_from_id = NFromId {
            iter: std::iter::once(Ok(TraversalValue::Empty)),
            storage: Arc::clone(&self.storage),
            txn: self.txn,
            id: *id,
        };

        RoTraversalIterator {
            inner: n_from_id,
            storage: self.storage,
            txn: self.txn,
        }
    }
}


pub trait NFromIdAdapterArena<'a>: Iterator<Item = Result<TraversalValueArena<'a>, GraphError>> {
    type OutputIter: Iterator<Item = Result<TraversalValueArena<'a>, GraphError>>;

    /// Returns an iterator containing the node with the given id.
    ///
    /// Note that the `id` cannot be empty and must be a valid, existing node id.
    fn n_from_id(self, id: &u128) -> Self::OutputIter;
}

pub struct NFromIdArena<'a, 'env, T> {
    iter: Once<Result<TraversalValueArena<'a>, GraphError>>,
    storage: &'env HelixGraphStorageArena,
    txn: &'a T,
    id: u128,
}

impl<'a, 'env> Iterator for NFromIdArena<'a, 'env, RoTxn<'a>> {
    type Item = Result<TraversalValueArena<'a>, GraphError>;

    #[debug_trace("N_FROM_ID")]
    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|_| {
            let node: Node = match self.storage.get_node(self.txn, &self.id) {
                Ok(node) => node,
                Err(e) => return Err(e),
            };
            Ok(TraversalValueArena::Node(node))
        })
    }
}

impl<'a, 'env, I: Iterator<Item = Result<TraversalValueArena<'a>, GraphError>>> NFromIdAdapterArena<'a>
    for RoArenaTraversalIterator<'a, 'env, I>
{
    type OutputIter = RoArenaTraversalIterator<'a, 'env, NFromIdArena<'a, 'env, RoTxn<'a>>>;

    #[inline]
    fn n_from_id(self, id: &u128) -> Self::OutputIter {
        let n_from_id = NFromIdArena {
            iter: std::iter::once(Ok(TraversalValueArena::Empty)),
            storage: self.storage,
            txn: self.txn,
            id: *id,
        };

        RoArenaTraversalIterator {
            inner: n_from_id,
            storage: self.storage,
            arena: self.arena,
            txn: self.txn,
        }
    }
}
