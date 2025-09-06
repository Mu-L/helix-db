use std::sync::Arc;

use crate::{
    helix_engine::{
        storage_core::HelixGraphStorage,
        traversal_core::{
            ops::{
                g::G,
                source::{
                    add_n::AddNAdapter, n_from_id::NFromIdAdapter, n_from_index::NFromIndexAdapter,
                },
                util::{drop::Drop, update::UpdateAdapter},
            },
            traversal_value::{Traversable, TraversalValue},
        },
    },
    props,
};

use tempfile::TempDir;

#[allow(dead_code)]
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
fn test_delete_node_with_secondary_index() {
    let (storage, _) = {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().to_str().unwrap();
        let mut config = crate::helix_engine::traversal_core::config::Config::default();
        config.graph_config.as_mut().unwrap().secondary_indices = Some(vec!["name".to_string()]);
        let storage = HelixGraphStorage::new(db_path, config, Default::default()).unwrap();
        (Arc::new(storage), temp_dir)
    };

    let mut txn = storage.graph_env.write_txn().unwrap();

    let node = G::new_mut(Arc::clone(&storage), &mut txn)
        .add_n("person", Some(props! { "name" => "John" }), Some(&["name"]))
        .collect_to_obj();
    let node_id = node.id(); // Save the ID before moving

    let _ = G::new_mut_from(Arc::clone(&storage), &mut txn, node)
        .update(Some(props! { "name" => "Jane" }))
        .collect_to_obj();

    txn.commit().unwrap();

    let txn = storage.graph_env.read_txn().unwrap();

    let jane_nodes = G::new(Arc::clone(&storage), &txn)
        .n_from_index("person", "name", &"Jane".to_string())
        .collect_to::<Vec<_>>();
    assert_eq!(jane_nodes.len(), 1);
    assert_eq!(jane_nodes[0].id(), node_id); // Compare with original node id

    let john_nodes = G::new(Arc::clone(&storage), &txn)
        .n_from_index("person", "name", &"John".to_string())
        .collect_to::<Vec<_>>();
    assert_eq!(john_nodes.len(), 0);

    txn.commit().unwrap();

    let mut txn = storage.graph_env.write_txn().unwrap();
    Drop::<Vec<_>>::drop_traversal(
        G::new(Arc::clone(&storage), &txn)
            .n_from_id(&node_id) // Use the original node id
            .collect_to::<Vec<_>>(),
        Arc::clone(&storage),
        &mut txn,
    )
    .unwrap();

    txn.commit().unwrap();

    let txn = storage.graph_env.read_txn().unwrap();

    let node = G::new(Arc::clone(&storage), &txn)
        .n_from_index("person", "name", &"Jane".to_string())
        .collect_to::<Vec<_>>();
    assert_eq!(node.len(), 0);
}

#[test]
fn test_update_of_secondary_indices() {
    let (storage, _) = {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().to_str().unwrap();
        let mut config = crate::helix_engine::traversal_core::config::Config::default();
        config.graph_config.as_mut().unwrap().secondary_indices = Some(vec!["name".to_string()]);
        let storage = HelixGraphStorage::new(db_path, config, Default::default()).unwrap();
        (Arc::new(storage), temp_dir)
    };
    let mut txn = storage.graph_env.write_txn().unwrap();

    let node = G::new_mut(Arc::clone(&storage), &mut txn)
        .add_n("person", Some(props! { "name" => "John" }), Some(&["name"]))
        .collect_to_obj();

    let _ = G::new_mut_from(Arc::clone(&storage), &mut txn, node)
        .update(Some(props! { "name" => "Jane" }))
        .collect_to_obj();

    txn.commit().unwrap();

    let txn = storage.graph_env.read_txn().unwrap();

    let node = G::new(Arc::clone(&storage), &txn)
        .n_from_index("person", "name", &"Jane".to_string())
        .collect_to::<Vec<_>>();
    assert_eq!(node.len(), 1);
    assert_eq!(node[0].id(), node.id());
    if let TraversalValue::Node(node) = &node[0] {
        assert_eq!(
            *node.properties.as_ref().unwrap().get("name").unwrap(),
            "Jane".to_string()
        );
    } else {
        panic!("Node not found");
    }

    let node = G::new(Arc::clone(&storage), &txn)
        .n_from_index("person", "name", &"John".to_string())
        .collect_to::<Vec<_>>();
    assert_eq!(node.len(), 0);

    txn.commit().unwrap();
}
