use crate::helix_engine::{
    storage_core::HelixGraphStorage,
    traversal_core::{traversal_iter::RoTraversalIterator, traversal_value::TraversalValue},
    types::GraphError,
};
use heed3::RoTxn;
use helix_macros::debug_trace;

pub struct ToVIterator<'db, 'arena, 'txn, I>
where
    'db: 'arena,
    'arena: 'txn,
{
    storage: &'db HelixGraphStorage,
    arena: &'arena bumpalo::Bump,
    txn: &'txn RoTxn<'db>,
    iter: I,
}

// implementing iterator for OutIterator
impl<'db, 'arena, 'txn, I> Iterator for ToVIterator<'db, 'arena, 'txn, I>
where
    I: Iterator<Item = Result<TraversalValue<'arena>, GraphError>>,
{
    type Item = Result<TraversalValue<'arena>, GraphError>;

    #[debug_trace("TO_V")]
    fn next(&mut self) -> Option<Self::Item> {
        match self.iter.next() {
            Some(item) => match item {
                Ok(TraversalValue::Edge(item)) => Some(Ok(TraversalValue::Vector(
                    match self.storage.get_vector(self.txn, &item.to_node) {
                        Ok(vector) => vector,
                        Err(e) => {
                            println!("Error getting vector: {e:?}");
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
pub trait ToVAdapter<'db, 'arena, 'txn, I>:
    Iterator<Item = Result<TraversalValue<'arena>, GraphError>>
{
    fn to_v(
        self,
    ) -> RoTraversalIterator<
        'db,
        'arena,
        'txn,
        impl Iterator<Item = Result<TraversalValue<'arena>, GraphError>>,
    >;
}

impl<'db, 'arena, 'txn, I: Iterator<Item = Result<TraversalValue<'arena>, GraphError>>>
    ToVAdapter<'db, 'arena, 'txn, I> for RoTraversalIterator<'db, 'arena, 'txn, I>
{
    #[inline(always)]
    fn to_v(
        self,
    ) -> RoTraversalIterator<
        'db,
        'arena,
        'txn,
        impl Iterator<Item = Result<TraversalValue<'arena>, GraphError>>,
    > {
        let iter = ToVIterator {
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
