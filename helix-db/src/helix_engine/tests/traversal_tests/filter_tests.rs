use std::sync::Arc;

use crate::helix_engine::{
    storage_core::HelixGraphStorage,
    traversal_core::{
        ops::{g::G, source::add_n::AddNAdapter, util::filter_ref::FilterRefAdapter},
        traversal_value::{Traversable, TraversalValue},
    },
    types::GraphError,
};
use crate::{helix_engine::traversal_core::ops::source::e_from_type::EFromTypeAdapter, props};
use crate::{
    helix_engine::traversal_core::ops::source::n_from_type::NFromTypeAdapter,
    protocol::value::Value, utils::filterable::Filterable,
};
use tempfile::TempDir;

use crate::helix_engine::traversal_core::ops::source::add_e::{AddEAdapter, EdgeType};

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
fn test_filter_nodes() {
    let (storage, _temp_dir) = setup_test_db();
    let mut txn = storage.graph_env.write_txn().unwrap();

    // Create nodes with different properties
    let _ = G::new_mut(Arc::clone(&storage), &mut txn)
        .add_n("person", Some(props! { "age" => 25 }), None)
        .collect_to::<Vec<_>>();
    let _ = G::new_mut(Arc::clone(&storage), &mut txn)
        .add_n("person", Some(props! { "age" => 30 }), None)
        .collect_to::<Vec<_>>();
    let person3 = G::new_mut(Arc::clone(&storage), &mut txn)
        .add_n("person", Some(props! { "age" => 35 }), None)
        .collect_to::<Vec<_>>();

    txn.commit().unwrap();
    let txn = storage.graph_env.read_txn().unwrap();

    let traversal = G::new(Arc::clone(&storage), &txn)
        .n_from_type("person")
        .filter_ref(|val, _| {
            if let Ok(TraversalValue::Node(node)) = val {
                if let Ok(value) = node.check_property("age") {
                    match value.as_ref() {
                        Value::F64(age) => Ok(*age > 30.0),
                        Value::I32(age) => Ok(*age > 30),
                        _ => Ok(false),
                    }
                } else {
                    Ok(false)
                }
            } else {
                Ok(false)
            }
        })
        .collect_to::<Vec<_>>();
    assert_eq!(traversal.len(), 1);
    assert_eq!(traversal[0].id(), person3.id());
}

#[test]
fn test_filter_macro_single_argument() {
    let (storage, _temp_dir) = setup_test_db();
    let mut txn = storage.graph_env.write_txn().unwrap();

    let _ = G::new_mut(Arc::clone(&storage), &mut txn)
        .add_n("person", Some(props! { "name" => "Alice" }), None)
        .collect_to::<Vec<_>>();
    let _ = G::new_mut(Arc::clone(&storage), &mut txn)
        .add_n("person", Some(props! { "name" => "Bob" }), None)
        .collect_to::<Vec<_>>();

    fn has_name(val: &Result<TraversalValue, GraphError>) -> Result<bool, GraphError> {
        if let Ok(TraversalValue::Node(node)) = val {
            node.check_property("name").map_or(Ok(false), |_| Ok(true))
        } else {
            Ok(false)
        }
    }

    txn.commit().unwrap();
    let txn = storage.graph_env.read_txn().unwrap();
    let traversal = G::new(Arc::clone(&storage), &txn)
        .n_from_type("person")
        .filter_ref(|val, _| has_name(val))
        .collect_to::<Vec<_>>();
    assert_eq!(traversal.len(), 2);
    assert!(
        traversal
            .iter()
            .any(|val| if let TraversalValue::Node(node) = val {
                let name = node.check_property("name").unwrap();
                name.as_ref() == &Value::String("Alice".to_string())
                    || name.as_ref() == &Value::String("Bob".to_string())
            } else {
                false
            })
    );
}

#[test]
fn test_filter_macro_multiple_arguments() {
    let (storage, _temp_dir) = setup_test_db();
    let mut txn = storage.graph_env.write_txn().unwrap();

    let _ = G::new_mut(Arc::clone(&storage), &mut txn)
        .add_n("person", Some(props! { "age" => 25 }), None)
        .collect_to::<Vec<_>>();
    let person2 = G::new_mut(Arc::clone(&storage), &mut txn)
        .add_n("person", Some(props! { "age" => 30 }), None)
        .collect_to::<Vec<_>>();
    txn.commit().unwrap();

    fn age_greater_than(
        val: &Result<TraversalValue, GraphError>,
        min_age: i32,
    ) -> Result<bool, GraphError> {
        if let Ok(TraversalValue::Node(node)) = val {
            if let Ok(value) = node.check_property("age") {
                match value.as_ref() {
                    Value::F64(age) => Ok(*age > min_age as f64),
                    Value::I32(age) => Ok(*age > min_age),
                    _ => Ok(false),
                }
            } else {
                Ok(false)
            }
        } else {
            Ok(false)
        }
    }

    let txn = storage.graph_env.read_txn().unwrap();
    let traversal = G::new(Arc::clone(&storage), &txn)
        .n_from_type("person")
        .filter_ref(|val, _| age_greater_than(val, 27))
        .collect_to::<Vec<_>>();

    assert_eq!(traversal.len(), 1);
    assert_eq!(traversal[0].id(), person2.id());
}

