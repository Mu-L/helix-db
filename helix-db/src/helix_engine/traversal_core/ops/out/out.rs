use crate::{
    helix_engine::{
        storage_core::{storage_core_arena::HelixGraphStorageArena, storage_methods::StorageMethods, HelixGraphStorage},
        traversal_core::{
            ops::source::add_e::EdgeType,
            traversal_iter::RoTraversalIterator,
            traversal_value::{Traversable, TraversalValue},
            traversal_value_arena::{
                RoArenaTraversalIterator, Traversable as TraversableArena, TraversalValueArena,
            },
        },
        types::GraphError,
    },
    utils::label_hash::hash_label,
};
use heed3::{RoTxn, types::Bytes};
use helix_macros::debug_trace;
use std::sync::Arc;

pub struct OutNodesIterator<'a, T> {
    pub iter: heed3::RoIter<
        'a,
        Bytes,
        heed3::types::LazyDecode<Bytes>,
        heed3::iteration_method::MoveOnCurrentKeyDuplicates,
    >,
    pub storage: Arc<HelixGraphStorage>,
    pub edge_type: EdgeType,
    pub txn: &'a T,
}

impl<'a> Iterator for OutNodesIterator<'a, RoTxn<'a>> {
    type Item = Result<TraversalValue, GraphError>;

    #[debug_trace("OUT")]
    fn next(&mut self) -> Option<Self::Item> {
        while let Some(Ok((_, data))) = self.iter.next() {
            match data.decode() {
                Ok(data) => {
                    let (_, item_id) = match HelixGraphStorage::unpack_adj_edge_data(data) {
                        Ok(data) => data,
                        Err(e) => {
                            println!("Error unpacking edge data: {e:?}");
                            return Some(Err(e));
                        }
                    };
                    match self.edge_type {
                        EdgeType::Node => {
                            if let Ok(node) = self.storage.get_node(self.txn, &item_id) {
                                return Some(Ok(TraversalValue::Node(node)));
                            }
                        }
                        EdgeType::Vec => {
                            if let Ok(vector) = self.storage.get_vector(self.txn, &item_id) {
                                return Some(Ok(TraversalValue::Vector(vector)));
                            }
                        }
                    }
                }
                Err(e) => {
                    println!("Error decoding edge data: {e:?}");
                    return Some(Err(GraphError::DecodeError(e.to_string())));
                }
            }
        }
        None
    }
}

pub trait OutAdapter<'a, T>: Iterator<Item = Result<TraversalValue, GraphError>> {
    /// Returns an iterator containing the nodes that have an outgoing edge with the given label.
    ///
    /// Note that the `edge_label` cannot be empty and must be a valid, existing edge label.
    ///
    /// To provide safety, you cannot get all outgoing nodes as it would be ambiguous as to what
    /// type that resulting node would be.
    fn out(
        self,
        edge_label: &'a str,
        edge_type: &'a EdgeType,
    ) -> RoTraversalIterator<'a, impl Iterator<Item = Result<TraversalValue, GraphError>>>;
}

impl<'a, I: Iterator<Item = Result<TraversalValue, GraphError>>> OutAdapter<'a, RoTxn<'a>>
    for RoTraversalIterator<'a, I>
{
    #[inline]
    fn out(
        self,
        edge_label: &'a str,
        edge_type: &'a EdgeType,
    ) -> RoTraversalIterator<'a, impl Iterator<Item = Result<TraversalValue, GraphError>>> {
        let db = Arc::clone(&self.storage);
        let storage = Arc::clone(&self.storage);
        let txn = self.txn;

        let iter = self
            .inner
            .filter_map(move |item| {
                let edge_label_hash = hash_label(edge_label, None);
                let prefix = HelixGraphStorage::out_edge_key(
                    &match item {
                        Ok(item) => item.id(),
                        Err(_) => return None,
                    },
                    &edge_label_hash,
                );
                match db
                    .out_edges_db
                    .lazily_decode_data()
                    .get_duplicates(txn, &prefix)
                {
                    Ok(Some(iter)) => Some(OutNodesIterator {
                        iter,
                        storage: Arc::clone(&db),
                        edge_type: edge_type.clone(),
                        txn,
                    }),
                    Ok(None) => None,
                    Err(e) => {
                        println!("{} Error getting out edges: {:?}", line!(), e);
                        // return Err(e);
                        None
                    }
                }
            })
            .flatten();

        RoTraversalIterator {
            inner: iter,
            storage,
            txn,
        }
    }
}

