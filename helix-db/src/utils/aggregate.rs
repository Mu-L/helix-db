use std::collections::{HashMap, HashSet};


use crate::helix_engine::traversal_core::traversal_value::TraversalValue;

#[derive(Clone, Default)]
pub struct AggregateItem {
    pub values: HashSet<TraversalValue>,
    pub count: i32,
}


#[derive(Clone)]
pub enum Aggregate {
    Group(HashMap<String, AggregateItem>),
    Count(HashMap<String, AggregateItem>),
}

impl Aggregate {
    pub fn into_count(self) -> Self {
        match self {
            Self::Group(g) => Self::Count(g),
            _ => self,
        }
    }
}
