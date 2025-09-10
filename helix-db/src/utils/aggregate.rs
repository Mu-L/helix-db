use std::collections::{HashMap, HashSet};

use crate::helix_engine::traversal_core::traversal_value::TraversalValue;

pub struct Aggregate(pub HashMap<String, HashSet<TraversalValue>>);
