use std::collections::{HashMap, HashSet};


use crate::helix_engine::traversal_core::traversal_value::TraversalValue;

pub struct AggregateItem {
    pub values: HashSet<TraversalValue>,
    pub count: i32,
}

impl AggregateItem {
    pub fn new() -> Self {
        Self { values: HashSet::new(), count: 0 }
    }
}

pub enum Aggregate {
    Group(HashMap<String, AggregateItem>),
    Count(HashMap<String, AggregateItem>),
}

