use crate::helix_engine::{
    storage_core::{HelixGraphStorage, storage_methods::StorageMethods},
    traversal_core::{traversal_iter::RoTraversalIterator, traversal_value::TraversalValue},
    types::GraphError,
};
use heed3::RoTxn;

pub struct FromNIterator<'db, 'arena, 'txn, I>
where
    'db: 'arena,
    'arena: 'txn,
{
    storage: &'db HelixGraphStorage,
    arena: &'arena bumpalo::Bump,
    txn: &'txn RoTxn<'db>,
    iter: I,
}

impl<'db, 'arena, 'txn, I> Iterator for FromNIterator<'db, 'arena, 'txn, I>
where
    I: Iterator<Item = Result<TraversalValue<'arena>, GraphError>>,
{
    type Item = Result<TraversalValue<'arena>, GraphError>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.iter.next() {
            Some(item) => match item {
                Ok(TraversalValue::Edge(item)) => Some(Ok(TraversalValue::Node(
                    match self.storage.get_node(self.txn, &item.from_node) {
                        Ok(node) => node,
                        Err(e) => {
                            println!("Error getting node: {e:?}");
                            return Some(Err(e));
                        }
                    },
                ))),
                _ => return None,
            },
            None => None,
        }
    }
}

pub trait FromNAdapter<'db, 'arena, 'txn, I>:
    Iterator<Item = Result<TraversalValue<'arena>, GraphError>>
{
    /// Returns an iterator containing the nodes that the edges in `self.inner` originate from.
    fn from_n(
        self,
    ) -> RoTraversalIterator<
        'db,
        'arena,
        'txn,
        impl Iterator<Item = Result<TraversalValue<'arena>, GraphError>>,
    >;
}

impl<'db, 'arena, 'txn, I: Iterator<Item = Result<TraversalValue<'arena>, GraphError>>>
    FromNAdapter<'db, 'arena, 'txn, I> for RoTraversalIterator<'db, 'arena, 'txn, I>
{
    #[inline(always)]
    fn from_n(
        self,
    ) -> RoTraversalIterator<
        'db,
        'arena,
        'txn,
        impl Iterator<Item = Result<TraversalValue<'arena>, GraphError>>,
    > {
        let iter = FromNIterator {
            storage: self.storage,
            arena: self.arena,
            txn: self.txn,
            iter: self.inner,
        };
        RoTraversalIterator {
            storage: self.storage,
            arena: self.arena,
            txn: self.txn,
            inner: iter,
        }
    }
}
