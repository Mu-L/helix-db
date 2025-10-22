use std::sync::Arc;
use super::test_utils::props_option;

use tempfile::TempDir;
use bumpalo::Bump;
use crate::{
    helix_engine::{
        storage_core::HelixGraphStorage,
        traversal_core::{
            ops::{
                g::G,
                out::out::OutAdapter,
                source::{
                    add_e::AddEAdapter,
                    add_n::AddNAdapter,
                    n_from_type::NFromTypeAdapter,
                },
                util::range::RangeAdapter,
            },
        },
    },
    props,
};

fn setup_test_db() -> (Arc<HelixGraphStorage>, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().to_str().unwrap();
    let storage = HelixGraphStorage::new(
        db_path,
        crate::helix_engine::traversal_core::config::Config::default(),
        Default::default(),
    )
    .unwrap();
    (Arc::new(storage), temp_dir)
}

#[test]
fn test_range_subset() {
    let (storage, _temp_dir) = setup_test_db();
    let arena = Bump::new();
    let mut txn = storage.graph_env.write_txn().unwrap();

    // Create multiple nodes
    let _: Vec<_> = (0..5)
        .map(|_| {
            G::new_mut(&storage, &arena, &mut txn)
                .add_n("person", None, None)
                .collect_to::<Vec<_>>()
                .first()
                .unwrap();
        })
        .collect();

    txn.commit().unwrap();
    let txn = storage.graph_env.read_txn().unwrap();
    let count = G::new(&storage, &txn, &arena)
        .n_from_type("person") // Get all nodes
        .range(1, 3) // Take nodes at index 1 and 2
        .count();

    assert_eq!(count, 2);
}

#[test]
fn test_range_chaining() {
    let (storage, _temp_dir) = setup_test_db();
    let arena = Bump::new();
    let mut txn = storage.graph_env.write_txn().unwrap();

    // Create graph: (p1)-[knows]->(p2)-[knows]->(p3)-[knows]->(p4)-[knows]->(p5)
    let nodes: Vec<_> = (0..5)
        .map(|i| {
            G::new_mut(&storage, &arena, &mut txn)
                .add_n("person", props_option(&arena, props! { "name" => i }), None)
                .collect_to::<Vec<_>>()
                .first()
                .unwrap()
                .clone()
        })
        .collect();

    // Create edges connecting nodes sequentially
    for i in 0..4 {
        G::new_mut(&storage, &arena, &mut txn)
            .add_edge(
                "knows",
                None,
                nodes[i].id(),
                nodes[i + 1].id(),
                false,
            )
            .collect_to::<Vec<_>>();
    }

    G::new_mut(&storage, &arena, &mut txn)
        .add_edge(
            "knows",
            None,
            nodes[4].id(),
            nodes[0].id(),
            false,
        )
        .collect_to::<Vec<_>>();
    txn.commit().unwrap();
    let txn = storage.graph_env.read_txn().unwrap();
    let count = G::new(&storage, &txn, &arena)
        .n_from_type("person") // Get all nodes
        .range(0, 3) // Take first 3 nodes
        .out_node("knows") // Get their outgoing nodes
        .collect_to::<Vec<_>>();

    assert_eq!(count.len(), 3);
}

#[test]
fn test_range_empty() {
    let (storage, _temp_dir) = setup_test_db();
    let arena = Bump::new();

    let txn = storage.graph_env.read_txn().unwrap();
    let count = G::new(&storage, &txn, &arena)
        .n_from_type("person") // Get all nodes
        .range(0, 0) // Take first 3 nodes
        .collect_to::<Vec<_>>();

    assert_eq!(count.len(), 0);
}
