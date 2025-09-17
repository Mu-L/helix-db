use std::sync::Arc;

use crate::{
    helix_engine::{
        storage_core::HelixGraphStorage,
        traversal_core::{
            ops::{
                g::G,
                in_::in_::InAdapter,
                out::out::OutAdapter,
                source::{
                    add_e::{AddEAdapter, EdgeType},
                    add_n::AddNAdapter,
                    e_from_type::EFromTypeAdapter,
                    n_from_id::NFromIdAdapter,
                    n_from_type::NFromTypeAdapter,
                },
                util::{filter_ref::FilterRefAdapter, props::PropsAdapter},
            },
            traversal_value::{Traversable, TraversalValue},
        },
    },
    props,
    protocol::value::Value,
    utils::{filterable::Filterable, id::ID},
};

use serde::{Deserialize, Serialize};
use tempfile::TempDir;

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
fn test_add_n() {
    let (storage, _temp_dir) = setup_test_db();

    let mut txn = storage.graph_env.write_txn().unwrap();

    let nodes = G::new_mut(Arc::clone(&storage), &mut txn)
        .add_n("person", Some(props! { "name" => "John"}), None)
        .filter_map(|node| node.ok())
        .collect::<Vec<_>>();

    let node = G::new(Arc::clone(&storage), &txn)
        .n_from_id(&nodes.first().unwrap().id())
        .collect_to::<Vec<_>>();
    assert_eq!(node.first().unwrap().label(), "person");
    println!("node: {:?}", node.first().unwrap());

    assert_eq!(node.first().unwrap().id(), nodes.first().unwrap().id());
    assert_eq!(
        *node.first().unwrap().check_property("name").unwrap(),
        Value::String("John".to_string())
    );
    println!("node: {:?}", node.first().unwrap());

    // If we haven't dropped txn, ensure no borrows exist before commit
    txn.commit().unwrap();
}

#[test]
fn test_out() {
    let (storage, _temp_dir) = setup_test_db();
    let mut txn = storage.graph_env.write_txn().unwrap();

    // Create graph: (person1)-[knows]->(person2)-[knows]->(person3)
    let person1 = G::new_mut(Arc::clone(&storage), &mut txn)
        .add_n("person", Some(props!()), None)
        .collect_to::<Vec<_>>();
    let person1 = person1.first().unwrap();
    let person2 = G::new_mut(Arc::clone(&storage), &mut txn)
        .add_n("person", Some(props!()), None)
        .collect_to::<Vec<_>>();
    let person2 = person2.first().unwrap();
    let person3 = G::new_mut(Arc::clone(&storage), &mut txn)
        .add_n("person", Some(props!()), None)
        .collect_to::<Vec<_>>();
    let person3 = person3.first().unwrap();

    G::new_mut(Arc::clone(&storage), &mut txn)
        .add_e(
            "knows",
            Some(props!()),
            person1.id(),
            person2.id(),
            false,
            EdgeType::Node,
        )
        .collect_to::<Vec<_>>();
    G::new_mut(Arc::clone(&storage), &mut txn)
        .add_e(
            "knows",
            Some(props!()),
            person2.id(),
            person3.id(),
            false,
            EdgeType::Node,
        )
        .collect_to::<Vec<_>>();

    txn.commit().unwrap();
    let txn = storage.graph_env.write_txn().unwrap();

    // let nodes = VFromId::new(&storage, &txn, person1.id.as_str())
    //     .out("knows")
    //     .filter_map(|node| node.ok())
    //     .collect::<Vec<_>>();
    let nodes = G::new(Arc::clone(&storage), &txn)
        .n_from_id(&person1.id())
        .out("knows", &EdgeType::Node)
        .filter_map(|node| node.ok())
        .collect::<Vec<_>>();

    // txn.commit().unwrap();
    // Check that current step is at person2
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].id(), person2.id());
}

#[test]
fn test_in() {
    let (storage, _temp_dir) = setup_test_db();
    let mut txn = storage.graph_env.write_txn().unwrap();

    // Create graph: (person1)-[knows]->(person2)
    let person1 = G::new_mut(Arc::clone(&storage), &mut txn)
        .add_n("person", Some(props!()), None)
        .collect_to::<Vec<_>>();
    let person1 = person1.first().unwrap();
    let person2 = G::new_mut(Arc::clone(&storage), &mut txn)
        .add_n("person", Some(props!()), None)
        .collect_to::<Vec<_>>();
    let person2 = person2.first().unwrap();

    G::new_mut(Arc::clone(&storage), &mut txn)
        .add_e(
            "knows",
            Some(props!()),
            person1.id(),
            person2.id(),
            false,
            EdgeType::Node,
        )
        .collect_to::<Vec<_>>();
    txn.commit().unwrap();
    let txn = storage.graph_env.read_txn().unwrap();
    let nodes = G::new(Arc::clone(&storage), &txn)
        .n_from_id(&person2.id())
        .in_("knows", &EdgeType::Node)
        .collect_to::<Vec<_>>();

    // Check that current step is at person1
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].id(), person1.id());
}

