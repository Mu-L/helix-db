use std::collections::HashMap;

use crate::protocol::value::Value;

pub struct GroupByItem {
    pub values: HashMap<String, Value>,
    pub count: i32,
}

impl GroupByItem {
    pub fn new() -> Self {
        Self {
            values: HashMap::new(),
            count: 0,
        }
    }
}

pub enum GroupBy {
    Group(HashMap<String, GroupByItem>),
    Count(HashMap<String, GroupByItem>),
}

impl GroupBy {
    pub fn into_count(self) -> Self {
        match self {
            Self::Group(g) => Self::Count(g),
            _ => self,
        }
    }
}
