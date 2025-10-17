use crate::helix_engine::{
    storage_core::HelixGraphStorage,
    traversal_core::{
        LMDB_STRING_HEADER_LENGTH, traversal_iter::RoTraversalIterator,
        traversal_value::TraversalValue,
    },
    types::{GraphError, VectorError},
    vector_core::{vector::HVector, vector_without_data::VectorWithoutData},
};
use heed3::{RoTxn, types::Bytes};

pub struct VFromType<'db, 'arena, 'txn, 's>
where
    'db: 'arena,
    'arena: 'txn,
{
    storage: &'db HelixGraphStorage,
    arena: &'arena bumpalo::Bump,
    txn: &'txn RoTxn<'db>,
    iter: heed3::RoIter<'txn, Bytes, heed3::types::LazyDecode<Bytes>>,
    label: &'s [u8],
}

impl<'db, 'arena, 'txn, 's> Iterator for VFromType<'db, 'arena, 'txn, 's> {
    type Item = Result<TraversalValue<'arena>, GraphError>;

    fn next(&mut self) -> Option<Self::Item> {
        for value in self.iter.by_ref() {
            let (key, vector_data) = value.unwrap();
            match vector_data.decode() {
                Ok(value) => {
                    assert!(
                        value.len() >= LMDB_STRING_HEADER_LENGTH,
                        "value length does not contain header which means the `label` field was missing from the node on insertion"
                    );
                    let length_of_label_in_lmdb =
                        u64::from_le_bytes(value[..LMDB_STRING_HEADER_LENGTH].try_into().unwrap())
                            as usize;
                    assert!(
                        value.len() >= length_of_label_in_lmdb + LMDB_STRING_HEADER_LENGTH,
                        "value length is not at least the header length plus the label length meaning there has been a corruption on node insertion"
                    );
                    let label_in_lmdb = &value[LMDB_STRING_HEADER_LENGTH
                        ..LMDB_STRING_HEADER_LENGTH + length_of_label_in_lmdb as usize];

                    if label_in_lmdb == self.label {
                        let properties = match self
                            .storage
                            .vectors
                            .vector_properties_db
                            .get(self.txn, key)
                            .ok()?
                        {
                            Some(bytes) => Some(
                                // TODO change to use bump map here
                                bincode::deserialize(bytes)
                                    .map_err(VectorError::from)
                                    .ok()?,
                            ),
                            None => None,
                        };

                        let mut bytes = [0u8; 16];
                        bytes.copy_from_slice(&key[1..=16]);
                        let id = u128::from_be_bytes(bytes);
                        if self.get_vector_data {
                            match HVector::decode_vector(value, properties, id, self.arena) {
                                Ok(vector) => return Some(Ok(TraversalValue::Vector(vector))),
                                Err(e) => {
                                    println!("{} Error decoding vector: {:?}", line!(), e);
                                    return Some(Err(GraphError::ConversionError(e.to_string())));
                                }
                            }
                        } else {
                            return Some(Ok(TraversalValue::VectorNodeWithoutVectorData(
                                VectorWithoutData::from_properties(id, 0, properties),
                            )));
                        }
                    } else {
                        continue;
                    }
                }
                Err(e) => return Some(Err(GraphError::ConversionError(e.to_string()))),
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
            .vectors_properties_db
            .lazily_decode_data()
            .iter(self.txn)
            .unwrap();
        let v_from_type = VFromType {
            storage: self.storage,
            arena: self.arena,
            txn: self.txn,
            iter,
            label: label.as_bytes(),
        };

        RoTraversalIterator {
            storage: self.storage,
            arena: self.arena,
            txn: self.txn,
            inner: v_from_type,
        }
    }
}
