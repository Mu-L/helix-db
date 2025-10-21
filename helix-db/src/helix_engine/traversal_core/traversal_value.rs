use crate::{
    helix_engine::vector_core::{vector::HVector, vector_without_data::VectorWithoutData},
    protocol::value::Value,
    utils::{
        count::Count,
        items::{Edge, Node},
    },
};
use std::{borrow::Cow, hash::Hash};

pub type Variable<'arena> = Cow<'arena, TraversalValue<'arena>>;

#[derive(Clone, Debug)]
pub enum TraversalValue<'arena> {
    /// A node in the graph
    Node(Node<'arena>),
    /// An edge in the graph
    Edge(Edge<'arena>),
    /// A vector in the graph
    Vector(HVector<'arena>),
    /// Vector node without vector data
    VectorNodeWithoutVectorData(VectorWithoutData<'arena>),
    /// A count of the number of items
    Count(Count),
    /// A path between two nodes in the graph
    Path((Vec<Node<'arena>>, Vec<Edge<'arena>>)),
    /// A value in the graph
    Value(Value),
    /// An empty traversal value
    Empty,
}

impl<'arena> TraversalValue<'arena> {
    pub fn id(&self) -> u128 {
        match self {
            TraversalValue::Node(node) => node.id,
            TraversalValue::Edge(edge) => edge.id,
            TraversalValue::Vector(vector) => vector.id,
            TraversalValue::VectorNodeWithoutVectorData(vector) => vector.id,
            TraversalValue::Empty => 0,
            _ => 0,
        }
    }

    pub fn get_property(&self, property: &str) -> Option<&Value> {
        match self {
            TraversalValue::Node(node) => node.get_property(property),
            TraversalValue::Edge(edge) => edge.get_property(property),
            TraversalValue::Vector(vector) => vector.get_property(property),
            TraversalValue::VectorNodeWithoutVectorData(vector) => vector.get_property(property),
            TraversalValue::Empty => None,
            _ => None,
        }
    }
}

impl Hash for TraversalValue<'_> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self {
            TraversalValue::Node(node) => node.id.hash(state),
            TraversalValue::Edge(edge) => edge.id.hash(state),
            TraversalValue::Vector(vector) => vector.id.hash(state),
            TraversalValue::VectorNodeWithoutVectorData(vector) => vector.id.hash(state),
            TraversalValue::Empty => state.write_u8(0),
            _ => state.write_u8(0),
        }
    }
}

impl Eq for TraversalValue<'_> {}
impl PartialEq for TraversalValue<'_> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (TraversalValue::Node(node1), TraversalValue::Node(node2)) => node1.id == node2.id,
            (TraversalValue::Edge(edge1), TraversalValue::Edge(edge2)) => edge1.id == edge2.id,
            (TraversalValue::Vector(vector1), TraversalValue::Vector(vector2)) => {
                vector1.id() == vector2.id()
            }
            (
                TraversalValue::VectorNodeWithoutVectorData(vector1),
                TraversalValue::VectorNodeWithoutVectorData(vector2),
            ) => vector1.id() == vector2.id(),
            (
                TraversalValue::Vector(vector1),
                TraversalValue::VectorNodeWithoutVectorData(vector2),
            ) => vector1.id() == vector2.id(),
            (
                TraversalValue::VectorNodeWithoutVectorData(vector1),
                TraversalValue::Vector(vector2),
            ) => vector1.id() == vector2.id(),
            (TraversalValue::Empty, TraversalValue::Empty) => true,
            _ => false,
        }
    }
}
