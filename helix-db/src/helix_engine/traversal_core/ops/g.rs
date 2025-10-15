use crate::helix_engine::{
    storage_core::{storage_core_arena::HelixGraphStorageArena, HelixGraphStorage},
    traversal_core::{
        traversal_iter::{RoTraversalIterator, RwTraversalIterator}, traversal_value::{IntoTraversalValues, TraversalValue}, traversal_value_arena::{RoArenaTraversalIterator, TraversalValueArena}
    },
    types::GraphError,
};
use heed3::{RoTxn, RwTxn};
use std::sync::Arc;

pub struct G {}

impl G {
    /// Starts a new empty traversal
    ///
    /// # Arguments
    ///
    /// * `storage` - An owned Arc of the storage for the traversal
    /// * `txn` - A reference to the transaction for the traversal
    ///
    /// # Example
    ///
    /// ```rust
    /// let storage = Arc::new(HelixGraphStorage::new());
    /// let txn = storage.graph_env.read_txn().unwrap();
    /// let traversal = G::new(storage, &txn);
    /// ```
    #[inline]
    pub fn new<'a>(
        storage: Arc<HelixGraphStorage>,
        txn: &'a RoTxn<'a>,
    ) -> RoTraversalIterator<'a, impl Iterator<Item = Result<TraversalValue, GraphError>>>
    where
        Self: Sized,
    {
        RoTraversalIterator {
            inner: std::iter::once(Ok(TraversalValue::Empty)),
            storage,
            txn,
        }
    }

    #[inline]
    pub fn new_with_arena<'a, 'env>(
        arena: &'a bumpalo::Bump,
        storage: &'env HelixGraphStorageArena,
        txn: &'a RoTxn<'a>,
    ) -> RoArenaTraversalIterator<
        'a,
        'env,
        impl Iterator<Item = Result<TraversalValueArena<'a>, GraphError>>,
    >
    where
        'env: 'a,
        Self: Sized,
    {
        RoArenaTraversalIterator {
            inner: std::iter::once(Ok(TraversalValueArena::Empty)),
            storage,
            arena,
            txn,
        }
    }

    /// Starts a new traversal from a vector of traversal values
    ///
    /// # Arguments
    ///
    /// * `storage` - An owned Arc of the storage for the traversal
    /// * `txn` - A reference to the transaction for the traversal
    /// * `items` - A vector of traversal values to start the traversal from
    ///
    /// # Example
    ///
    /// ```rust
    /// let storage = Arc::new(HelixGraphStorage::new());
    /// let txn = storage.graph_env.read_txn().unwrap();
    /// let traversal = G::new_from(storage, &txn, vec![TraversalValue::Node(Node { id: 1, label: "Person".to_string(), properties: None })]);
    /// ```
    pub fn new_from<'a, T: IntoTraversalValues>(
        storage: Arc<HelixGraphStorage>,
        txn: &'a RoTxn<'a>,
        items: T,
    ) -> RoTraversalIterator<'a, impl Iterator<Item = Result<TraversalValue, GraphError>>> {
        RoTraversalIterator {
            inner: items.into().into_iter().map(Ok),
            storage,
            txn,
        }
    }

    /// Starts a new mutable traversal
    ///
    /// # Arguments
    ///
    /// * `storage` - An owned Arc of the storage for the traversal
    /// * `txn` - A reference to the transaction for the traversal
    /// * `items` - A vector of traversal values to start the traversal from
    ///
    /// # Example
    ///
    /// ```rust
    /// let storage = Arc::new(HelixGraphStorage::new());
    /// let txn = storage.graph_env.write_txn().unwrap();
    /// let traversal = G::new_mut(storage, &mut txn);
    /// ```
    pub fn new_mut<'scope, 'env, 'a>(
        storage: Arc<HelixGraphStorage>,
        txn: &'scope mut RwTxn<'env>,
    ) -> RwTraversalIterator<
        'scope,
        'env,
        impl Iterator<Item = Result<TraversalValue, GraphError>>,
    >
    where
        Self: Sized,
    {
        RwTraversalIterator {
            inner: std::iter::once(Ok(TraversalValue::Empty)),
            storage,
            txn,
        }
    }

    /// Starts a new mutable traversal from a vector of traversal values
    ///
    /// # Arguments
    ///
    /// * `storage` - An owned Arc of the storage for the traversal
    /// * `txn` - A reference to the transaction for the traversal
    /// * `items` - A vector of traversal values to start the traversal from
    ///
    /// # Example
    ///
    /// ```rust
    /// let storage = Arc::new(HelixGraphStorage::new());
    /// let txn = storage.graph_env.write_txn().unwrap();
    /// let traversal = G::new_mut_from(storage, &mut txn, vec![TraversalValue::Node(Node { id: 1, label: "Person".to_string(), properties: None })]);
    /// ```
    pub fn new_mut_from<'a, 'scope, 'env, T: IntoTraversalValues>(
        storage: Arc<HelixGraphStorage>,
        txn: &'scope mut RwTxn<'env>,
        vals: T,
    ) -> RwTraversalIterator<
        'scope,
        'env,
        impl Iterator<Item = Result<TraversalValue, GraphError>>,
    > {
        RwTraversalIterator {
            inner: vals.into().into_iter().map(Ok),
            storage,
            txn,
        }
    }
}
