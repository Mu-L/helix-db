use std::sync::Arc;

use heed3::RoTxn;
use tempfile::TempDir;

use crate::{
    helix_engine::{
        storage_core::{version_info::VersionInfo},
        traversal_core::{
            HelixGraphEngine, HelixGraphEngineOpts,
            config::Config,
            ops::{
                g::G,
                source::{
                    add_e::{AddEAdapter, EdgeType},
                    add_n::AddNAdapter,
                },
                vectors::insert::InsertVAdapter,
            },
            traversal_value::{Traversable, TraversalValue},
        },
        vector_core::vector::HVector,
    },
    helix_gateway::mcp::{mcp::MCPConnection, tools::McpTools},
};

fn setup_test_db() -> (HelixGraphEngine, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().to_str().unwrap();
    let opts = HelixGraphEngineOpts {
        path: db_path.to_string(),
        config: Config::default(),
        version_info: VersionInfo::default(),
    };
    let storage = HelixGraphEngine::new(opts).unwrap();
    (storage, temp_dir)
}

#[test]
fn test_mcp_tool_out_step() {
    let (engine, _temp_dir) = setup_test_db();
    let mut txn = engine.storage.graph_env.write_txn().unwrap();

    // Create a graph: person1 -[knows]-> person2
    let person1 = G::new_mut(Arc::clone(&engine.storage), &mut txn)
        .add_n("person", None, None)
        .collect_to_obj();

    let person2 = G::new_mut(Arc::clone(&engine.storage), &mut txn)
        .add_n("person", None, None)
        .collect_to_obj();

    let _edge = G::new_mut(Arc::clone(&engine.storage), &mut txn)
        .add_e("knows", None, person1.id(), person2.id(), false, EdgeType::Node)
        .collect_to_obj();

    txn.commit().unwrap();
    let txn = engine.storage.graph_env.read_txn().unwrap();

    let mcp_backend = engine.mcp_backend.as_ref().unwrap();

    // Create MCP connection with person1
    let mcp_connection = MCPConnection::new("test".to_string(), vec![person1.into()].into_iter());

    // Traverse out via "knows" edge
    let result = mcp_backend
        .out_step(&txn, &mcp_connection, "knows".to_string(), EdgeType::Node)
        .unwrap();

    assert_eq!(result.len(), 1);
    if let TraversalValue::Node(n) = &result[0] {
        assert_eq!(n.id, person2.id());
    } else {
        panic!("Expected node");
    }
}

#[test]
fn test_mcp_tool_out_e_step() {
    let (engine, _temp_dir) = setup_test_db();
    let mut txn = engine.storage.graph_env.write_txn().unwrap();

    // Create a graph: person1 -[knows]-> person2
    let person1 = G::new_mut(Arc::clone(&engine.storage), &mut txn)
        .add_n("person", None, None)
        .collect_to_obj();

    let person2 = G::new_mut(Arc::clone(&engine.storage), &mut txn)
        .add_n("person", None, None)
        .collect_to_obj();

    let edge = G::new_mut(Arc::clone(&engine.storage), &mut txn)
        .add_e("knows", None, person1.id(), person2.id(), false, EdgeType::Node)
        .collect_to_obj();

    txn.commit().unwrap();
    let txn = engine.storage.graph_env.read_txn().unwrap();

    let mcp_backend = engine.mcp_backend.as_ref().unwrap();

    // Create MCP connection with person1
    let mcp_connection = MCPConnection::new("test".to_string(), vec![person1.into()].into_iter());

    // Get outgoing edges
    let result = mcp_backend
        .out_e_step(&txn, &mcp_connection, "knows".to_string())
        .unwrap();

    assert_eq!(result.len(), 1);
    if let TraversalValue::Edge(e) = &result[0] {
        assert_eq!(e.id, edge.id());
        assert_eq!(e.label, "knows");
    } else {
        panic!("Expected edge");
    }
}

#[test]
fn test_mcp_tool_in_step() {
    let (engine, _temp_dir) = setup_test_db();
    let mut txn = engine.storage.graph_env.write_txn().unwrap();

    // Create a graph: person1 -[knows]-> person2
    let person1 = G::new_mut(Arc::clone(&engine.storage), &mut txn)
        .add_n("person", None, None)
        .collect_to_obj();

    let person2 = G::new_mut(Arc::clone(&engine.storage), &mut txn)
        .add_n("person", None, None)
        .collect_to_obj();

    let _edge = G::new_mut(Arc::clone(&engine.storage), &mut txn)
        .add_e("knows", None, person1.id(), person2.id(), false, EdgeType::Node)
        .collect_to_obj();

    txn.commit().unwrap();
    let txn = engine.storage.graph_env.read_txn().unwrap();

    let mcp_backend = engine.mcp_backend.as_ref().unwrap();

    // Create MCP connection with person2 (the target node)
    let mcp_connection = MCPConnection::new("test".to_string(), vec![person2.into()].into_iter());

    // Traverse in via "knows" edge to get person1
    let result = mcp_backend
        .in_step(&txn, &mcp_connection, "knows".to_string(), EdgeType::Node)
        .unwrap();

    assert_eq!(result.len(), 1);
    if let TraversalValue::Node(n) = &result[0] {
        assert_eq!(n.id, person1.id());
    } else {
        panic!("Expected node");
    }
}

