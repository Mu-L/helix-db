use std::collections::{HashMap, HashSet};

use crate::{
    helix_engine::{
        traversal_core::{
            traversal_iter::RoTraversalIterator,
            traversal_value::{Traversable, TraversalValue},
        },
        types::GraphError,
    },
    protocol::value::Value,
};

pub trait AggregateAdapter<'a>: Iterator {
    fn aggregate(
        self,
        properties: &[&str],
    ) -> Result<HashMap<String, HashSet<TraversalValue>>, GraphError>;
}

impl<'a, I: Iterator<Item = Result<TraversalValue, GraphError>>> AggregateAdapter<'a>
    for RoTraversalIterator<'a, I>
{
    fn aggregate(
        self,
        properties: &[&str],
    ) -> Result<HashMap<String, HashSet<TraversalValue>>, GraphError> {
    }
}