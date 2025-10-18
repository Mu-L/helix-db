//! Node and Edge types for the graph.
//!
//! Nodes are the main entities in the graph and edges are the connections between them.
//!
//! Nodes and edges are serialised without enum variant names in JSON format.

use crate::protocol::value::Value;
use crate::{helix_engine::types::GraphError, utils::properties::ImmutablePropertiesMap};
use sonic_rs::{Deserialize, Serialize};
use std::{cmp::Ordering, marker::PhantomData};

/// A node in the graph containing an ID, label, and property map.
/// Properties are serialised without enum variant names in JSON format.
#[derive(Serialize, Deserialize, PartialEq)]
pub struct Node<'arena> {
    /// The ID of the node.
    ///
    /// This is not serialized when stored as it is the key.
    #[serde(skip)]
    pub id: u128,
    /// The label of the node.
    pub label: bumpalo::collections::String<'arena>,
    /// The version of the node.
    #[serde(default)]
    pub version: u8,
    /// The properties of the node.
    ///
    /// Properties are optional and can be None.
    /// Properties are serialised without enum variant names in JSON format.
    #[serde(default)]
    pub properties: Option<ImmutablePropertiesMap<'arena>>,

    #[serde(skip)]
    pub _phantom: PhantomData<&'arena ()>,
}

impl<'arena> Node<'arena> {
    /// The number of properties in a node.
    ///
    /// This is used as a constant in the return value mixin methods.
    pub const NUM_PROPERTIES: usize = 2;

    /// Decodes a node from a byte slice.
    ///
    /// Takes ID as the ID is not serialized when stored as it is the key.
    /// Uses the known ID (either from the query or the key in an LMDB iterator) to construct a new node.
    pub fn decode_node<'s>(
        bytes: &'s [u8],
        id: u128,
        _arena: &'arena bumpalo::Bump,
    ) -> Result<Node<'arena>, GraphError> {
        match bincode::deserialize::<Node<'arena>>(bytes) {
            Ok(node) => Ok(Node { id, ..node }),
            Err(e) => Err(GraphError::ConversionError(format!(
                "Error deserializing node: {e}"
            ))),
        }
    }

    /// Encodes a node into a byte slice
    ///
    /// This skips the ID and if the properties are None, it skips the properties.
    pub fn encode_node(&self) -> Result<Vec<u8>, GraphError> {
        bincode::serialize(&self)
            .map_err(|e| GraphError::ConversionError(format!("Error serializing node: {e}")))
    }

    #[inline(always)]
    pub fn get_property(&self, prop: &str) -> Option<&Value> {
        self.properties.as_ref().and_then(|value| value.get(prop))
    }
}

// Core trait implementations for Node
impl std::fmt::Display for Node<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{{ id: {}, label: {} }}",
            uuid::Uuid::from_u128(self.id),
            self.label,
        )
    }
}
impl std::fmt::Debug for Node<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{{ \nid:{},\nlabel:{} }}",
            uuid::Uuid::from_u128(self.id),
            self.label,
        )
    }
}
impl Eq for Node<'_> {}
impl Ord for Node<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.id.cmp(&other.id)
    }
}
impl PartialOrd for Node<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// An edge in the graph connecting two nodes with an ID, label, and property map.
/// Properties are serialised without enum variant names in JSON format.
#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub struct Edge<'arena> {
    /// The ID of the edge.
    ///
    /// This is not serialized when stored as it is the key.
    #[serde(skip)]
    pub id: u128,
    /// The label of the edge.
    pub label: bumpalo::collections::String<'arena>,
    /// The version of the edge.
    #[serde(default)]
    pub version: u8,
    /// The ID of the from node.
    pub from_node: u128,
    /// The ID of the to node.
    pub to_node: u128,
    /// The properties of the edge.
    ///
    /// Properties are optional and can be None.
    /// Properties are serialised without enum variant names in JSON format.
    #[serde(default)]
    pub properties: Option<ImmutablePropertiesMap<'arena>>,

    pub _phantom: PhantomData<&'arena ()>,
}

impl<'arena> Edge<'arena> {
    /// The number of properties in an edge.
    ///
    /// This is used as a constant in the return value mixin methods.
    pub const NUM_PROPERTIES: usize = 4;

