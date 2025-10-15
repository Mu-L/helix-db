use heed3::RoTxn;
use itertools::Itertools;

use crate::{
    helix_engine::{
        storage_core::HelixGraphStorage, types::GraphError, vector_core::arena::vector::HVector as ArenaHVector,
    },
    protocol::value::Value,
    utils::{
        count::Count,
        filterable::Filterable,
        items::{Edge, Node},
    },
};
use std::{borrow::Cow, collections::HashMap, hash::Hash};

#[derive(Clone, Debug)]
pub enum TraversalValueArena<'a> {
    /// A node in the graph
    Node(Node),
    /// An edge in the graph
    Edge(Edge),
    /// A vector in the graph
    Vector(ArenaHVector<'a>),
    /// A count of the number of items
    Count(Count),
    /// A path between two nodes in the graph
    Path((Vec<Node>, Vec<Edge>)),
    /// A value in the graph
    Value(Value),
    /// An empty traversal value
    Empty,
}

impl Hash for TraversalValueArena<'_> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self {
            TraversalValueArena::Node(node) => node.id.hash(state),
            TraversalValueArena::Edge(edge) => edge.id.hash(state),
            TraversalValueArena::Vector(vector) => vector.id.hash(state),
            TraversalValueArena::Empty => state.write_u8(0),
            _ => state.write_u8(0),
        }
    }
}

impl Eq for TraversalValueArena<'_> {}
impl PartialEq for TraversalValueArena<'_> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (TraversalValueArena::Node(node1), TraversalValueArena::Node(node2)) => {
                node1.id == node2.id
            }
            (TraversalValueArena::Edge(edge1), TraversalValueArena::Edge(edge2)) => {
                edge1.id == edge2.id
            }
            (TraversalValueArena::Vector(vector1), TraversalValueArena::Vector(vector2)) => {
                vector1.id() == vector2.id()
            }
            (TraversalValueArena::Empty, TraversalValueArena::Empty) => true,
            _ => false,
        }
    }
}

impl<'a> IntoIterator for TraversalValueArena<'a> {
    type Item = TraversalValueArena<'a>;
    type IntoIter = std::vec::IntoIter<Self::Item>;
    fn into_iter(self) -> Self::IntoIter {
        vec![self].into_iter()
    }
}

pub enum TraversableType {
    Value,
    Vec,
}

/// A trait for all traversable values in the graph
///
/// This trait is used to define the common methods for all traversable values in the graph so we don't need to write match statements to access id's and properties every time.
pub trait Traversable {
    fn id(&self) -> u128;
    fn label(&self) -> String;
    fn check_property(&self, prop: &str) -> Result<Cow<'_, Value>, GraphError>;
    fn uuid(&self) -> String;
    fn traversal_type(&self) -> TraversableType;
    fn get_properties(&self) -> &Option<HashMap<String, Value>>;
}

impl Traversable for TraversalValueArena<'_> {
    fn id(&self) -> u128 {
        match self {
            TraversalValueArena::Node(node) => node.id,
            TraversalValueArena::Edge(edge) => edge.id,
            TraversalValueArena::Vector(vector) => vector.id,
            TraversalValueArena::Value(_) => unreachable!(),
            TraversalValueArena::Empty => 0,
            t => {
                println!("invalid traversal value {t:?}");
                panic!("Invalid traversal value")
            }
        }
    }

    fn traversal_type(&self) -> TraversableType {
        TraversableType::Value
    }

    fn uuid(&self) -> String {
        match self {
            TraversalValueArena::Node(node) => uuid::Uuid::from_u128(node.id).to_string(),
            TraversalValueArena::Edge(edge) => uuid::Uuid::from_u128(edge.id).to_string(),
            TraversalValueArena::Vector(vector) => uuid::Uuid::from_u128(vector.id).to_string(),
            _ => panic!("Invalid traversal value"),
        }
    }

    fn label(&self) -> String {
        match self {
            TraversalValueArena::Node(node) => node.label.clone(),
            TraversalValueArena::Edge(edge) => edge.label.clone(),
            _ => panic!("Invalid traversal value"),
        }
    }

    fn check_property(&self, prop: &str) -> Result<Cow<'_, Value>, GraphError> {
        match self {
            TraversalValueArena::Node(node) => node.check_property(prop),
            TraversalValueArena::Edge(edge) => edge.check_property(prop),
            TraversalValueArena::Vector(vector) => vector.check_property(prop),
            _ => Err(GraphError::ConversionError(
                "Invalid traversal value".to_string(),
            )),
        }
    }

    fn get_properties(&self) -> &Option<HashMap<String, Value>> {
        match self {
            TraversalValueArena::Node(node) => &node.properties,
            TraversalValueArena::Edge(edge) => &edge.properties,
            TraversalValueArena::Vector(vector) => &vector.properties,
            _ => &None,
        }
    }
}

