use crate::helix_engine::{
    traversal_core::{
        traversal_iter::RoTraversalIterator,
        traversal_value::TraversalValue,
        traversal_value_arena::{RoArenaTraversalIterator, TraversalValueArena},
    },
    types::GraphError,
};
use heed3::RoTxn;
use helix_macros::debug_trace;

pub struct FilterRef<'a, I, F> {
    iter: I,
    txn: &'a RoTxn<'a>,
    f: F,
}

impl<'a, I, F> Iterator for FilterRef<'a, I, F>
where
    I: Iterator<Item = Result<TraversalValue, GraphError>>,
    F: Fn(&I::Item, &RoTxn) -> Result<bool, GraphError>,
{
    type Item = I::Item;
    #[debug_trace("FILTER_REF")]
    fn next(&mut self) -> Option<Self::Item> {
        for item in self.iter.by_ref() {
            match (self.f)(&item, self.txn) {
                Ok(result) => {
                    if result {
                        return Some(item);
                    }
                }
                Err(e) => {
                    return Some(Err(e));
                }
            }
        }
        None
    }
}

pub trait FilterRefAdapter<'a>: Iterator {
    /// FilterRef filters the iterator by taking a reference
    /// to each item and a transaction.
    fn filter_ref<F>(
        self,
        f: F,
    ) -> RoTraversalIterator<'a, impl Iterator<Item = Result<TraversalValue, GraphError>>>
    where
        F: Fn(&Result<TraversalValue, GraphError>, &RoTxn) -> Result<bool, GraphError>;
}

impl<'a, I: Iterator<Item = Result<TraversalValue, GraphError>>> FilterRefAdapter<'a>
    for RoTraversalIterator<'a, I>
{
    #[inline]
    fn filter_ref<F>(
        self,
        f: F,
    ) -> RoTraversalIterator<'a, impl Iterator<Item = Result<TraversalValue, GraphError>>>
    where
        F: Fn(&Result<TraversalValue, GraphError>, &RoTxn) -> Result<bool, GraphError>,
    {
        RoTraversalIterator {
            inner: FilterRef {
                iter: self.inner,
                txn: self.txn,
                f,
            },
            storage: self.storage,
            txn: self.txn,
        }
    }
}

pub struct FilterRefArena<'a, I, F> {
    iter: I,
    txn: &'a RoTxn<'a>,
    f: F,
}

impl<'a, I, F> Iterator for FilterRefArena<'a, I, F>
where
    I: Iterator<Item = Result<TraversalValueArena<'a>, GraphError>>,
    F: Fn(&I::Item, &RoTxn) -> Result<bool, GraphError>,
{
    type Item = I::Item;
    #[debug_trace("FILTER_REF")]
    fn next(&mut self) -> Option<Self::Item> {
        for item in self.iter.by_ref() {
            match (self.f)(&item, self.txn) {
                Ok(result) => {
                    if result {
                        return Some(item);
                    }
                }
                Err(e) => {
                    return Some(Err(e));
                }
            }
        }
        None
    }
}

pub trait FilterRefAdapterArena<'a, 'env>: Iterator {
    /// FilterRef filters the iterator by taking a reference
    /// to each item and a transaction.
    fn filter_ref<F>(
        self,
        f: F,
    ) -> RoArenaTraversalIterator<
        'a,
        'env,
        impl Iterator<Item = Result<TraversalValueArena<'a>, GraphError>>,
    >
    where
        F: Fn(&Result<TraversalValueArena<'a>, GraphError>, &RoTxn) -> Result<bool, GraphError>;
}

impl<'a, 'env, I: Iterator<Item = Result<TraversalValueArena<'a>, GraphError>>>
    FilterRefAdapterArena<'a, 'env> for RoArenaTraversalIterator<'a, 'env, I>
{
    #[inline]
    fn filter_ref<F>(
        self,
        f: F,
    ) -> RoArenaTraversalIterator<
        'a,
        'env,
        impl Iterator<Item = Result<TraversalValueArena<'a>, GraphError>>,
    >
    where
        F: Fn(&Result<TraversalValueArena<'a>, GraphError>, &RoTxn) -> Result<bool, GraphError>,
    {
        RoArenaTraversalIterator {
            inner: FilterRefArena {
                iter: self.inner,
                txn: self.txn,
                f,
            },
            storage: self.storage,
            arena: self.arena,
            txn: self.txn,
        }
    }
}
