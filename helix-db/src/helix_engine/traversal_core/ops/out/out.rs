use crate::{
    helix_engine::{
        storage_core::{HelixGraphStorage, storage_methods::StorageMethods},
        traversal_core::{
            traversal_iter::RoTraversalIterator,
            traversal_value::{Traversable, TraversalValue},
        },
        types::GraphError,
    },
    utils::label_hash::hash_label,
};
use heed3::{RoTxn, types::Bytes};
use helix_macros::debug_trace;

pub struct OutNodesIterator<'db, 'arena, 'txn>
where
    'db: 'arena,
    'arena: 'txn,
{
    pub storage: &'db HelixGraphStorage,
    pub arena: &'arena bumpalo::Bump,
    pub txn: &'txn RoTxn<'db>,
    pub iter: heed3::RoIter<
        'txn,
        Bytes,
        heed3::types::LazyDecode<Bytes>,
        heed3::iteration_method::MoveOnCurrentKeyDuplicates,
    >,
}

pub struct OutVecIterator<'db, 'arena, 'txn>
where
    'db: 'arena,
    'arena: 'txn,
{
    pub storage: &'db HelixGraphStorage,
    pub arena: &'arena bumpalo::Bump,
    pub txn: &'txn RoTxn<'db>,
    pub iter: heed3::RoIter<
        'txn,
        Bytes,
        heed3::types::LazyDecode<Bytes>,
        heed3::iteration_method::MoveOnCurrentKeyDuplicates,
    >,
    /// Whether to read vector data from 'vector_db' (if true) table or read from 'vector_properties_db' table (if false).
    /// If false, it will treat it as a normal node and avoid reading the additional bytes.
    pub get_vector_data: bool,
}

impl<'db, 'arena, 'txn, 's> Iterator for OutNodesIterator<'db, 'arena, 'txn> {
    type Item = Result<TraversalValue<'arena>, GraphError>;

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
                        return Some(Ok(TraversalValue::Node(node)));
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

impl<'db, 'arena, 'txn> Iterator for OutVecIterator<'db, 'arena, 'txn> {
    type Item = Result<TraversalValue<'arena>, GraphError>;

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
                    if self.get_vector_data {
                        if let Ok(vec) = self.storage.get_vector_in(self.txn, &item_id, self.arena)
                        {
                            return Some(Ok(TraversalValue::Vector(vec)));
                        }
                    } else {
                        if let Ok(vec) = self
                            .storage
                            .get_vector_without_raw_vector_data_in(self.txn, &item_id, self.arena)
                        {
                            return Some(Ok(TraversalValue::VectorNodeWithoutVectorData(vec)));
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

pub trait OutAdapter<'db, 'arena, 'txn, 's>:
    Iterator<Item = Result<TraversalValue<'arena>, GraphError>>
{
    /// Returns an iterator containing the nodes that have an outgoing edge with the given label.
    ///
    /// Note that the `edge_label` cannot be empty and must be a valid, existing edge label.
    ///
    /// To provide safety, you cannot get all outgoing nodes as it would be ambiguous as to what
    /// type that resulting node would be.
    fn out_vec(
        self,
        edge_label: &'s str,
        get_vector_data: bool,
    ) -> RoTraversalIterator<
        'db,
        'arena,
        'txn,
        impl Iterator<Item = Result<TraversalValue<'arena>, GraphError>>,
    >;

    fn out_node(
        self,
        edge_label: &'s str,
    ) -> RoTraversalIterator<
        'db,
        'arena,
        'txn,
        impl Iterator<Item = Result<TraversalValue<'arena>, GraphError>>,
    >;
}

impl<'db, 'arena, 'txn, 's, I: Iterator<Item = Result<TraversalValue<'arena>, GraphError>>>
    OutAdapter<'db, 'arena, 'txn, 's> for RoTraversalIterator<'db, 'arena, 'txn, I>
{
    #[inline]
    fn out_vec(
        self,
        edge_label: &'s str,
        get_vector_data: bool,
    ) -> RoTraversalIterator<
        'db,
        'arena,
        'txn,
        impl Iterator<Item = Result<TraversalValue<'arena>, GraphError>>,
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
                    Ok(Some(iter)) => Some(OutVecIterator {
                        iter,
                        storage: self.storage,
                        txn,
                        arena: self.arena,
                        get_vector_data,
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
            storage: self.storage,
            arena: self.arena,
            txn,
        }
    }

    #[inline]
    fn out_node(
        self,
        edge_label: &'s str,
    ) -> RoTraversalIterator<
        'db,
        'arena,
        'txn,
        impl Iterator<Item = Result<TraversalValue<'arena>, GraphError>>,
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
                    Ok(Some(iter)) => Some(OutNodesIterator {
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

        RoTraversalIterator {
            inner: iter,
            storage: self.storage,
            arena: self.arena,
            txn,
        }
    }
}