impl Traversable for Vec<TraversalValueArena<'_>> {
    fn id(&self) -> u128 {
        if self.is_empty() {
            return 0;
        }
        self[0].id()
    }

    fn label(&self) -> String {
        if self.is_empty() {
            return "".to_string();
        }
        self[0].label()
    }

    fn check_property(&self, prop: &str) -> Result<Cow<'_, Value>, GraphError> {
        if self.is_empty() {
            return Err(GraphError::ConversionError(
                "Invalid traversal value".to_string(),
            ));
        }
        self[0].check_property(prop)
    }

    fn get_properties(&self) -> &Option<HashMap<String, Value>> {
        if self.is_empty() {
            return &None;
        }
        self[0].get_properties()
    }

    fn uuid(&self) -> String {
        if self.is_empty() {
            return "".to_string();
        }
        self[0].uuid()
    }

    fn traversal_type(&self) -> TraversableType {
        TraversableType::Vec
    }
}

pub trait IntoTraversalValues<'a> {
    fn into(self) -> Vec<TraversalValueArena<'a>>;
}

impl<'a> IntoTraversalValues<'a> for Vec<TraversalValueArena<'a>> {
    fn into(self) -> Vec<TraversalValueArena<'a>> {
        self
    }
}

impl<'a> IntoTraversalValues<'a> for TraversalValueArena<'a> {
    fn into(self) -> Vec<TraversalValueArena<'a>> {
        vec![self]
    }
}

pub struct RoArenaTraversalIterator<'a, 'env, I> {
    pub inner: I,
    pub storage: &'env HelixGraphStorage,
    pub arena: &'a bumpalo::Bump,
    pub txn: &'a RoTxn<'a>,
}

// implementing iterator for TraversalIterator
impl<'a, 'env, I> Iterator for RoArenaTraversalIterator<'a, 'env, I>
where
    I: Iterator<Item = Result<TraversalValueArena<'a>, GraphError>>,
{
    type Item = Result<TraversalValueArena<'a>, GraphError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}

impl<'a, 'env, I: Iterator<Item = Result<TraversalValueArena<'a>, GraphError>>>
    RoArenaTraversalIterator<'a, 'env, I>
{
    pub fn take_and_collect_to<B: FromIterator<TraversalValueArena<'a>>>(self, n: usize) -> B {
        self.inner
            .filter_map(|item| item.ok())
            .take(n)
            .collect::<B>()
    }

    pub fn collect_to<B: FromIterator<TraversalValueArena<'a>>>(self) -> B {
        self.inner.filter_map(|item| item.ok()).collect::<B>()
    }

    pub fn collect_dedup<B: FromIterator<TraversalValueArena<'a>>>(self) -> B {
        self.inner
            .filter_map(|item| item.ok())
            .unique()
            .collect::<B>()
    }

    pub fn collect_to_obj(self) -> TraversalValueArena<'a> {
        match self.inner.filter_map(|item| item.ok()).next() {
            Some(val) => val,
            None => TraversalValueArena::Empty,
        }
    }

    pub fn collect_to_value(self) -> Value {
        match self.inner.filter_map(|item| item.ok()).next() {
            Some(TraversalValueArena::Value(val)) => val,
            _ => Value::Empty,
        }
    }

    pub fn map_value_or(
        mut self,
        default: bool,
        f: impl Fn(&Value) -> bool,
    ) -> Result<bool, GraphError> {
        let val = match &self.inner.next() {
            Some(Ok(TraversalValueArena::Value(val))) => Ok(f(val)),
            Some(Ok(_)) => Err(GraphError::ConversionError(
                "Expected value, got something else".to_string(),
            )),
            Some(Err(err)) => Err(GraphError::from(err.to_string())),
            None => Ok(default),
        };
        val
    }
}
