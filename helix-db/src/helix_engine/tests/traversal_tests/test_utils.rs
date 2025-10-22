use std::sync::Arc;

use bumpalo::Bump;
use heed3::{RoTxn, RwTxn};

use crate::{
    helix_engine::{
        storage_core::HelixGraphStorage,
        traversal_core::{
            ops::g::G,
            traversal_iter::{RoTraversalIterator, RwTraversalIterator},
            traversal_value::TraversalValue,
        },
        types::GraphError,
    },
    protocol::value::Value,
    utils::{
        items::{Edge, Node},
        properties::ImmutablePropertiesMap,
    },
};

pub fn props_map<'arena>(
    arena: &'arena Bump,
    props: Vec<(String, Value)>,
) -> ImmutablePropertiesMap<'arena> {
    let len = props.len();
    ImmutablePropertiesMap::new(
        len,
        props
            .into_iter()
            .map(|(key, value)| {
                let key: &'arena str = arena.alloc_str(&key);
                (key, value)
            }),
        arena,
    )
}

pub fn props_option<'arena>(
    arena: &'arena Bump,
    props: Vec<(String, Value)>,
) -> Option<ImmutablePropertiesMap<'arena>> {
    Some(props_map(arena, props))
}

pub fn g_new<'db, 'arena, 'txn>(
    storage: &'db Arc<HelixGraphStorage>,
    txn: &'txn RoTxn<'db>,
    arena: &'arena Bump,
) -> RoTraversalIterator<
    'db,
    'arena,
    'txn,
    impl Iterator<Item = Result<TraversalValue<'arena>, GraphError>>,
> {
    G::new(storage.as_ref(), txn, arena)
}

pub fn g_from_iter<'db, 'arena, 'txn>(
    storage: &'db Arc<HelixGraphStorage>,
    txn: &'txn RoTxn<'db>,
    arena: &'arena Bump,
    items: impl Iterator<Item = TraversalValue<'arena>>,
) -> RoTraversalIterator<
    'db,
    'arena,
    'txn,
    impl Iterator<Item = Result<TraversalValue<'arena>, GraphError>>,
> {
    G::from_iter(storage.as_ref(), txn, items, arena)
}

pub fn g_new_mut<'db, 'arena, 'txn>(
    storage: &'db Arc<HelixGraphStorage>,
    arena: &'arena Bump,
    txn: &'txn mut RwTxn<'db>,
) -> RwTraversalIterator<
    'db,
    'arena,
    'txn,
    impl Iterator<Item = Result<TraversalValue<'arena>, GraphError>>,
> {
    G::new_mut(storage.as_ref(), arena, txn)
}

pub fn g_new_mut_from_iter<'db, 'arena, 'txn>(
    storage: &'db Arc<HelixGraphStorage>,
    arena: &'arena Bump,
    txn: &'txn mut RwTxn<'db>,
    items: impl Iterator<Item = TraversalValue<'arena>>,
) -> RwTraversalIterator<
    'db,
    'arena,
    'txn,
    impl Iterator<Item = Result<TraversalValue<'arena>, GraphError>>,
> {
    G::new_mut_from_iter(storage.as_ref(), txn, items, arena)
}


pub trait NodeTestExt {
    fn check_property(&self, key: &str) -> Result<&Value, GraphError>;
}

impl NodeTestExt for Node<'_> {
    fn check_property(&self, key: &str) -> Result<&Value, GraphError> {
        self.get_property(key)
            .ok_or_else(|| GraphError::New(format!("Property {key} not found")))
    }
}

pub trait EdgeTestExt {
    fn check_property(&self, key: &str) -> Result<&Value, GraphError>;
}

impl EdgeTestExt for Edge<'_> {
    fn check_property(&self, key: &str) -> Result<&Value, GraphError> {
        self.get_property(key)
            .ok_or_else(|| GraphError::New(format!("Property {key} not found")))
    }
}

pub trait TraversalValueTestExt {
    fn check_property(&self, key: &str) -> Result<&Value, GraphError>;
}

impl TraversalValueTestExt for TraversalValue<'_> {
    fn check_property(&self, key: &str) -> Result<&Value, GraphError> {
        match self {
            TraversalValue::Node(node) => node.check_property(key),
            TraversalValue::Edge(edge) => edge.check_property(key),
            TraversalValue::Vector(vector) => vector.properties.as_ref().and_then(|p| p.get(key)).ok_or_else(|| GraphError::New(format!("Property {key} not found"))),
            TraversalValue::VectorNodeWithoutVectorData(vector) => vector.properties.as_ref().and_then(|p| p.get(key)).ok_or_else(|| GraphError::New(format!("Property {key} not found"))),
            _ => Err(GraphError::New(format!("Property {key} not available"))),
        }
    }
}
