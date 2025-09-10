use std::collections::HashMap;

use serde::{Serialize, ser::SerializeSeq};

use crate::protocol::value::Value;

pub enum GroupBy {
    Group(HashMap<String, HashMap<String, Value>>),
    Count(HashMap<String, HashMap<String, Value>>),
}

impl GroupBy {
    pub fn new(data: HashMap<String, HashMap<String, Value>>) -> Self {
        GroupBy::Group(data)
    }

    pub fn count(self) -> Self {
        if let GroupBy::Group(data) = self {
            GroupBy::Count(data)
        } else {
            self
        }
    }
}