pub trait OutAdapterArena<'a, 'env, T>:
    Iterator<Item = Result<TraversalValueArena<'a>, GraphError>>
{
    /// Returns an iterator containing the nodes that have an outgoing edge with the given label.
    ///
    /// Note that the `edge_label` cannot be empty and must be a valid, existing edge label.
    ///
    /// To provide safety, you cannot get all outgoing nodes as it would be ambiguous as to what
    /// type that resulting node would be.
    fn out_vec(
        self,
        edge_label: &'a str,
    ) -> RoArenaTraversalIterator<
        'a,
        'env,
        impl Iterator<Item = Result<TraversalValueArena<'a>, GraphError>>,
    >;
}

pub struct OutNodesIteratorArena<'a, 'env, T> {
    pub iter: heed3::RoIter<
        'a,
        Bytes,
        heed3::types::LazyDecode<Bytes>,
        heed3::iteration_method::MoveOnCurrentKeyDuplicates,
    >,
    pub storage: &'env HelixGraphStorage,
    pub txn: &'a T,
}

impl<'a, 'env> Iterator for OutNodesIteratorArena<'a, 'env, RoTxn<'a>> {
    type Item = Result<TraversalValueArena<'a>, GraphError>;

    #[debug_trace("OUT")]
    fn next(&mut self) -> Option<Self::Item> {
        while let Some(Ok((_, data))) = self.iter.next() {
            match data.decode() {
                Ok(data) => {
                    let (_, item_id) = match HelixGraphStorage::unpack_adj_edge_data(data) {
                        Ok(data) => data,
                        Err(e) => {
                            println!("Error unpacking edge data: {e:?}");
                            return Some(Err(e));
                        }
                    };
                    if let Ok(node) = self.storage.get_node(self.txn, &item_id) {
                        return Some(Ok(TraversalValueArena::Node(node)));
                    }
                }
                Err(e) => {
                    println!("Error decoding edge data: {e:?}");
                    return Some(Err(GraphError::DecodeError(e.to_string())));
                }
            }
        }
        None
    }
}

pub struct OutVecIteratorArena<'a, 'env, T> {
    pub iter: heed3::RoIter<
        'a,
        Bytes,
        heed3::types::LazyDecode<Bytes>,
        heed3::iteration_method::MoveOnCurrentKeyDuplicates,
    >,
    pub storage: &'env HelixGraphStorageArena,
    pub txn: &'a T,
    pub arena: &'a bumpalo::Bump,
}

impl<'a, 'env> Iterator for OutVecIteratorArena<'a, 'env, RoTxn<'a>> {
    type Item = Result<TraversalValueArena<'a>, GraphError>;

    #[debug_trace("OUT")]
    fn next(&mut self) -> Option<Self::Item> {
        while let Some(Ok((_, data))) = self.iter.next() {
            match data.decode() {
                Ok(data) => {
                    let (_, item_id) = match HelixGraphStorage::unpack_adj_edge_data(data) {
                        Ok(data) => data,
                        Err(e) => {
                            println!("Error unpacking edge data: {e:?}");
                            return Some(Err(e));
                        }
                    };
                    if let Ok(node) = self.storage.get_vector(self.txn, &item_id, self.arena) {
                        return Some(Ok(TraversalValueArena::Vector(node)));
                    }
                }
                Err(e) => {
                    println!("Error decoding edge data: {e:?}");
                    return Some(Err(GraphError::DecodeError(e.to_string())));
                }
            }
        }
        None
    }
}

impl<'a, 'env, I: Iterator<Item = Result<TraversalValueArena<'a>, GraphError>>>
    OutAdapterArena<'a, 'env, RoTxn<'a>> for RoArenaTraversalIterator<'a, 'env, I>
{
    #[inline]
    fn out_vec(
        self,
        edge_label: &'a str,
    ) -> RoArenaTraversalIterator<
        'a,
        'env,
        impl Iterator<Item = Result<TraversalValueArena<'a>, GraphError>>,
    > {
        let txn = self.txn;

        let iter = self
            .inner
            .filter_map(move |item| {
                let edge_label_hash = hash_label(edge_label, None);
                let prefix = HelixGraphStorage::out_edge_key(
                    &match item {
                        Ok(item) => item.id(),
                        Err(_) => return None,
                    },
                    &edge_label_hash,
                );
                match self
                    .storage
                    .out_edges_db
                    .lazily_decode_data()
                    .get_duplicates(txn, &prefix)
                {
                    Ok(Some(iter)) => Some(OutVecIteratorArena {
                        iter,
                        storage: self.storage,
                        txn,
                        arena: self.arena,
                    }),
                    Ok(None) => None,
                    Err(e) => {
                        println!("{} Error getting out edges: {:?}", line!(), e);
                        // return Err(e);
                        None
                    }
                }
            })
            .flatten();

        RoArenaTraversalIterator {
            inner: iter,
            storage: self.storage,
            arena: self.arena,
            txn,
        }
    }
}
