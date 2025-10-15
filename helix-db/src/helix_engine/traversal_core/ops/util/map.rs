use crate::helix_engine::{
    traversal_core::{
        traversal_iter::{RoTraversalIterator, RwTraversalIterator},
        traversal_value::TraversalValue,
        traversal_value_arena::{RoArenaTraversalIterator, TraversalValueArena},
    },
    types::GraphError,
};

use heed3::RoTxn;

pub struct Map<'a, I, F> {
    iter: I,
    txn: &'a RoTxn<'a>,
    f: F,
}

// implementing iterator for filter ref
impl<'a, I, F> Iterator for Map<'a, I, F>
where
    I: Iterator<Item = Result<TraversalValue, GraphError>>,
    F: FnMut(TraversalValue, &RoTxn<'a>) -> Result<TraversalValue, GraphError>,
{
    type Item = I::Item;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(item) = self.iter.by_ref().next() {
            return match item {
                Ok(item) => Some((self.f)(item, self.txn)),
                Err(e) => return Some(Err(e)),
            };
        }
        None
    }
}

pub trait MapAdapter<'a>: Iterator<Item = Result<TraversalValue, GraphError>> {
    /// MapTraversal maps the iterator by taking a reference
    /// to each item and a transaction.
    ///
    /// # Arguments
    ///
    /// * `f` - A function to map the iterator
    ///
    /// # Example
    ///
    /// ```rust
    /// let traversal = G::new(storage, &txn).map_traversal(|item, txn| {
    ///     Ok(item)
    /// });
    /// ```
    fn map_traversal<F>(
        self,
        f: F,
    ) -> RoTraversalIterator<'a, impl Iterator<Item = Result<TraversalValue, GraphError>>>
    where
        F: FnMut(TraversalValue, &RoTxn<'a>) -> Result<TraversalValue, GraphError>;
}

impl<'a, I: Iterator<Item = Result<TraversalValue, GraphError>>> MapAdapter<'a>
    for RoTraversalIterator<'a, I>
{
    #[inline]
    fn map_traversal<F>(
        self,
        f: F,
    ) -> RoTraversalIterator<'a, impl Iterator<Item = Result<TraversalValue, GraphError>>>
    where
        F: FnMut(TraversalValue, &RoTxn<'a>) -> Result<TraversalValue, GraphError>,
    {
        RoTraversalIterator {
            inner: Map {
                iter: self.inner,
                txn: self.txn,
                f,
            },
            storage: self.storage,
            txn: self.txn,
        }
    }
}

pub struct MapMut<I, F> {
    iter: I,
    f: F,
}
impl<I, F> Iterator for MapMut<I, F>
where
    I: Iterator<Item = Result<TraversalValue, GraphError>>,
    F: Fn(I::Item) -> Result<TraversalValue, GraphError>,
{
    type Item = I::Item;

    fn next(&mut self) -> Option<Self::Item> {
        for item in self.iter.by_ref() {
            if let Ok(item) = (self.f)(item) {
                return Some(Ok(item));
            }
        }
        None
    }
}
pub trait MapAdapterMut<'scope, 'env>: Iterator<Item = Result<TraversalValue, GraphError>> {
    /// MapTraversalMut maps the iterator by taking a mutable
    /// reference to each item and a transaction.
    ///
    /// # Arguments
    ///
    /// * `f` - A function to map the iterator
    fn map_traversal_mut<F>(
        self,
        f: F,
    ) -> RwTraversalIterator<'scope, 'env, impl Iterator<Item = Result<TraversalValue, GraphError>>>
    where
        F: Fn(Result<TraversalValue, GraphError>) -> Result<TraversalValue, GraphError>;
}

impl<'scope, 'env, I: Iterator<Item = Result<TraversalValue, GraphError>>>
    MapAdapterMut<'scope, 'env> for RwTraversalIterator<'scope, 'env, I>
{
    #[inline]
    fn map_traversal_mut<F>(
        self,
        f: F,
    ) -> RwTraversalIterator<'scope, 'env, impl Iterator<Item = Result<TraversalValue, GraphError>>>
    where
        F: Fn(I::Item) -> Result<TraversalValue, GraphError>,
    {
        RwTraversalIterator {
            inner: MapMut {
                iter: self.inner,
                f,
            },
            storage: self.storage,
            txn: self.txn,
        }
    }
}

pub struct MapArena<'a, I, F> {
    iter: I,
    txn: &'a RoTxn<'a>,
    f: F,
}

// implementing iterator for filter ref
impl<'a, I, F> Iterator for MapArena<'a, I, F>
where
    I: Iterator<Item = Result<TraversalValueArena<'a>, GraphError>>,
    F: FnMut(TraversalValueArena<'a>, &RoTxn<'a>) -> Result<TraversalValueArena<'a>, GraphError>,
{
    type Item = I::Item;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(item) = self.iter.by_ref().next() {
            return match item {
                Ok(item) => Some((self.f)(item, self.txn)),
                Err(e) => return Some(Err(e)),
            };
        }
        None
    }
}

pub trait MapAdapterArena<'a, 'env>:
    Iterator<Item = Result<TraversalValueArena<'a>, GraphError>>
{
    /// MapTraversal maps the iterator by taking a reference
    /// to each item and a transaction.
    ///
    /// # Arguments
    ///
    /// * `f` - A function to map the iterator
    ///
    /// # Example
    ///
    /// ```rust
    /// let traversal = G::new(storage, &txn).map_traversal(|item, txn| {
    ///     Ok(item)
    /// });
    /// ```
    fn map_traversal<F>(
        self,
        f: F,
    ) -> RoArenaTraversalIterator<
        'a,
        'env,
        impl Iterator<Item = Result<TraversalValueArena<'a>, GraphError>>,
    >
    where
        F: FnMut(
            TraversalValueArena<'a>,
            &RoTxn<'a>,
        ) -> Result<TraversalValueArena<'a>, GraphError>;
}

impl<'a, 'env, I: Iterator<Item = Result<TraversalValueArena<'a>, GraphError>>> MapAdapterArena<'a, 'env>
    for RoArenaTraversalIterator<'a, 'env, I>
{
    #[inline]
    fn map_traversal<F>(
        self,
        f: F,
    ) -> RoArenaTraversalIterator<
        'a,
        'env,
        impl Iterator<Item = Result<TraversalValueArena<'a>, GraphError>>,
    >
    where
        F: FnMut(
            TraversalValueArena<'a>,
            &RoTxn<'a>,
        ) -> Result<TraversalValueArena<'a>, GraphError>,
    {
        RoArenaTraversalIterator {
            inner: MapArena {
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
