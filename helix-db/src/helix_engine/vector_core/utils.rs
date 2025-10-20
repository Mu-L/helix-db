use super::binary_heap::BinaryHeap;
use crate::{
    helix_engine::{types::VectorError, vector_core::vector::HVector},
    protocol::value::Value,
};
use heed3::{byteorder::BE, types::{Bytes, U128}, Database, RoTxn};
use std::cmp::Ordering;

#[derive(PartialEq)]
pub(super) struct Candidate {
    pub id: u128,
    pub distance: f64,
}

impl Eq for Candidate {}

impl PartialOrd for Candidate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Candidate {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .distance
            .partial_cmp(&self.distance)
            .unwrap_or(Ordering::Equal)
    }
}

pub(super) trait HeapOps<'a, T> {
    /// Take the top k elements from the heap
    /// Used because using `.iter()` does not keep the order
    fn take_inord(&mut self, k: usize) -> BinaryHeap<'a, T>
    where
        T: Ord;

    /// Get the maximum element from the heap
    fn get_max<'q>(&'q self) -> Option<&'a T>
    where
        T: Ord,
        'q: 'a;
}

impl<'a, T> HeapOps<'a, T> for BinaryHeap<'a, T> {
    #[inline(always)]
    fn take_inord(&mut self, k: usize) -> BinaryHeap<'a, T>
    where
        T: Ord,
    {
        let mut result = BinaryHeap::with_capacity(self.arena, k);
        for _ in 0..k {
            if let Some(item) = self.pop() {
                result.push(item);
            } else {
                break;
            }
        }
        result
    }

    #[inline(always)]
    fn get_max<'q>(&'q self) -> Option<&'a T>
    where
        T: Ord,
        'q: 'a,
    {
        self.iter().max()
    }
}

pub trait VectorFilter<'a, 'q> {
    fn to_vec_with_filter<F, const SHOULD_CHECK_DELETED: bool>(
        self,
        k: usize,
        filter: Option<&'q [F]>,
        label: &'q str,
        txn: &'q RoTxn<'q>,
        db: Database<U128<BE>, Bytes>,
        arena: &'a bumpalo::Bump,
    ) -> Result<bumpalo::collections::Vec<'a, HVector<'a>>, VectorError>
    where
        F: Fn(&HVector, &RoTxn) -> bool;
}

impl<'a, 'q> VectorFilter<'a, 'q> for BinaryHeap<'a, HVector<'a>> {
    #[inline(always)]
    fn to_vec_with_filter<F, const SHOULD_CHECK_DELETED: bool>(
        mut self,
        k: usize,
        filter: Option<&'q [F]>,
        label: &'q str,
        txn: &'q RoTxn<'q>,
        db: Database<U128<BE>, Bytes>,
        arena: &'a bumpalo::Bump,
    ) -> Result<bumpalo::collections::Vec<'a, HVector<'a>>, VectorError>
    where
        F: Fn(&HVector, &RoTxn) -> bool,
    {
        let mut result = bumpalo::collections::Vec::with_capacity_in(k, arena);
        for _ in 0..k {
            // while pop check filters and pop until one passes
            while let Some(mut item) = self.pop() {
                item.properties = match db.get(txn, &item.get_id())? {
                    Some(bytes) => Some(bincode::deserialize(bytes).map_err(VectorError::from)?),
                    None => None, // TODO: maybe should be an error?
                };

                if SHOULD_CHECK_DELETED
                    && let Some(is_deleted) = item.get_property("is_deleted")
                    && *is_deleted == true
                {
                    continue;
                }

                if item.label() == label
                    && (filter.is_none() || filter.unwrap().iter().all(|f| f(&item, txn)))
                {
                    result.push(item);
                    break;
                }
            }
        }

        Ok(result)
    }
}