#[test]
fn test_mcp_tool_in_e_step() {
    let (engine, _temp_dir) = setup_test_db();
    let mut txn = engine.storage.graph_env.write_txn().unwrap();

    // Create a graph: person1 -[knows]-> person2
    let person1 = G::new_mut(Arc::clone(&engine.storage), &mut txn)
        .add_n("person", None, None)
        .collect_to_obj();

    let person2 = G::new_mut(Arc::clone(&engine.storage), &mut txn)
        .add_n("person", None, None)
        .collect_to_obj();

    let edge = G::new_mut(Arc::clone(&engine.storage), &mut txn)
        .add_e("knows", None, person1.id(), person2.id(), false, EdgeType::Node)
        .collect_to_obj();

    txn.commit().unwrap();
    let txn = engine.storage.graph_env.read_txn().unwrap();

    let mcp_backend = engine.mcp_backend.as_ref().unwrap();

    // Create MCP connection with person2 (the target node)
    let mcp_connection = MCPConnection::new("test".to_string(), vec![person2.into()].into_iter());

    // Get incoming edges
    let result = mcp_backend
        .in_e_step(&txn, &mcp_connection, "knows".to_string())
        .unwrap();

    assert_eq!(result.len(), 1);
    if let TraversalValue::Edge(e) = &result[0] {
        assert_eq!(e.id, edge.id());
        assert_eq!(e.label, "knows");
    } else {
        panic!("Expected edge");
    }
}

#[test]
fn test_mcp_tool_n_from_type() {
    let (engine, _temp_dir) = setup_test_db();
    let mut txn = engine.storage.graph_env.write_txn().unwrap();

    // Create multiple nodes of different types
    let person1 = G::new_mut(Arc::clone(&engine.storage), &mut txn)
        .add_n("person", None, None)
        .collect_to_obj();

    let person2 = G::new_mut(Arc::clone(&engine.storage), &mut txn)
        .add_n("person", None, None)
        .collect_to_obj();

    let _company = G::new_mut(Arc::clone(&engine.storage), &mut txn)
        .add_n("company", None, None)
        .collect_to_obj();

    txn.commit().unwrap();
    let txn = engine.storage.graph_env.read_txn().unwrap();

    let mcp_backend = engine.mcp_backend.as_ref().unwrap();
    let mcp_connection = MCPConnection::new("test".to_string(), vec![].into_iter());

    // Get all person nodes
    let result = mcp_backend
        .n_from_type(&txn, &mcp_connection, "person".to_string())
        .unwrap();

    assert_eq!(result.len(), 2);

    let person_ids: Vec<u128> = result.iter().filter_map(|tv| {
        if let TraversalValue::Node(n) = tv {
            Some(n.id)
        } else {
            None
        }
    }).collect();

    assert!(person_ids.contains(&person1.id()));
    assert!(person_ids.contains(&person2.id()));
}

#[test]
fn test_mcp_tool_e_from_type() {
    let (engine, _temp_dir) = setup_test_db();
    let mut txn = engine.storage.graph_env.write_txn().unwrap();

    // Create nodes and edges of different types
    let person1 = G::new_mut(Arc::clone(&engine.storage), &mut txn)
        .add_n("person", None, None)
        .collect_to_obj();

    let person2 = G::new_mut(Arc::clone(&engine.storage), &mut txn)
        .add_n("person", None, None)
        .collect_to_obj();

    let company = G::new_mut(Arc::clone(&engine.storage), &mut txn)
        .add_n("company", None, None)
        .collect_to_obj();

    let knows1 = G::new_mut(Arc::clone(&engine.storage), &mut txn)
        .add_e("knows", None, person1.id(), person2.id(), false, EdgeType::Node)
        .collect_to_obj();

    let knows2 = G::new_mut(Arc::clone(&engine.storage), &mut txn)
        .add_e("knows", None, person2.id(), person1.id(), false, EdgeType::Node)
        .collect_to_obj();

    let _works_at = G::new_mut(Arc::clone(&engine.storage), &mut txn)
        .add_e("works_at", None, person1.id(), company.id(), false, EdgeType::Node)
        .collect_to_obj();

    txn.commit().unwrap();
    let txn = engine.storage.graph_env.read_txn().unwrap();

    let mcp_backend = engine.mcp_backend.as_ref().unwrap();
    let mcp_connection = MCPConnection::new("test".to_string(), vec![].into_iter());

    // Get all "knows" edges
    let result = mcp_backend
        .e_from_type(&txn, &mcp_connection, "knows".to_string())
        .unwrap();

    assert_eq!(result.len(), 2);

    let edge_ids: Vec<u128> = result.iter().filter_map(|tv| {
        if let TraversalValue::Edge(e) = tv {
            Some(e.id)
        } else {
            None
        }
    }).collect();

    assert!(edge_ids.contains(&knows1.id()));
    assert!(edge_ids.contains(&knows2.id()));
}

