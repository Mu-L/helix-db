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

pub struct OutEdgesIterator<'db, 'arena, 'txn>
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

impl<'db, 'arena, 'txn> Iterator for OutEdgesIterator<'db, 'arena, 'txn> {
    type Item = Result<TraversalValue<'arena>, GraphError>;

    #[debug_trace("OUT_E")]
    fn next(&mut self) -> Option<Self::Item> {
        while let Some(Ok((_, data))) = self.iter.next() {
            match data.decode() {
                Ok(data) => {
                    let (edge_id, _) = match HelixGraphStorage::unpack_adj_edge_data(data) {
                        Ok(data) => data,
                        Err(e) => {
                            println!("Error unpacking edge data: {e:?}");
                            return Some(Err(e));
                        }
                    };
                    if let Ok(edge) = self.storage.get_edge(self.txn, &edge_id) {
                        return Some(Ok(TraversalValue::Edge(edge)));
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

pub trait OutEdgesAdapter<'db, 'arena, 'txn, 's>:
    Iterator<Item = Result<TraversalValue<'arena>, GraphError>>
{
    /// Returns an iterator containing the edges that have an outgoing edge with the given label.
    ///
    /// Note that the `edge_label` cannot be empty and must be a valid, existing edge label.
    ///
    /// To provide safety, you cannot get all outgoing edges as it would be ambiguous as to what
    /// type that resulting edge would be.
    fn out_e(
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
    OutEdgesAdapter<'db, 'arena, 'txn, 's> for RoTraversalIterator<'db, 'arena, 'txn, I>
{
    #[inline]
    fn out_e(
        self,
        edge_label: &'s str,
    ) -> RoTraversalIterator<
        'db,
        'arena,
        'txn,
        impl Iterator<Item = Result<TraversalValue<'arena>, GraphError>>,
    > {
        // iterate through the iterator and create a new iterator on the out edges
        let iter = self
            .inner
            .filter_map(move |item| {
                let edge_label_hash = hash_label(edge_label, None);
                match item {
                    Ok(item) => {
                        let prefix = HelixGraphStorage::out_edge_key(&item.id(), &edge_label_hash);
                        match self
                            .storage
                            .out_edges_db
                            .lazily_decode_data()
                            .get_duplicates(self.txn, &prefix)
                        {
                            Ok(Some(iter)) => Some(OutEdgesIterator {
                                storage: self.storage,
                                arena: self.arena,
                                txn: self.txn,
                                iter,
                            }),
                            Ok(None) => None,
                            Err(e) => {
                                println!("{} Error getting out edges: {:?}", line!(), e);
                                // return Err(e);
                                None
                            }
                        }
                    }
                    Err(e) => {
                        println!("{} Error getting oupt edges: {:?}", line!(), e);
                        None
                    }
                }
            })
            .flatten();
        RoTraversalIterator {
            storage: self.storage,
            arena: self.arena,
            txn: self.txn,
            inner: iter,
        }
    }
}
