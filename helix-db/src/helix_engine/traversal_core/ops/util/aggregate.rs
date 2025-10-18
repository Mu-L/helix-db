use std::collections::HashMap;

use crate::{
    helix_engine::{
        traversal_core::{
            traversal_iter::RoTraversalIterator,
            traversal_value::{Traversable, TraversalValue},
        },
        types::GraphError,
    },
    utils::aggregate::{Aggregate, AggregateItem},
};

pub trait AggregateAdapter: Iterator {
    fn aggregate_by(
        self,
        properties: &[String],
        should_count: bool,
    ) -> Result<Aggregate, GraphError>;
}

impl<'db, 'arena, 'txn, I: Iterator<Item = Result<TraversalValue<'arena>, GraphError>>>
    AggregateAdapter for RoTraversalIterator<'db, 'arena, 'txn, I>
{
    fn aggregate_by(
        self,
        properties: &[String],
        should_count: bool,
    ) -> Result<Aggregate, GraphError> {
        let mut groups: HashMap<String, AggregateItem> = HashMap::new();

        for item in self.inner {
            let item = item?;

            let mut kvs = Vec::new();
            let mut key_parts = Vec::new();

            for property in properties {
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

            let group = groups.entry(key).or_default();
            group.values.insert(item);
            group.count += 1;
        }

        if should_count {
            Ok(Aggregate::Count(groups))
        } else {
            Ok(Aggregate::Group(groups))
        }
    }
}
