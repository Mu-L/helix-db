use crate::{helix_engine::types::VectorError, protocol::value::Value};
use crate::helix_engine::vector_core::vector::HVector;
use heed3::{RoTxn, RwTxn};

pub trait HNSW
{
    /// Search for the k nearest neighbors of a query vector
    ///
    /// # Arguments
    ///
    /// * `txn` - The transaction to use
    /// * `query` - The query vector
    /// * `k` - The number of nearest neighbors to search for
    ///
    /// # Returns
    ///
    /// A vector of tuples containing the id and distance of the nearest neighbors
    fn search<'a, 'q, F>(
        &self,
        txn: &'a RoTxn<'a>,
        query: &'q [f64],
        k: usize,
        label: &'q str,
        filter: Option<&'q [F]>,
        should_trickle: bool,
        arena: &'a bumpalo::Bump,
    ) -> Result<bumpalo::collections::Vec<'a, HVector<'a>>, VectorError>
    where
        F: Fn(&HVector, &RoTxn) -> bool,
        'a: 'q;

    /// Insert a new vector into the index
    ///
    /// # Arguments
    ///
    /// * `txn` - The transaction to use
    /// * `data` - The vector data
    ///
    /// # Returns
    ///
    /// An HVector of the data inserted
    fn insert<'arena, F>(
        &self,
        txn: &mut RwTxn,
        data: &[f64],
        fields: Option<Vec<(String, Value)>>,
    ) -> Result<HVector<'arena>, VectorError>
    where
        F: Fn(&HVector, &RoTxn) -> bool;

    /// Delete a vector from the index
    ///
    /// # Arguments
    ///
    /// * `txn` - The transaction to use
    /// * `id` - The id of the vector
    fn delete(
        &self,
        txn: &mut RwTxn,
        id: u128,
    ) -> Result<(), VectorError>;

    /// Get specific vector based on id and level
    ///
    /// # Arguments
    ///
    /// * `txn` - The transaction to use
    /// * `id` - The id of the vector
    /// * `level` - Which level to get the vector from
    /// * `with_data` - Whether or not to fetch the vector with data
    ///
    /// # Returns
    ///
    /// A `Result` containing a `Vec` of `HVector` if successful
    fn get_vector<'arena>(
        &self,
        txn: &RoTxn,
        id: u128,
        level: usize,
        with_data: bool,
        arena: &'arena bumpalo::Bump,
    ) -> Result<HVector<'arena>, VectorError>;
}