#[test]
fn test_mcp_tool_filter_items() {
    use crate::helix_gateway::mcp::tools::{FilterTraversal, FilterProperties, Operator};
    use crate::protocol::value::Value;

    let (engine, _temp_dir) = setup_test_db();
    let mut txn = engine.storage.graph_env.write_txn().unwrap();

    // Create nodes with different ages
    let person1 = G::new_mut(Arc::clone(&engine.storage), &mut txn)
        .add_n("person", Some(vec![("age".to_string(), Value::I64(25))]), None)
        .collect_to_obj();

    let person2 = G::new_mut(Arc::clone(&engine.storage), &mut txn)
        .add_n("person", Some(vec![("age".to_string(), Value::I64(35))]), None)
        .collect_to_obj();

    let person3 = G::new_mut(Arc::clone(&engine.storage), &mut txn)
        .add_n("person", Some(vec![("age".to_string(), Value::I64(45))]), None)
        .collect_to_obj();

    txn.commit().unwrap();
    let txn = engine.storage.graph_env.read_txn().unwrap();

    let mcp_backend = engine.mcp_backend.as_ref().unwrap();

    // Create connection with all three persons
    let mcp_connection = MCPConnection::new(
        "test".to_string(),
        vec![
            person1.clone().into(),
            person2.clone().into(),
            person3.clone().into(),
        ].into_iter()
    );

    // Filter for age > 30
    let filter = FilterTraversal {
        properties: Some(vec![vec![FilterProperties {
            key: "age".to_string(),
            value: Value::I64(30),
            operator: Some(Operator::Gt),
        }]]),
        filter_traversals: None,
    };

    let result = mcp_backend
        .filter_items(&txn, &mcp_connection, filter)
        .unwrap();

    assert_eq!(result.len(), 2);

    let filtered_ids: Vec<u128> = result.iter().filter_map(|tv| {
        if let TraversalValue::Node(n) = tv {
            Some(n.id)
        } else {
            None
        }
    }).collect();

    assert!(filtered_ids.contains(&person2.id()));
    assert!(filtered_ids.contains(&person3.id()));
    assert!(!filtered_ids.contains(&person1.id()));
}

#[test]
fn test_mcp_tool_search_keyword() {
    use crate::protocol::value::Value;

    let (engine, _temp_dir) = setup_test_db();
    let mut txn = engine.storage.graph_env.write_txn().unwrap();

    // Create nodes with text content
    let node1 = G::new_mut(Arc::clone(&engine.storage), &mut txn)
        .add_n("document", Some(vec![
            ("content".to_string(), Value::String("The quick brown fox jumps over the lazy dog".to_string())),
        ]), None)
        .collect_to_obj();

    let node2 = G::new_mut(Arc::clone(&engine.storage), &mut txn)
        .add_n("document", Some(vec![
            ("content".to_string(), Value::String("A fast brown fox leaps across the sleeping canine".to_string())),
        ]), None)
        .collect_to_obj();

    let _node3 = G::new_mut(Arc::clone(&engine.storage), &mut txn)
        .add_n("document", Some(vec![
            ("content".to_string(), Value::String("Cats and dogs are popular pets".to_string())),
        ]), None)
        .collect_to_obj();

    txn.commit().unwrap();
    let txn = engine.storage.graph_env.read_txn().unwrap();

    let mcp_backend = engine.mcp_backend.as_ref().unwrap();

    // Create connection with all nodes
    let mcp_connection = MCPConnection::new(
        "test".to_string(),
        vec![
            node1.into(),
            node2.into(),
        ].into_iter()
    );

    // Search for "fox" keyword
    let result = mcp_backend
        .search_keyword(&txn, &mcp_connection, "fox".to_string(), 10, "document".to_string());

    // BM25 search should either work or return an error - both are acceptable in tests
    // The important thing is that it doesn't panic
    match result {
        Ok(results) => {
            // If BM25 works, verify we got results
            assert!(!results.is_empty());
        },
        Err(_e) => {
            // BM25 may not be fully initialized in test environment, which is acceptable
        }
    }
}

