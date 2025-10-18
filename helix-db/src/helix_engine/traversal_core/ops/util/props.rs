use crate::{
    helix_engine::{
        traversal_core::{
            traversal_iter::{RoTraversalIterator, RwTraversalIterator},
            traversal_value::TraversalValue,
        },
        types::GraphError,
    },
    protocol::value::Value,
    utils::properties::ImmutablePropertiesMap,
};

pub struct PropsIterator<'s, I> {
    iter: I,
    prop: &'s str,
}

impl<'arena, 's, I> Iterator for PropsIterator<'s, I>
where
    I: Iterator,
{
    type Item = I::Item;
    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }
}
pub trait PropsAdapter<'db, 'arena, 'txn, 's, I>:
    Iterator<Item = Result<TraversalValue<'arena>, GraphError>>
{
    /// Returns a new iterator which yeilds the value of the property if it exists
    ///
    /// Given the type checking of the schema there should be no need to return an empty traversal.
    fn get_property(self, prop: &'s str) -> RoTraversalIterator<'db, 'arena, 'txn, impl Iterator>;
}

impl<'db, 'arena, 'txn, 's, I> PropsAdapter<'db, 'arena, 'txn, 's, I>
    for RoTraversalIterator<'db, 'arena, 'txn, I>
where
    I: Iterator<Item = Result<TraversalValue<'arena>, GraphError>>,
{
    #[inline]
    fn get_property(self, prop: &'s str) -> RoTraversalIterator<'db, 'arena, 'txn, impl Iterator> {
        let iter = self.inner.filter_map(move |item| match &item {
            Ok(TraversalValue::Node(node)) => Some(node.get_property(prop)),
            Ok(TraversalValue::Edge(edge)) => Some(edge.get_property(prop)),
            Ok(TraversalValue::Vector(vec)) => Some(vec.get_property(prop)),
            _ => None,
        });
        RoTraversalIterator {
            storage: self.storage,
            arena: self.arena,
            txn: self.txn,
            inner: PropsIterator { iter, prop },
        }
    }
}