    /// Decodes an edge from a byte slice.
    ///
    /// Takes ID as the ID is not serialized when stored as it is the key.
    /// Uses the known ID (either from the query or the key in an LMDB iterator) to construct a new edge.
    pub fn decode_edge(
        bytes: &[u8],
        id: u128,
        _arena: &'arena bumpalo::Bump,
    ) -> Result<Edge<'arena>, GraphError> {
        match bincode::deserialize::<Edge<'arena>>(bytes) {
            Ok(edge) => Ok(Edge { id, ..edge }),
            Err(e) => Err(GraphError::ConversionError(format!(
                "Error deserializing edge: {e}"
            ))),
        }
    }

    /// Encodes an edge into a byte slice
    ///
    /// This skips the ID and if the properties are None, it skips the properties.
    pub fn encode_edge(&self) -> Result<Vec<u8>, GraphError> {
        bincode::serialize(self)
            .map_err(|e| GraphError::ConversionError(format!("Error serializing edge: {e}")))
    }

    pub fn get_property(&self, prop: &str) -> Option<&Value> {
        self.properties.as_ref().and_then(|value| value.get(prop))
    }
}

// Core trait implementations for Edge
impl std::fmt::Display for Edge<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{{ id: {}, label: {}, from_node: {}, to_node: {}}}",
            uuid::Uuid::from_u128(self.id),
            self.label,
            uuid::Uuid::from_u128(self.from_node),
            uuid::Uuid::from_u128(self.to_node),
        )
    }
}
impl std::fmt::Debug for Edge<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{{ \nid: {},\nlabel: {},\nfrom_node: {},\nto_node: {}}}",
            uuid::Uuid::from_u128(self.id),
            self.label,
            uuid::Uuid::from_u128(self.from_node),
            uuid::Uuid::from_u128(self.to_node),
        )
    }
}
impl Eq for Edge<'_> {}
impl Ord for Edge<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.id.cmp(&other.id)
    }
}
impl PartialOrd for Edge<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use bumpalo::Bump;

    use super::*;

    // Helper function to create a test node
    fn create_test_node(id: u128, label: &str, props: Option<HashMap<String, Value>>) -> Node {
        Node {
            id,
            label,
            version: 0,
            properties: props,
            _phantom: PhantomData,
        }
    }

    // Helper function to create a test edge
    fn create_test_edge(
        id: u128,
        label: &str,
        from: u128,
        to: u128,
        props: Option<HashMap<String, Value>>,
    ) -> Edge {
        Edge {
            id,
            label: label.to_string(),
            version: 0,
            from_node: from,
            to_node: to,
            properties: props,
            _phantom: PhantomData,
        }
    }

    // Node encoding/decoding tests

    #[test]
    fn test_node_encode_decode_roundtrip_basic() {
        let props = Some(HashMap::from([
            ("name".to_string(), Value::String("Alice".to_string())),
            ("age".to_string(), Value::I32(30)),
        ]));
        let arena = Bump::new();

        let node = create_test_node(12345, "person", props);
        let encoded = node.encode_node().unwrap();
        let decoded = Node::decode_node(&encoded, 12345, &arena).unwrap();

        assert_eq!(node.id, decoded.id);
        assert_eq!(node.label, decoded.label);
        assert_eq!(node.properties, decoded.properties);
    }

    #[test]
    fn test_node_encode_decode_empty_properties() {
        let arena = Bump::new();
        let node = create_test_node(123, "empty", None);
        let encoded = node.encode_node().unwrap();
        let decoded = Node::decode_node(&encoded, 123, &arena).unwrap();

        assert_eq!(node.id, decoded.id);
        assert_eq!(node.label, decoded.label);
        assert_eq!(decoded.properties, None);
    }

    #[test]
    fn test_node_encode_decode_all_value_types() {
        let arena = Bump::new();
        let props = Some(HashMap::from([
            ("string".to_string(), Value::String("test".to_string())),
            ("i32".to_string(), Value::I32(42)),
            ("i64".to_string(), Value::I64(123456789)),
            ("u64".to_string(), Value::U64(987654321)),
            ("f64".to_string(), Value::F64(3.14159)),
            ("bool".to_string(), Value::Boolean(true)),
        ]));

        let node = create_test_node(456, "test_node", props);
        let encoded = node.encode_node().unwrap();
        let decoded = Node::decode_node(&encoded, 456, &arena).unwrap();

        assert_eq!(node.properties, decoded.properties);
    }

    #[test]
    fn test_node_encode_decode_large_properties() {
        // Test with many properties (100+)
        let arena = Bump::new();
        let mut props_map = HashMap::new();
        for i in 0..150 {
            props_map.insert(format!("prop_{}", i), Value::String(format!("value_{}", i)));
        }

        let node = create_test_node(789, "large_node", Some(props_map.clone()));
        let encoded = node.encode_node().unwrap();
        let decoded = Node::decode_node(&encoded, 789, &arena).unwrap();

        assert_eq!(
            node.properties.unwrap().len(),
            decoded.properties.unwrap().len()
        );
    }

    #[test]
    fn test_node_encode_decode_long_strings() {
        // Test with very long property values
        let long_string = "a".repeat(10_000);
        let arena = Bump::new();
        let props = Some(HashMap::from([(
            "long_value".to_string(),
            Value::String(long_string.clone()),
        )]));

        let node = create_test_node(999, "long_string_node", props);
        let encoded = node.encode_node().unwrap();
        let decoded = Node::decode_node(&encoded, 999, &arena).unwrap();

        match &decoded.properties {
            Some(p) => {
                if let Some(Value::String(s)) = p.get("long_value") {
                    assert_eq!(s.len(), 10_000);
                    assert_eq!(s, &long_string);
                } else {
                    panic!("Expected String value");
                }
            }
            None => panic!("Expected properties"),
        }
    }

    #[test]
    fn test_node_encode_decode_utf8_strings() {
        let arena = Bump::new();
        let props = Some(HashMap::from([
            ("chinese".to_string(), Value::String("你好世界".to_string())),
            ("emoji".to_string(), Value::String("🚀🎉🔥".to_string())),
            ("arabic".to_string(), Value::String("مرحبا".to_string())),
        ]));

        let node = create_test_node(111, "utf8_node", props);
        let encoded = node.encode_node().unwrap();
        let decoded = Node::decode_node(&encoded, 111, &arena).unwrap();

        assert_eq!(node.properties, decoded.properties);
    }

    #[test]
    fn test_node_decode_with_different_id() {
        // Test that decode properly uses the provided ID
        let arena = Bump::new();
        let node = create_test_node(100, "person", None);
        let encoded = node.encode_node().unwrap();

        // Decode with different ID
        let decoded = Node::decode_node(&encoded, 200, &arena).unwrap();

        assert_eq!(decoded.id, 200); // Should use the provided ID
        assert_eq!(decoded.label, node.label);
    }

    #[test]
    fn test_node_decode_malformed_data() {
        // Test decoding with invalid/malformed data
        let arena = Bump::new();
        let bad_data = vec![1, 2, 3, 4, 5];
        let result = Node::decode_node(&bad_data, 123, &arena);

        assert!(result.is_err(), "Should fail to decode malformed data");
    }

    // Edge encoding/decoding tests

    #[test]
    fn test_edge_encode_decode_roundtrip_basic() {
        let arena = Bump::new();
        let props = Some(HashMap::from([
            ("weight".to_string(), Value::F64(0.75)),
            ("since".to_string(), Value::String("2020-01-01".to_string())),
        ]));

        let edge = create_test_edge(1, "knows", 100, 200, props);
        let encoded = edge.encode_edge().unwrap();
        let decoded = Edge::decode_edge(&encoded, 1, &arena).unwrap();

        assert_eq!(edge.id, decoded.id);
        assert_eq!(edge.label, decoded.label);
        assert_eq!(edge.from_node, decoded.from_node);
        assert_eq!(edge.to_node, decoded.to_node);
        assert_eq!(edge.properties, decoded.properties);
    }

    #[test]
    fn test_edge_encode_decode_empty_properties() {
        let arena = Bump::new();
        let edge = create_test_edge(2, "follows", 300, 400, None);
        let encoded = edge.encode_edge().unwrap();
        let decoded = Edge::decode_edge(&encoded, 2, &arena).unwrap();

        assert_eq!(edge.id, decoded.id);
        assert_eq!(edge.from_node, decoded.from_node);
        assert_eq!(edge.to_node, decoded.to_node);
        assert_eq!(decoded.properties, None);
    }

    #[test]
    fn test_edge_encode_decode_all_value_types() {
        let arena = Bump::new();
        let props = Some(HashMap::from([
            ("string".to_string(), Value::String("edge_prop".to_string())),
            ("number".to_string(), Value::I32(99)),
            ("float".to_string(), Value::F64(2.718)),
            ("bool".to_string(), Value::Boolean(false)),
        ]));

        let edge = create_test_edge(3, "likes", 500, 600, props);
        let encoded = edge.encode_edge().unwrap();
        let decoded = Edge::decode_edge(&encoded, 3, &arena).unwrap();

        assert_eq!(edge.properties, decoded.properties);
    }

    #[test]
    fn test_edge_encode_decode_self_loop() {
        // Test edge where from_node == to_node (self-loop)
        let arena = Bump::new();
        let edge = create_test_edge(4, "self_reference", 700, 700, None);
        let encoded = edge.encode_edge().unwrap();
        let decoded = Edge::decode_edge(&encoded, 4, &arena).unwrap();

        assert_eq!(decoded.from_node, decoded.to_node);
        assert_eq!(decoded.from_node, 700);
    }

    #[test]
    fn test_edge_decode_with_different_id() {
        let arena = Bump::new();
        let edge = create_test_edge(5, "works_at", 800, 900, None);
        let encoded = edge.encode_edge().unwrap();

        // Decode with different ID
        let decoded = Edge::decode_edge(&encoded, 50, &arena).unwrap();

        assert_eq!(decoded.id, 50); // Should use the provided ID
        assert_eq!(decoded.label, edge.label);
        assert_eq!(decoded.from_node, edge.from_node);
        assert_eq!(decoded.to_node, edge.to_node);
    }

    #[test]
    fn test_edge_decode_malformed_data() {
        let arena = Bump::new();
        let bad_data = vec![1, 2, 3, 4, 5];
        let result = Edge::decode_edge(&bad_data, 123, &arena);

        assert!(result.is_err(), "Should fail to decode malformed data");
    }

    #[test]
    fn test_edge_encode_decode_large_node_ids() {
        // Test with maximum u128 values
        let arena = Bump::new();
        let max_id = u128::MAX;
        let edge = create_test_edge(6, "test", max_id - 1, max_id, None);
        let encoded = edge.encode_edge().unwrap();
        let decoded = Edge::decode_edge(&encoded, 6, &arena).unwrap();

        assert_eq!(decoded.from_node, max_id - 1);
        assert_eq!(decoded.to_node, max_id);
    }

    // Test Display and Debug implementations

    #[test]
    fn test_node_display() {
        let node = create_test_node(
            123456789,
            "test",
            Some(HashMap::from([(
                "key".to_string(),
                Value::String("value".to_string()),
            )])),
        );

        let display = format!("{}", node);
        assert!(display.contains("test"));
        assert!(display.contains("id"));
    }

    #[test]
    fn test_edge_display() {
        let edge = create_test_edge(123, "knows", 100, 200, None);

        let display = format!("{}", edge);
        assert!(display.contains("knows"));
        assert!(display.contains("from_node"));
        assert!(display.contains("to_node"));
    }

    // Test ordering implementations

    #[test]
    fn test_node_ordering() {
        let node1 = create_test_node(100, "a", None);
        let node2 = create_test_node(200, "b", None);
        let node3 = create_test_node(100, "a", None); // Same ID and label

        assert!(node1 < node2);
        assert!(node2 > node1);
        // Nodes with same ID and same data are equal
        assert_eq!(node1, node3);
        // Nodes with same ID but different data are NOT equal (PartialEq compares all fields)
        let node4 = create_test_node(100, "different", None);
        assert_ne!(node1, node4);
        // But ordering only considers ID
        assert_eq!(node1.cmp(&node4), Ordering::Equal);
    }

    #[test]
    fn test_edge_ordering() {
        let edge1 = create_test_edge(100, "a", 1, 2, None);
        let edge2 = create_test_edge(200, "b", 3, 4, None);
        let edge3 = create_test_edge(100, "a", 1, 2, None); // Same ID and data

        assert!(edge1 < edge2);
        assert!(edge2 > edge1);
        // Edges with same ID and same data are equal
        assert_eq!(edge1, edge3);
        // Edges with same ID but different data are NOT equal (PartialEq compares all fields)
        let edge4 = create_test_edge(100, "different", 5, 6, None);
        assert_ne!(edge1, edge4);
        // But ordering only considers ID
        assert_eq!(edge1.cmp(&edge4), Ordering::Equal);
    }
}
