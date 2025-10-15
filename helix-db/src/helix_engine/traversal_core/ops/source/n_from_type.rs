use crate::{
    helix_engine::{
        traversal_core::{
            traversal_iter::RoTraversalIterator,
            traversal_value::TraversalValue,
            traversal_value_arena::{RoArenaTraversalIterator, TraversalValueArena},
        },
        types::GraphError,
    },
    utils::items::Node,
};
use heed3::{
    byteorder::BE,
    types::{Bytes, U128},
};
use helix_macros::debug_trace;

pub struct NFromType<'a> {
    pub iter: heed3::RoIter<'a, U128<BE>, heed3::types::LazyDecode<Bytes>>,
    pub label: &'a str,
}

impl<'a> Iterator for NFromType<'a> {
    type Item = Result<TraversalValue, GraphError>;

    #[debug_trace("N_FROM_TYPE")]
    fn next(&mut self) -> Option<Self::Item> {
        for value in self.iter.by_ref() {
            let (key_, value) = value.unwrap();
            match value.decode() {
                Ok(value) => match Node::decode_node(value, key_) {
                    Ok(node) => match &node.label {
                        label if label == self.label => {
                            return Some(Ok(TraversalValue::Node(node)));
                        }
                        _ => continue,
                    },
                    Err(e) => {
                        println!("{} Error decoding node: {:?}", line!(), e);
                        return Some(Err(GraphError::ConversionError(e.to_string())));
                    }
                },
                Err(e) => return Some(Err(GraphError::ConversionError(e.to_string()))),
            }
        }
        None
    }
}
pub trait NFromTypeAdapter<'a>: Iterator<Item = Result<TraversalValue, GraphError>> {
    /// Returns an iterator containing the nodes with the given label.
    ///
    /// Note that the `label` cannot be empty and must be a valid, existing node label.
    fn n_from_type(
        self,
        label: &'a str,
    ) -> RoTraversalIterator<'a, impl Iterator<Item = Result<TraversalValue, GraphError>>>;
}
impl<'a, I: Iterator<Item = Result<TraversalValue, GraphError>>> NFromTypeAdapter<'a>
    for RoTraversalIterator<'a, I>
{
    #[inline]
    fn n_from_type(
        self,
        label: &'a str,
    ) -> RoTraversalIterator<'a, impl Iterator<Item = Result<TraversalValue, GraphError>>> {
        let iter = self
            .storage
            .nodes_db
            .lazily_decode_data()
            .iter(self.txn)
            .unwrap();
        RoTraversalIterator {
            inner: NFromType { iter, label },
            storage: self.storage,
            txn: self.txn,
        }
    }
}

pub struct NFromTypeArena<'a> {
    pub iter: heed3::RoIter<'a, U128<BE>, heed3::types::LazyDecode<Bytes>>,
    pub label: &'a str,
}

impl<'a> Iterator for NFromTypeArena<'a> {
    type Item = Result<TraversalValueArena<'a>, GraphError>;

    #[debug_trace("N_FROM_TYPE")]
    fn next(&mut self) -> Option<Self::Item> {
        for value in self.iter.by_ref() {
            let (key_, value) = value.unwrap();
            match value.decode() {
                Ok(value) => match Node::decode_node(value, key_) {
                    Ok(node) => match &node.label {
                        label if label == self.label => {
                            return Some(Ok(TraversalValueArena::Node(node)));
                        }
                        _ => continue,
                    },
                    Err(e) => {
                        println!("{} Error decoding node: {:?}", line!(), e);
                        return Some(Err(GraphError::from(e.to_string())));
                    }
                },
                Err(e) => return Some(Err(GraphError::from(e.to_string()))),
            }
        }
        None
    }
}
pub trait NFromTypeAdapterArena<'a, 'env>:
    Iterator<Item = Result<TraversalValueArena<'a>, GraphError>>
{
    /// Returns an iterator containing the nodes with the given label.
    ///
    /// Note that the `label` cannot be empty and must be a valid, existing node label.
    fn n_from_type(
        self,
        label: &'a str,
    ) -> RoArenaTraversalIterator<
        'a,
        'env,
        impl Iterator<Item = Result<TraversalValueArena<'a>, GraphError>>,
    >;
}
impl<'a, 'env, I: Iterator<Item = Result<TraversalValueArena<'a>, GraphError>>>
    NFromTypeAdapterArena<'a, 'env> for RoArenaTraversalIterator<'a, 'env, I>
{
    #[inline]
    fn n_from_type(
        self,
        label: &'a str,
    ) -> RoArenaTraversalIterator<
        'a,
        'env,
        impl Iterator<Item = Result<TraversalValueArena<'a>, GraphError>>,
    > {
        let iter = self
            .storage
            .nodes_db
            .lazily_decode_data()
            .iter(self.txn)
            .unwrap();
        RoArenaTraversalIterator {
            inner: NFromTypeArena { iter, label },
            storage: self.storage,
            txn: self.txn,
            arena: self.arena,
        }
    }
}
