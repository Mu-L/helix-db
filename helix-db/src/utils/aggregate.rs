use std::collections::{HashMap, HashSet};

use serde::Serialize;

use crate::helix_engine::traversal_core::traversal_value::TraversalValue;

pub enum Aggregate {
    Group(HashMap<String, HashSet<TraversalValue>>),
    Count(HashMap<String, HashSet<TraversalValue>>),
}

impl Aggregate {
    pub fn new(data: HashMap<String, HashSet<TraversalValue>>) -> Self {
        Aggregate::Group(data)
    }

    pub fn count(self) -> Self {
        if let Aggregate::Group(data) = self {
            Aggregate::Count(data)
        } else {
            self
        }
    }
}
