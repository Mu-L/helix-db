use crate::{
    helix_engine::{
        traversal_core::{
            traversal_iter::{RoTraversalIterator, RwTraversalIterator},
            traversal_value::TraversalValue,
        },
        types::GraphError,
    },
    protocol::value::Value,
    utils::count::Count,
};

pub trait CountAdapter<'arena>: Iterator {
    fn count_to_traversal_value(self) -> TraversalValue<'arena>;
    fn count_to_val(self) -> Value;
}

impl<'db, 'arena: 'txn, 'txn, I: Iterator<Item = Result<TraversalValue<'arena>, GraphError>>>
    CountAdapter<'arena> for RoTraversalIterator<'db, 'arena, 'txn, I>
{
    fn count_to_traversal_value(self) -> TraversalValue<'arena> {
        TraversalValue::Count(Count::from(self.inner.count()))
    }

    fn count_to_val(self) -> Value {
        Value::from(self.inner.count())
    }
}

impl<'db, 'arena: 'txn, 'txn, I: Iterator<Item = Result<TraversalValue<'arena>, GraphError>>>
    CountAdapter<'arena> for RwTraversalIterator<'db, 'arena, 'txn, I>
{
    fn count_to_traversal_value(self) -> TraversalValue<'arena> {
        TraversalValue::Count(Count::from(self.inner.count()))
    }

    fn count_to_val(self) -> Value {
        Value::from(self.inner.count())
    }
}
