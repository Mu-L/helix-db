use std::collections::HashMap;

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

pub trait GroupByAdapter<'a>: Iterator {
    fn group_by(
        self,
        properties: &[&str],
    ) -> Result<HashMap<String, Vec<(String, Value)>>, GraphError>;
}

impl<'a, I: Iterator<Item = Result<TraversalValue, GraphError>>> GroupByAdapter<'a>
    for RoTraversalIterator<'a, I>
{
    fn group_by(
        self,
        properties: &[&str],
    ) -> Result<HashMap<String, Vec<(String, Value)>>, GraphError> {
        let mut groups: HashMap<String, Vec<(String, Value)>> = HashMap::new();

        for item in self.inner {
            let item = item?;

            // TODO HANDLE COUNT
            let mut kvs = Vec::new();
            let mut key_parts = Vec::new();

            for &property in properties {
                match item.check_property(property) {
                    Ok(val) => {
                        key_parts.push(val.inner_stringify());
                        kvs.push((property.to_string(), val.into_owned()));
                    }
                    Err(_) => {
                        key_parts.push("null".to_string());
                    }
                }
            }
            let key = key_parts.join("_");

            groups.entry(key).or_insert_with(Vec::new).extend(kvs);
        }

        Ok(groups)
    }
}