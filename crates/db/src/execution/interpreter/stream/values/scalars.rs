//! Scalar terminal sequence contracts.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use super::*;

pub(in crate::execution::interpreter::stream) fn scalar_items(
    value: ExecutionValue,
) -> Vec<ExecutionScalar> {
    match value {
        ExecutionValue::Count(count) => vec![count_scalar(count)],
        ExecutionValue::Bool(value) => vec![ExecutionScalar::Value(DbPropertyValue::Bool(value))],
        ExecutionValue::Scalars(values) => values,
        ExecutionValue::Stream(_)
        | ExecutionValue::FoldedStream(_)
        | ExecutionValue::IndexDdlReceipt(_)
        | ExecutionValue::IndexOperationStatus(_) => {
            unreachable!("scalar_items is only called for scalar execution values")
        }
    }
}

pub(in crate::execution::interpreter::stream) fn limit_scalars(
    mut values: Vec<ExecutionScalar>,
    count: usize,
) -> Vec<ExecutionScalar> {
    values.truncate(count);
    values
}

pub(in crate::execution::interpreter::stream) fn skip_scalars(
    values: Vec<ExecutionScalar>,
    count: usize,
) -> Vec<ExecutionScalar> {
    values.into_iter().skip(count).collect()
}

pub(in crate::execution::interpreter::stream) fn slice_scalars(
    values: Vec<ExecutionScalar>,
    start: usize,
    end: usize,
) -> Vec<ExecutionScalar> {
    values
        .into_iter()
        .skip(start)
        .take(end.saturating_sub(start))
        .collect()
}

pub(in crate::execution::interpreter::stream) fn distinct_scalars(
    values: Vec<ExecutionScalar>,
) -> Vec<ExecutionScalar> {
    let mut seen = BTreeSet::new();
    values
        .into_iter()
        .filter(|value| seen.insert(DistinctKey::from(value)))
        .collect()
}

fn count_scalar(count: usize) -> ExecutionScalar {
    ExecutionScalar::Value(DbPropertyValue::I64(count.try_into().unwrap_or(i64::MAX)))
}

/// DISTINCT identity for scalar outputs: property values compare by storage
/// `total_order` (CanonicalNumber for numerics, so `42`, `42.0` and `42.0f32` are one
/// value), projected objects compare entry-wise the same way, ids and strings compare
/// structurally. This is the identity `WHERE`, `ORDER BY`, GROUP BY and secondary
/// indexes already use. Debug strings are not an identity. The first-seen value wins.
pub(in crate::execution::interpreter::stream) struct DistinctKey(ExecutionScalar);

impl From<&ExecutionScalar> for DistinctKey {
    fn from(value: &ExecutionScalar) -> Self {
        Self(value.clone())
    }
}

impl PartialEq for DistinctKey {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for DistinctKey {}

impl PartialOrd for DistinctKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for DistinctKey {
    fn cmp(&self, other: &Self) -> Ordering {
        let rank = |value: &ExecutionScalar| match value {
            ExecutionScalar::NodeId(_) => 0_u8,
            ExecutionScalar::EdgeId(_) => 1,
            ExecutionScalar::String(_) => 2,
            ExecutionScalar::Value(_) => 3,
            ExecutionScalar::Object(_) => 4,
        };
        rank(&self.0)
            .cmp(&rank(&other.0))
            .then_with(|| match (&self.0, &other.0) {
                (ExecutionScalar::NodeId(left), ExecutionScalar::NodeId(right))
                | (ExecutionScalar::EdgeId(left), ExecutionScalar::EdgeId(right)) => {
                    left.cmp(right)
                }
                (ExecutionScalar::String(left), ExecutionScalar::String(right)) => {
                    left.cmp(right)
                }
                (ExecutionScalar::Value(left), ExecutionScalar::Value(right)) => {
                    left.total_order(right)
                }
                (ExecutionScalar::Object(left), ExecutionScalar::Object(right)) => {
                    object_total_order(left, right)
                }
                _ => Ordering::Equal,
            })
    }
}

/// Entry-wise `total_order` over projected objects, mirroring how storage orders
/// `PropertyValue::Object`.
fn object_total_order(
    left: &BTreeMap<String, DbPropertyValue>,
    right: &BTreeMap<String, DbPropertyValue>,
) -> Ordering {
    left.iter()
        .zip(right)
        .find_map(|((left_key, left_value), (right_key, right_value))| {
            let ordering = left_key
                .cmp(right_key)
                .then_with(|| left_value.total_order(right_value));
            (ordering != Ordering::Equal).then_some(ordering)
        })
        .unwrap_or_else(|| left.len().cmp(&right.len()))
}
