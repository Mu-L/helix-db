use heed3::RoTxn;

use crate::helix_engine::{
    bm25::bm25::BM25,
    storage_core::{HelixGraphStorage, storage_methods::StorageMethods},
    traversal_core::{traversal_iter::RoTraversalIterator, traversal_value::TraversalValue},
    types::GraphError,
};
use std::sync::Arc;

pub struct SearchBM25<'scope, 'inner> {
    txn: &'scope RoTxn<'scope>,
    iter: std::vec::IntoIter<(u128, f32)>,
    storage: Arc<HelixGraphStorage>,
    label: &'inner str,
}

// implementing iterator for SearchBM25
impl<'scope, 'inner> Iterator for SearchBM25<'scope, 'inner> {
    type Item = Result<TraversalValue, GraphError>;

    fn next(&mut self) -> Option<Self::Item> {
        let next = self.iter.next()?;
        match self.storage.get_node(self.txn, &next.0) {
            Ok(node) => {
                if node.label == self.label {
                    Some(Ok(TraversalValue::Node(node)))
                } else {
                    None
                }
            }
            Err(e) => Some(Err(e)),
        }
    }
}

pub trait SearchBM25Adapter<'a>: Iterator<Item = Result<TraversalValue, GraphError>> {
    fn search_bm25<K>(
        self,
        label: &str,
        query: &str,
        k: K,
    ) -> Result<
        RoTraversalIterator<'a, impl Iterator<Item = Result<TraversalValue, GraphError>>>,
        GraphError,
    >
    where
        K: TryInto<usize>,
        K::Error: std::fmt::Debug;
}

impl<'a, I: Iterator<Item = Result<TraversalValue, GraphError>>> SearchBM25Adapter<'a>
    for RoTraversalIterator<'a, I>
{
    fn search_bm25<K>(
        self,
        label: &str,
        query: &str,
        k: K,
    ) -> Result<
        RoTraversalIterator<'a, impl Iterator<Item = Result<TraversalValue, GraphError>>>,
        GraphError,
    >
    where
        K: TryInto<usize>,
        K::Error: std::fmt::Debug,
    {
        let results = match self.storage.bm25.as_ref() {
            Some(s) => s.search(self.txn, query, k.try_into().unwrap())?,
            None => return Err(GraphError::from("BM25 not enabled!")),
        };

        let iter = SearchBM25 {
            txn: self.txn,
            iter: results.into_iter(),
            storage: Arc::clone(&self.storage),
            label,
        };
        Ok(RoTraversalIterator {
            inner: iter,
            storage: self.storage,
            txn: self.txn,
        })
    }
}
