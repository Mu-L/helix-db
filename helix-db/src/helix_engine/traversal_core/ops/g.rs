use crate::helix_engine::{
    storage_core::HelixGraphStorage,
    traversal_core::{
        traversal_iter::{RoTraversalIterator, RwTraversalIterator},
        traversal_value::{IntoTraversalValues, TraversalValue, Variable},
    },
    types::GraphError,
};
use heed3::{RoTxn, RwTxn};
use std::{borrow::Cow, sync::Arc};

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
    pub fn new<'db, 'arena, 'txn>(
        storage: &'db HelixGraphStorage,
        txn: &'txn RoTxn<'db>,
        arena: &'arena bumpalo::Bump,
    ) -> RoTraversalIterator<
        'db,
        'arena,
        'txn,
        impl Iterator<Item = Result<Variable<'arena>, GraphError>>,
    >
    where
        Self: Sized,
    {
        RoTraversalIterator {
            storage,
            txn,
            arena,
            inner: std::iter::once(Ok(Cow::Owned(TraversalValue::Empty))),
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
    /// let traversal = G::from_iter(storage, &txn, vec![TraversalValue::Node(Node { id: 1, label: "Person".to_string(), properties: None })]);
    /// ```
    pub fn from_iter<'db, 'arena, 'txn>(
        storage: &'db HelixGraphStorage,
        txn: &'txn RoTxn<'db>,
        items: impl Iterator<Item = Cow<'arena, TraversalValue<'arena>>>,
        arena: &'arena bumpalo::Bump,
    ) -> RoTraversalIterator<
        'db,
        'arena,
        'txn,
        impl Iterator<Item = Result<Variable<'arena>, GraphError>>,
    > {
        RoTraversalIterator {
            inner: items.map(Ok),
            storage,
            txn,
            arena,
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
        impl Iterator<Item = Result<TraversalValue<'scope>, GraphError>>,
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
    pub fn new_mut_from<'a, 'scope, 'env, T: IntoTraversalValues<'scope>>(
        storage: Arc<HelixGraphStorage>,
        txn: &'scope mut RwTxn<'env>,
        vals: T,
    ) -> RwTraversalIterator<
        'scope,
        'env,
        impl Iterator<Item = Result<TraversalValue<'scope>, GraphError>>,
    > {
        RwTraversalIterator {
            inner: vals.into().into_iter().map(Ok),
            storage,
            txn,
        }
    }
}