#[test]
fn test_filter_edges() {
    let (storage, _temp_dir) = setup_test_db();
    let mut txn = storage.graph_env.write_txn().unwrap();

    let person1 = G::new_mut(Arc::clone(&storage), &mut txn)
        .add_n("person", Some(props!()), None)
        .collect_to::<Vec<_>>();
    let person2 = G::new_mut(Arc::clone(&storage), &mut txn)
        .add_n("person", Some(props!()), None)
        .collect_to::<Vec<_>>();

    let _ = G::new_mut(Arc::clone(&storage), &mut txn)
        .add_e(
            "knows",
            Some(props! { "since" => 2020 }),
            person1.id(),
            person2.id(),
            false,
            EdgeType::Node,
        )
        .collect_to::<Vec<_>>();
    let edge2 = G::new_mut(Arc::clone(&storage), &mut txn)
        .add_e(
            "knows",
            Some(props! { "since" => 2022 }),
            person2.id(),
            person1.id(),
            false,
            EdgeType::Node,
        )
        .collect_to::<Vec<_>>();

    txn.commit().unwrap();
    let txn = storage.graph_env.read_txn().unwrap();

    fn recent_edge(
        val: &Result<TraversalValue, GraphError>,
        year: i32,
    ) -> Result<bool, GraphError> {
        if let Ok(TraversalValue::Edge(edge)) = val {
            if let Ok(value) = edge.check_property("since") {
                match value.as_ref() {
                    Value::I32(since) => Ok(*since > year),
                    Value::F64(since) => Ok(*since > year as f64),
                    _ => Ok(false),
                }
            } else {
                Ok(false)
            }
        } else {
            Ok(false)
        }
    }

    let traversal = G::new(Arc::clone(&storage), &txn)
        .e_from_type("knows")
        .filter_ref(|val, _| recent_edge(val, 2021))
        .collect_to::<Vec<_>>();

    assert_eq!(traversal.len(), 1);
    assert_eq!(traversal[0].id(), edge2.id());
}

#[test]
fn test_filter_empty_result() {
    let (storage, _temp_dir) = setup_test_db();
    let mut txn = storage.graph_env.write_txn().unwrap();

    let _ = G::new_mut(Arc::clone(&storage), &mut txn)
        .add_n("person", Some(props! { "age" => 25 }), None)
        .collect_to::<Vec<_>>();

    txn.commit().unwrap();
    let txn = storage.graph_env.read_txn().unwrap();
    let traversal = G::new(Arc::clone(&storage), &txn)
        .n_from_type("person")
        .filter_ref(|val, _| {
            if let Ok(TraversalValue::Node(node)) = val {
                if let Ok(value) = node.check_property("age") {
                    match value.as_ref() {
                        Value::I32(age) => Ok(*age > 100),
                        Value::F64(age) => Ok(*age > 100.0),
                        _ => Ok(false),
                    }
                } else {
                    Ok(false)
                }
            } else {
                Ok(false)
            }
        })
        .collect_to::<Vec<_>>();
    assert!(traversal.is_empty());
}

#[test]
fn test_filter_chain() {
    let (storage, _temp_dir) = setup_test_db();
    let mut txn = storage.graph_env.write_txn().unwrap();

    let _ = G::new_mut(Arc::clone(&storage), &mut txn)
        .add_n(
            "person",
            Some(props! { "age" => 25, "name" => "Alice" }),
            None,
        )
        .collect_to_obj();
    let person2 = G::new_mut(Arc::clone(&storage), &mut txn)
        .add_n(
            "person",
            Some(props! { "age" => 30, "name" => "Bob" }),
            None,
        )
        .collect_to_obj();
    let _ = G::new_mut(Arc::clone(&storage), &mut txn)
        .add_n("person", Some(props! { "age" => 35 }), None)
        .collect_to_obj();

    txn.commit().unwrap();
    let txn = storage.graph_env.read_txn().unwrap();

    fn has_name(val: &Result<TraversalValue, GraphError>) -> Result<bool, GraphError> {
        if let Ok(TraversalValue::Node(node)) = val {
            node.check_property("name").map_or(Ok(false), |_| Ok(true))
        } else {
            Ok(false)
        }
    }

    fn age_greater_than(
        val: &Result<TraversalValue, GraphError>,
        min_age: i32,
    ) -> Result<bool, GraphError> {
        if let Ok(TraversalValue::Node(node)) = val {
            if let Ok(value) = node.check_property("age") {
                match value.as_ref() {
                    Value::F64(age) => Ok(*age > min_age as f64),
                    Value::I32(age) => Ok(*age > min_age),
                    _ => Ok(false),
                }
            } else {
                Ok(false)
            }
        } else {
            Ok(false)
        }
    }

    let traversal = G::new(Arc::clone(&storage), &txn)
        .n_from_type("person")
        .filter_ref(|val, _| has_name(val))
        .filter_ref(|val, _| age_greater_than(val, 27))
        .collect_to::<Vec<_>>();

    assert_eq!(traversal.len(), 1);
    assert_eq!(traversal[0].id(), person2.id());
}
