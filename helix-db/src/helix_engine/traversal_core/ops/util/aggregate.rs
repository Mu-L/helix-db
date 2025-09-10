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
    utils::aggregate::{Aggregate, AggregateItem},
};

pub trait AggregateAdapter<'a>: Iterator {
    fn aggregate(self, properties: &[&str]) -> Result<Aggregate, GraphError>;
}

impl<'a, I: Iterator<Item = Result<TraversalValue, GraphError>>> AggregateAdapter<'a>
    for RoTraversalIterator<'a, I>
{
    fn aggregate(self, properties: &[&str]) -> Result<Aggregate, GraphError> {
        let mut groups: HashMap<String, AggregateItem> = HashMap::new();

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

            let group = groups.entry(key).or_insert_with(AggregateItem::new);
            group.values.insert(item);
            group.count += 1;
        }

        Ok(Aggregate::Group(groups))
    }
}