#[test]
fn test_complex_traversal() {
    let (storage, _temp_dir) = setup_test_db();
    let mut txn = storage.graph_env.write_txn().unwrap();

    // Graph structure:
    // (person1)-[knows]->(person2)-[likes]->(person3)
    //     ^                                     |
    //     |                                     |
    //     +-------<------[follows]------<-------+

    let person1 = G::new_mut(Arc::clone(&storage), &mut txn)
        .add_n("person", Some(props!()), None)
        .collect_to::<Vec<_>>();
    let person1 = person1.first().unwrap();
    let person2 = G::new_mut(Arc::clone(&storage), &mut txn)
        .add_n("person", Some(props!()), None)
        .collect_to::<Vec<_>>();
    let person2 = person2.first().unwrap();
    let person3 = G::new_mut(Arc::clone(&storage), &mut txn)
        .add_n("person", Some(props!()), None)
        .collect_to::<Vec<_>>();
    let person3 = person3.first().unwrap();

    G::new_mut(Arc::clone(&storage), &mut txn)
        .add_e(
            "knows",
            Some(props!()),
            person1.id(),
            person2.id(),
            false,
            EdgeType::Node,
        )
        .collect_to::<Vec<_>>();
    G::new_mut(Arc::clone(&storage), &mut txn)
        .add_e(
            "likes",
            Some(props!()),
            person2.id(),
            person3.id(),
            false,
            EdgeType::Node,
        )
        .collect_to::<Vec<_>>();
    G::new_mut(Arc::clone(&storage), &mut txn)
        .add_e(
            "follows",
            Some(props!()),
            person3.id(),
            person1.id(),
            false,
            EdgeType::Node,
        )
        .collect_to::<Vec<_>>();
    txn.commit().unwrap();

    let txn = storage.graph_env.read_txn().unwrap();

    let nodes = G::new(Arc::clone(&storage), &txn)
        .n_from_id(&person1.id())
        .out("knows", &EdgeType::Node)
        .collect_to::<Vec<_>>();

    // Check that current step is at person2
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].id(), person2.id());

    // Traverse from person2 to person3
    let nodes = G::new_from(Arc::clone(&storage), &txn, vec![nodes[0].clone()])
        .out("likes", &EdgeType::Node)
        .collect_to::<Vec<_>>();

    // Check that current step is at person3
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].id(), person3.id());

    // Traverse from person3 to person1
    let nodes = G::new_from(Arc::clone(&storage), &txn, vec![nodes[0].clone()])
        .out("follows", &EdgeType::Node)
        .collect_to::<Vec<_>>();

    // Check that current step is at person1
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].id(), person1.id());
}

#[test]
fn test_n_from_id() {
    let (storage, _temp_dir) = setup_test_db();
    let mut txn = storage.graph_env.write_txn().unwrap();

    // Create a test node
    let person = G::new_mut(Arc::clone(&storage), &mut txn)
        .add_n("person", Some(props!()), None)
        .collect_to::<Vec<_>>();
    let node_id = person.id();

    txn.commit().unwrap();
    let txn = storage.graph_env.read_txn().unwrap();
    let count = G::new(Arc::clone(&storage), &txn)
        .n_from_id(&node_id)
        .collect_to::<Vec<_>>();

    assert_eq!(count.len(), 1);
}

#[test]
fn test_n_from_id_with_traversal() {
    let (storage, _temp_dir) = setup_test_db();
    let mut txn = storage.graph_env.write_txn().unwrap();

    // Create test graph: (person1)-[knows]->(person2)
    let person1 = G::new_mut(Arc::clone(&storage), &mut txn)
        .add_n("person", Some(props!()), None)
        .collect_to::<Vec<_>>();
    let person2 = G::new_mut(Arc::clone(&storage), &mut txn)
        .add_n("person", Some(props!()), None)
        .collect_to::<Vec<_>>();
    G::new_mut(Arc::clone(&storage), &mut txn)
        .add_e(
            "knows",
            Some(props!()),
            person1.id(),
            person2.id(),
            true,
            EdgeType::Node,
        )
        .collect_to::<Vec<_>>();

    txn.commit().unwrap();
    let txn = storage.graph_env.read_txn().unwrap();
    let count = G::new(Arc::clone(&storage), &txn)
        .n_from_id(&person1.id())
        .out("knows", &EdgeType::Node)
        .collect_to::<Vec<_>>();

    // Check that traversal reaches person2
    assert_eq!(count.len(), 1);
    assert_eq!(count[0].id(), person2.id());
}

#[test]
fn test_n_from_id_nonexistent() {
    let (storage, _temp_dir) = setup_test_db();
    let txn = storage.graph_env.read_txn().unwrap();
    let nodes = G::new(Arc::clone(&storage), &txn)
        .n_from_id(&100)
        .collect_to::<Vec<_>>();
    assert!(nodes.is_empty());
}