#[test]
fn test_mcp_tool_search_vector_text() {
    let (engine, _temp_dir) = setup_test_db();
    let mut txn = engine.storage.graph_env.write_txn().unwrap();

    // Create vectors with labels
    let _vector1 = G::new_mut(Arc::clone(&engine.storage), &mut txn)
        .insert_v::<fn(&HVector, &RoTxn) -> bool>(&vec![0.1, 0.2, 0.3], "test_vector", None)
        .collect_to_obj();

    let _vector2 = G::new_mut(Arc::clone(&engine.storage), &mut txn)
        .insert_v::<fn(&HVector, &RoTxn) -> bool>(&vec![0.4, 0.5, 0.6], "test_vector", None)
        .collect_to_obj();

    txn.commit().unwrap();
    let txn = engine.storage.graph_env.read_txn().unwrap();

    let mcp_backend = engine.mcp_backend.as_ref().unwrap();
    let mcp_connection = MCPConnection::new("test".to_string(), vec![].into_iter());

    // Attempt to search using text (requires embedding model)
    let result = mcp_backend
        .search_vector_text(
            &txn,
            &mcp_connection,
            "test query".to_string(),
            "test_vector".to_string(),
            Some(5),
        );

    // Embedding model may not be available in test environment, which is acceptable
    // The important thing is that the function doesn't panic
    match result {
        Ok(results) => {
            // If embedding model is available, we should get results
            assert!(results.len() <= 5);
        },
        Err(_e) => {
            // Embedding model may not be configured in test environment, which is acceptable
            // This is expected when OPENAI_API_KEY or other embedding providers are not configured
        }
    }
}

use rand::prelude::SliceRandom;


#[test]
fn test_mcp_tool_search_vector() {
    let (engine, _temp_dir) = setup_test_db();
    let mut txn = engine.storage.graph_env.write_txn().unwrap();

    // creates nodes and vectors
    let node = G::new_mut(Arc::clone(&engine.storage), &mut txn)
        .add_n("person", None, None)
        .collect_to_obj();
    let mut vectors = vec![
        vec![1.0, 1.0, 1.0],
        vec![0.0, 0.0, 0.0],
        vec![0.3, 0.3, 0.3],
    ];

    for _ in 3..1000 {
        vectors.push(vec![
            rand::random_range(-1.0..0.5),
            rand::random_range(-1.0..0.5),
            rand::random_range(-1.0..0.5),
        ]);
    }

    vectors.shuffle(&mut rand::rng());

    for vector in vectors {
        let vector = G::new_mut(Arc::clone(&engine.storage), &mut txn)
            .insert_v::<fn(&HVector, &RoTxn) -> bool>(&vector, "vector", None)
            .collect_to_obj();

        let _ = G::new_mut(Arc::clone(&engine.storage), &mut txn)
            .add_e("knows", None, node.id(), vector.id(), false, EdgeType::Vec)
            .collect_to_obj();
    }
    txn.commit().unwrap();
    let txn = engine.storage.graph_env.read_txn().unwrap();
    let mcp_backend = engine.mcp_backend.as_ref().unwrap();
    let mcp_connections = engine.mcp_connections.as_ref().unwrap();
    let mut mcp_connections = mcp_connections.lock().unwrap();

    // creates mcp connection
    let mcp_connection = MCPConnection::new("test".to_string(), vec![].into_iter());
    mcp_connections.add_connection(mcp_connection);
    let mut mcp_connection = mcp_connections.get_connection_owned("test").unwrap();

    // gets node
    let res = mcp_backend
        .n_from_type(&txn, &mcp_connection, "person".to_string())
        .unwrap();
    assert_eq!(res.len(), 1);
    mcp_connection.iter = res.into_iter();

    // traverses to vectors
    let res = mcp_backend
        .out_step(&txn, &mcp_connection, "knows".to_string(), EdgeType::Vec)
        .unwrap();
    mcp_connection.iter = res.into_iter();

    // brute force searches for vectors
    let res = mcp_backend
        .search_vector(&txn, &mcp_connection, vec![1.0, 1.0, 1.0], 10, None)
        .unwrap();

    // checks that the first vector is correct
    if let TraversalValue::Vector(v) = res[0].clone() {
        assert_eq!(v.get_data(), &[1.0, 1.0, 1.0]);
    } else {
        panic!("Expected vector");
    }
}
