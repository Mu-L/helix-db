use crate::helix_engine::{
    storage_core::HelixGraphStorage,
    traversal_core::{
        traversal_iter::RoTraversalIterator, traversal_value::TraversalValue, LMDB_STRING_HEADER_LENGTH
    },
    types::{GraphError, VectorError},
    vector_core::{hnsw::HNSW, vector::HVector, vector_without_data::VectorWithoutData},
};
use heed3::{
    RoTxn,
    byteorder::BE,
    types::{Bytes, U128},
};

pub struct VFromType<'db, 'arena, 'txn, 's>
where
    'db: 'arena,
    'arena: 'txn,
{
    storage: &'db HelixGraphStorage,
    arena: &'arena bumpalo::Bump,
    txn: &'txn RoTxn<'db>,
    iter: heed3::RoIter<'txn, U128<BE>, Bytes>,
    label: &'s str,
    label_bytes: &'s [u8],
    get_vector_data: bool,
}

impl<'db, 'arena, 'txn, 's> Iterator for VFromType<'db, 'arena, 'txn, 's> {
    type Item = Result<TraversalValue<'arena>, GraphError>;

    fn next(&mut self) -> Option<Self::Item> {
        for value in self.iter.by_ref() {
            let (key, value) = value.unwrap();

            assert!(
                value.len() >= LMDB_STRING_HEADER_LENGTH,
                "value length does not contain header which means the `label` field was missing from the node on insertion"
            );
            let length_of_label_in_lmdb =
                u64::from_le_bytes(value[..LMDB_STRING_HEADER_LENGTH].try_into().unwrap()) as usize;
            assert!(
                value.len() >= length_of_label_in_lmdb + LMDB_STRING_HEADER_LENGTH,
                "value length is not at least the header length plus the label length meaning there has been a corruption on node insertion"
            );
            let label_in_lmdb = &value[LMDB_STRING_HEADER_LENGTH
                ..LMDB_STRING_HEADER_LENGTH + length_of_label_in_lmdb as usize];

            if label_in_lmdb == self.label_bytes {
                let properties = 
                        // TODO change to use bump map here
                        bincode::deserialize(value)
                            .map_err(VectorError::from)
                            .ok()?;

                if self.get_vector_data {
                    let mut vector = match self.storage.vectors.get_raw_vector_data(self.txn, key, self.label, 0, self.arena) {
                        Ok(bytes) => bytes,
                        Err(e) => return Some(Err(GraphError::from(e))),
                    };
                    vector.properties = Some(properties);
                    return Some(Ok(TraversalValue::Vector(vector)));
                } else {
                    return Some(Ok(TraversalValue::VectorNodeWithoutVectorData(
                        VectorWithoutData::from_properties(key, self.label, 0, properties),
                    )));
                }
            } else {
                continue;
            }
        }
        None
    }
}

pub trait VFromTypeAdapter<'db, 'arena, 'txn, 's>:
    Iterator<Item = Result<TraversalValue<'arena>, GraphError>>
{
    type OutputIter: Iterator<Item = Result<TraversalValue<'arena>, GraphError>>;

    /// Returns an iterator containing the vector with the given label.
    ///
    /// Note that the `label` cannot be empty and must be a valid, existing vector label.
    fn v_from_type(self, label: &'s str, get_vector_data: bool) -> Self::OutputIter;
}

impl<'db, 'arena, 'txn, 's, I: Iterator<Item = Result<TraversalValue<'arena>, GraphError>>>
    VFromTypeAdapter<'db, 'arena, 'txn, 's> for RoTraversalIterator<'db, 'arena, 'txn, I>
{
    type OutputIter = RoTraversalIterator<'db, 'arena, 'txn, VFromType<'db, 'arena, 'txn, 's>>;

    #[inline]
    fn v_from_type(self, label: &'s str, get_vector_data: bool) -> Self::OutputIter {
        let iter = self
            .storage
            .vectors
            .vector_properties_db
            .iter(self.txn)
            .unwrap();
        let v_from_type = VFromType {
            storage: self.storage,
            arena: self.arena,
            txn: self.txn,
            label,
            label_bytes: label.as_bytes(),
            get_vector_data,
            iter,
        };

        RoTraversalIterator {
            storage: self.storage,
            arena: self.arena,
            txn: self.txn,
            inner: v_from_type,
        }
    }
}
