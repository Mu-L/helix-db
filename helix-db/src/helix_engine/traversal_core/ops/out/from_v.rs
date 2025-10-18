use crate::helix_engine::{
    storage_core::HelixGraphStorage,
    traversal_core::{traversal_iter::RoTraversalIterator, traversal_value::TraversalValue},
    types::{GraphError, VectorError},
};
use heed3::RoTxn;
use helix_macros::debug_trace;

pub struct FromVIterator<'db, 'arena, 'txn, I>
where
    'db: 'arena,
    'arena: 'txn,
{
    storage: &'db HelixGraphStorage,
    arena: &'arena bumpalo::Bump,
    txn: &'txn RoTxn<'db>,
    iter: I,
    get_vector_data: bool,
}

// implementing iterator for OutIterator
impl<'db, 'arena, 'txn, I> Iterator for FromVIterator<'db, 'arena, 'txn, I>
where
    I: Iterator<Item = Result<TraversalValue<'arena>, GraphError>>,
{
    type Item = Result<TraversalValue<'arena>, GraphError>;

    #[debug_trace("FROM_V")]
    fn next(&mut self) -> Option<Self::Item> {
        match self.iter.next() {
            Some(item) => match item {
                Ok(TraversalValue::Edge(item)) => {
                    let vector = if self.get_vector_data {
                        match self.storage.get_vector(self.txn, &item.from_node) {
                            Ok(vector) => TraversalValue::Vector(vector),
                            Err(e) => return Some(Err(e)),
                        }
                    } else {
                        match self.storage.get_vector_without_raw_vector_data_in(
                            self.txn,
                            &item.from_node,
                            self.arena,
                        ) {
                            Ok(Some(vector)) => TraversalValue::VectorNodeWithoutVectorData(vector),
                            Ok(None) => {
                                return Some(Err(GraphError::from(VectorError::VectorNotFound(
                                    item.from_node.to_string(),
                                ))));
                            }
                            Err(e) => return Some(Err(e)),
                        }
                    };

                    Some(Ok(vector))
                }
                _ => return None,
            },
            None => None,
        }
    }
}
pub trait FromVAdapter<'db, 'arena, 'txn, I>:
    Iterator<Item = Result<TraversalValue<'arena>, GraphError>>
{
    fn from_v(
        self,
        get_vector_data: bool,
    ) -> RoTraversalIterator<
        'db,
        'arena,
        'txn,
        impl Iterator<Item = Result<TraversalValue<'arena>, GraphError>>,
    >;
}

impl<'db, 'arena, 'txn, I: Iterator<Item = Result<TraversalValue<'arena>, GraphError>>>
    FromVAdapter<'db, 'arena, 'txn, I> for RoTraversalIterator<'db, 'arena, 'txn, I>
{
    #[inline(always)]
    fn from_v(
        self,
        get_vector_data: bool,
    ) -> RoTraversalIterator<
        'db,
        'arena,
        'txn,
        impl Iterator<Item = Result<TraversalValue<'arena>, GraphError>>,
    > {
        let iter = FromVIterator {
            storage: self.storage,
            arena: self.arena,
            txn: self.txn,
            iter: self.inner,
            get_vector_data: get_vector_data,
        };
        RoTraversalIterator {
            storage: self.storage,
            arena: self.arena,
            txn: self.txn,
            inner: iter,
        }
    }
}
