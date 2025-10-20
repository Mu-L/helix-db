use crate::helix_engine::{
    storage_core::HelixGraphStorage,
    traversal_core::{traversal_iter::RoTraversalIterator, traversal_value::TraversalValue},
    types::{GraphError, VectorError},
    vector_core::{vector::HVector, vector_without_data::VectorWithoutData},
};
use heed3::RoTxn;
use std::iter::Once;

pub struct VFromId<'db, 'arena, 'txn>
where
    'db: 'arena,
    'arena: 'txn,
{
    storage: &'db HelixGraphStorage,
    arena: &'arena bumpalo::Bump,
    txn: &'txn RoTxn<'db>,
    iter: Once<Result<TraversalValue<'arena>, GraphError>>,
    id: u128,
    get_vector_data: bool,
}

impl<'db, 'arena, 'txn> Iterator for VFromId<'db, 'arena, 'txn> {
    type Item = Result<TraversalValue<'arena>, GraphError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|_| {
            if self.get_vector_data {
                let vec: HVector<'arena> =
                    match self.storage.get_full_vector(self.txn, &self.id, self.arena) {
                        Ok(vec) => vec,
                        Err(e) => return Err(e),
                    };
                Ok(TraversalValue::Vector(vec))
            } else {
                let vec: VectorWithoutData<'arena> = match self
                    .storage
                    .get(self.txn, &self.id, self.arena)
                {
                    Ok(Some(vec)) => vec,
                    Ok(None) => {
                        return Err(GraphError::from(VectorError::VectorNotFound(
                            self.id.to_string(),
                        )));
                    }
                    Err(e) => return Err(e),
                };
                Ok(TraversalValue::VectorNodeWithoutVectorData(vec))
            }
        })
    }
}

pub trait VFromIdAdapter<'arena>:
    Iterator<Item = Result<TraversalValue<'arena>, GraphError>>
{
    type OutputIter: Iterator<Item = Result<TraversalValue<'arena>, GraphError>>;

    /// Returns an iterator containing the vector with the given id.
    ///
    /// Note that the `id` cannot be empty and must be a valid, existing vector id.
    fn v_from_id(self, id: &u128, get_vector_data: bool) -> Self::OutputIter;
}

impl<'db, 'arena, 'txn, I: Iterator<Item = Result<TraversalValue<'arena>, GraphError>>>
    VFromIdAdapter<'arena> for RoTraversalIterator<'db, 'arena, 'txn, I>
{
    type OutputIter = RoTraversalIterator<'db, 'arena, 'txn, VFromId<'db, 'arena, 'txn>>;

    #[inline]
    fn v_from_id(self, id: &u128, get_vector_data: bool) -> Self::OutputIter {
        let v_from_id = VFromId {
            storage: self.storage,
            arena: self.arena,
            txn: self.txn,
            iter: std::iter::once(Ok(TraversalValue::Empty)),
            id: *id,
            get_vector_data,
        };

        RoTraversalIterator {
            storage: self.storage,
            arena: self.arena,
            txn: self.txn,
            inner: v_from_id,
        }
    }
}