#[test]
fn test_n_from_id_chain_operations() {
    let (storage, _temp_dir) = setup_test_db();
    let mut txn = storage.graph_env.write_txn().unwrap();

    // Create test graph: (person1)-[knows]->(person2)-[likes]->(person3)
    let person1 = G::new_mut(Arc::clone(&storage), &mut txn)
        .add_n("person", Some(props!()), None)
        .collect_to::<Vec<_>>();
    let person2 = G::new_mut(Arc::clone(&storage), &mut txn)
        .add_n("person", Some(props!()), None)
        .collect_to::<Vec<_>>();
    let _ = G::new_mut(Arc::clone(&storage), &mut txn)
        .add_n("person", Some(props!()), None)
        .collect_to::<Vec<_>>();
    let person3 = G::new_mut(Arc::clone(&storage), &mut txn)
        .add_n("person", Some(props!()), None)
        .collect_to::<Vec<_>>();

    G::new_mut(Arc::clone(&storage), &mut txn)
        .add_e(
            "knows",
            Some(props!()),
            person1.id(),
            person2.id(),
            false,
            EdgeType::Node,
        )
        .collect_to::<Vec<_>>();
    G::new_mut(Arc::clone(&storage), &mut txn)
        .add_e(
            "likes",
            Some(props!()),
            person2.id(),
            person3.id(),
            false,
            EdgeType::Node,
        )
        .collect_to::<Vec<_>>();

    txn.commit().unwrap();
    let txn = storage.graph_env.read_txn().unwrap();
    let nodes = G::new(Arc::clone(&storage), &txn)
        .n_from_id(&person1.id())
        .out("knows", &EdgeType::Node)
        .out("likes", &EdgeType::Node)
        .collect_to::<Vec<_>>();

    // Check that the chain of traversals reaches person3
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].id(), person3.id());
}

#[test]
fn test_with_id_type() {
    let (storage, _temp_dir) = setup_test_db();
    let mut txn = storage.graph_env.write_txn().unwrap();

    let node = G::new_mut(Arc::clone(&storage), &mut txn)
        .add_n("person", Some(props! { "name" => "test" }), None)
        .collect_to_obj();
    txn.commit().unwrap();
    #[derive(Serialize, Deserialize, Debug)]
    struct Input {
        id: ID,
        name: String,
    }

    let input = sonic_rs::from_slice::<Input>(
        format!(
            "{{\"id\":\"{}\",\"name\":\"test\"}}",
            uuid::Uuid::from_u128(node.id())
        )
        .as_bytes(),
    )
    .unwrap();
    let txn = storage.graph_env.read_txn().unwrap();
    let traversal = G::new(Arc::clone(&storage), &txn)
        .n_from_id(&input.id)
        .collect_to::<Vec<_>>();

    assert_eq!(traversal.len(), 1);
    assert_eq!(traversal[0].id(), input.id.inner());
}

#[test]
fn test_double_add_and_double_fetch() {
    let (db, _temp_dir) = setup_test_db();
    let mut txn = db.graph_env.write_txn().unwrap();

    let original_node1 = G::new_mut(Arc::clone(&db), &mut txn)
        .add_n("person", Some(props! { "entity_name" => "person1" }), None)
        .collect_to_obj();

    let original_node2 = G::new_mut(Arc::clone(&db), &mut txn)
        .add_n("person", Some(props! { "entity_name" => "person2" }), None)
        .collect_to_obj();

    txn.commit().unwrap();

    let mut txn = db.graph_env.write_txn().unwrap();
    let node1 = G::new(Arc::clone(&db), &txn)
        .n_from_type("person")
        .filter_ref(|val, txn| {
            if let Ok(val) = val {
                Ok(G::new_from(Arc::clone(&db), txn, val.clone())
                    .check_property("entity_name")
                    .map_value_or(false, |v| *v == "person1")?)
            } else {
                Ok(false)
            }
        })
        .collect_to::<Vec<_>>();

    let node2 = G::new(Arc::clone(&db), &txn)
        .n_from_type("person")
        .filter_ref(|val, txn| {
            if let Ok(val) = val {
                Ok(G::new_from(Arc::clone(&db), txn, val.clone())
                    .check_property("entity_name")
                    .map_value_or(false, |v| *v == "person2")?)
            } else {
                Ok(false)
            }
        })
        .collect_to::<Vec<_>>();

    assert_eq!(node1.len(), 1);
    assert_eq!(node1[0].id(), original_node1.id());
    assert_eq!(node2.len(), 1);
    assert_eq!(node2[0].id(), original_node2.id());

    let _e = G::new_mut(Arc::clone(&db), &mut txn)
        .add_e("knows", None, node1.id(), node2.id(), false, EdgeType::Node)
        .collect_to_obj();

    txn.commit().unwrap();

    let txn = db.graph_env.read_txn().unwrap();
    let e = G::new(Arc::clone(&db), &txn)
        .e_from_type("knows")
        .collect_to::<Vec<_>>();
    assert_eq!(e.len(), 1);
    assert_eq!(e[0].id(), e.id());
    if let TraversalValue::Edge(e) = &e[0] {
        assert_eq!(e.from_node(), node1.id());
        assert_eq!(e.to_node(), node2.id());
    } else {
        panic!("e[0] is not an edge");
    }
}
