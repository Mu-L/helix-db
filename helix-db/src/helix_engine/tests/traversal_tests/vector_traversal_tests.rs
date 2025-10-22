use std::sync::Arc;

use bumpalo::Bump;
use heed3::RoTxn;
use tempfile::TempDir;

use super::test_utils::{g_new, g_new_mut};
use crate::helix_engine::{
    storage_core::HelixGraphStorage,
    traversal_core::{
        ops::{
            in_::{in_e::InEdgesAdapter, to_v::ToVAdapter},
            out::{out::OutAdapter, out_e::OutEdgesAdapter},
            source::{
                add_e::AddEAdapter, add_n::AddNAdapter, e_from_type::EFromTypeAdapter,
                n_from_id::NFromIdAdapter,
            },
            util::drop::Drop,
            vectors::{
                brute_force_search::BruteForceSearchVAdapter, insert::InsertVAdapter,
                search::SearchVAdapter,
            },
        },
    },
    vector_core::vector::HVector,
};

type Filter = fn(&HVector, &RoTxn) -> bool;

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
fn test_insert_and_fetch_vector() {
    let (storage, _temp_dir) = setup_test_db();
    let arena = Bump::new();
    let mut txn = storage.graph_env.write_txn().unwrap();

    let vector = g_new_mut(&storage, &arena, &mut txn)
        .insert_v::<Filter>(&[0.1, 0.2, 0.3], "embedding", None)
        .collect_to_obj();
    txn.commit().unwrap();

    let arena = Bump::new();
    let txn = storage.graph_env.read_txn().unwrap();
    let fetched = g_new(&storage, &txn, &arena)
        .e_from_type("embedding")
        .collect_to::<Vec<_>>();
    assert!(fetched.is_empty());

    let results = g_new(&storage, &txn, &arena)
        .search_v::<Filter, _>(&[0.1, 0.2, 0.3], 10, "embedding", None)
        .collect_to::<Vec<_>>();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id(), vector.id());
}

#[test]
fn test_vector_edges_from_and_to_node() {
    let (storage, _temp_dir) = setup_test_db();
    let arena = Bump::new();
    let mut txn = storage.graph_env.write_txn().unwrap();

    let node_id = g_new_mut(&storage, &arena, &mut txn)
        .add_n("person", None, None)
        .collect_to::<Vec<_>>()[0]
        .id();
    let vector_id = g_new_mut(&storage, &arena, &mut txn)
        .insert_v::<Filter>(&[1.0, 0.0, 0.0], "embedding", None)
        .collect_to_obj()
        .id();
    g_new_mut(&storage, &arena, &mut txn)
        .add_edge("has_vector", None, node_id, vector_id, false)
        .collect_to_obj();
    txn.commit().unwrap();

    let arena = Bump::new();
    let txn = storage.graph_env.read_txn().unwrap();
    let neighbors = g_new(&storage, &txn, &arena)
        .n_from_id(&node_id)
        .out_e("has_vector")
        .to_v(true)
        .collect_to::<Vec<_>>();
    assert_eq!(neighbors.len(), 1);
    assert_eq!(neighbors[0].id(), vector_id);

}

#[test]
fn test_brute_force_vector_search_orders_by_distance() {
    let (storage, _temp_dir) = setup_test_db();
    let arena = Bump::new();
    let mut txn = storage.graph_env.write_txn().unwrap();

    let node = g_new_mut(&storage, &arena, &mut txn)
        .add_n("person", None, None)
        .collect_to_obj();

    let vectors = vec![
        vec![1.0, 2.0, 3.0],
        vec![4.0, 5.0, 6.0],
        vec![7.0, 8.0, 9.0],
    ];
    let mut vector_ids = Vec::new();
    for vector in vectors {
        let vec_id = g_new_mut(&storage, &arena, &mut txn)
            .insert_v::<Filter>(&vector, "vector", None)
            .collect_to_obj()
            .id();
        g_new_mut(&storage, &arena, &mut txn)
            .add_edge("embedding", None, node.id(), vec_id, false)
            .collect_to_obj();
        vector_ids.push(vec_id);
    }
    txn.commit().unwrap();

    let arena = Bump::new();
    let txn = storage.graph_env.read_txn().unwrap();
    let traversal = g_new(&storage, &txn, &arena)
        .n_from_id(&node.id())
        .out_e("embedding")
        .to_v(true)
        .brute_force_search_v(&[1.0, 2.0, 3.0], 10)
        .collect_to::<Vec<_>>();
    assert_eq!(traversal.len(), 3);
    assert_eq!(traversal[0].id(), vector_ids[0]);
}

#[test]
fn test_drop_vector_removes_edges() {
    let (storage, _temp_dir) = setup_test_db();
    let arena = Bump::new();
    let mut txn = storage.graph_env.write_txn().unwrap();

    let node_id = g_new_mut(&storage, &arena, &mut txn)
        .add_n("person", None, None)
        .collect_to::<Vec<_>>()[0]
        .id();
    let vector_id = g_new_mut(&storage, &arena, &mut txn)
        .insert_v::<Filter>(&[0.5, 0.5, 0.5], "vector", None)
        .collect_to_obj()
        .id();
    g_new_mut(&storage, &arena, &mut txn)
        .add_edge("has_vector", None, node_id, vector_id, false)
        .collect_to_obj();
    txn.commit().unwrap();

    let arena = Bump::new();
    let txn = storage.graph_env.read_txn().unwrap();
    let vectors = g_new(&storage, &txn, &arena)
        .search_v::<Filter, _>(&[0.5, 0.5, 0.5], 10, "vector", None)
        .collect_to::<Vec<_>>();
    drop(txn);

    let mut txn = storage.graph_env.write_txn().unwrap();
    Drop::drop_traversal(
        vectors.into_iter().map(Ok::<_, crate::helix_engine::types::GraphError>),
        storage.as_ref(),
        &mut txn,
    )
    .unwrap();
    txn.commit().unwrap();

    let arena = Bump::new();
    let txn = storage.graph_env.read_txn().unwrap();
    let remaining = g_new(&storage, &txn, &arena)
        .n_from_id(&node_id)
        .out_vec("has_vector", false)
        .collect_to::<Vec<_>>();
    assert!(remaining.is_empty());
}
