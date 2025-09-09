//! The following tests are included in this file
//! - test_exclude_field_remapping ✅
//! - test_field_remapping ✅
//! - test_identifier_remapping ✅
//! - test_traversal_remapping ✅
//! - test_value_remapping ✅
//! - test-exists-remapping ✅
//! - test-one-of-each-remapping ✅
//! - test-nested-remapping ✅

use std::{collections::HashMap, sync::Arc};

use crate::{
    exclude_field, exists_remapping, field_remapping,
    helix_engine::{
        storage_core::HelixGraphStorage,
        traversal_core::{
            ops::{
                g::G, in_::in_::InAdapter, out::out::OutAdapter, source::{
                    add_e::{AddEAdapter, EdgeType},
                    add_n::AddNAdapter,
                    n_from_id::NFromIdAdapter,
                    n_from_type::NFromTypeAdapter,
                }, util::map::MapAdapter
            },
            traversal_value::{Traversable, TraversalValue},
        },
        types::GraphError,
    },
    identifier_remapping, props,
    protocol::{
        remapping::{Remapping, RemappingMap, ResponseRemapping},
        return_values::ReturnValue,
    },
    traversal_remapping, value_remapping,
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
fn test_exclude_field_remapping() {
    let (storage, _temp_dir) = setup_test_db();
    let mut txn = storage.graph_env.write_txn().unwrap();

    let _node = G::new_mut(Arc::clone(&storage), &mut txn)
        .add_n(
            "person",
            Some(props! { "text" => "test", "other" => "other" }),
            None,
        )
        .collect_to_obj();

    let traversal = G::new(Arc::clone(&storage), &txn)
        .n_from_type("person")
        .collect_to::<Vec<_>>();

    let remapping_vals = RemappingMap::new();

    let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
    return_vals.insert(
        "files".to_string(),
        ReturnValue::from_traversal_value_array_with_mixin(
            G::new_from(Arc::clone(&storage), &txn, traversal.clone())
                .map_traversal(|item, _txn| {
                    println!("item: {item:?}");
                    exclude_field!(remapping_vals, item.clone(), "text")?;
                    Ok(item)
                })
                .collect_to::<Vec<_>>()
                .clone(),
            remapping_vals.borrow_mut(),
        ),
    );

    assert_eq!(return_vals.len(), 1);

    #[derive(Serialize, Deserialize)]
    struct Test {
        text: Option<String>,
        other: Option<String>,
    }
    #[derive(Serialize, Deserialize)]
    struct Response {
        files: Vec<Test>,
    }
    let value = sonic_rs::to_vec(&return_vals).unwrap();
    let value: Response = sonic_rs::from_slice(&value).unwrap();
    let value = value.files.first().unwrap();

    let expected = Test {
        text: None,
        other: Some("other".to_string()),
    };

    assert_eq!(value.text, expected.text);
    assert_eq!(value.other, expected.other);
}

#[test]
fn test_field_remapping() {
    let (storage, _temp_dir) = setup_test_db();
    let mut txn = storage.graph_env.write_txn().unwrap();
    #[derive(Serialize, Deserialize)]
    struct Expected {
        new_name: String,
    }
    let original = Expected {
        new_name: "test".to_string(),
    };

    let _node = G::new_mut(Arc::clone(&storage), &mut txn)
        .add_n(
            "person",
            Some(props! { "text" => original.new_name.clone(), "other" => "other" }),
            None,
        )
        .collect_to_obj();

    let traversal = G::new(Arc::clone(&storage), &txn)
        .n_from_type("person")
        .collect_to::<Vec<_>>();

    let remapping_vals = RemappingMap::new();

    let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
    return_vals.insert(
        "files".to_string(),
        ReturnValue::from_traversal_value_array_with_mixin(
            G::new_from(Arc::clone(&storage), &txn, traversal.clone())
                .map_traversal(|item, _txn| {
                    field_remapping!(remapping_vals, item.clone(), false, "text" => "new_name")?;
                    Ok(item)
                })
                .collect_to::<Vec<_>>()
                .clone(),
            remapping_vals.borrow_mut(),
        ),
    );

    assert_eq!(return_vals.len(), 1);

    #[derive(Serialize, Deserialize)]
    struct Response {
        files: Vec<Expected>,
    }
    let value = sonic_rs::to_vec(&return_vals).unwrap();
    let value: Response = sonic_rs::from_slice(&value).unwrap();
    let value = value.files.first().unwrap();

    assert_eq!(value.new_name, original.new_name);
}

#[test]
fn test_identifier_remapping() {
    let (storage, _temp_dir) = setup_test_db();
    let mut txn = storage.graph_env.write_txn().unwrap();
    #[derive(Serialize, Deserialize)]
    struct Expected {
        new_value: String,
    }
    let original = Expected {
        new_value: "test".to_string(),
    };
    let _node = G::new_mut(Arc::clone(&storage), &mut txn)
        .add_n(
            "person",
            Some(props! { "field" => original.new_value.clone(), "other" => "other" }),
            None,
        )
        .collect_to_obj();

    let traversal = G::new(Arc::clone(&storage), &txn)
        .n_from_type("person")
        .collect_to::<Vec<_>>();

    let remapping_vals = RemappingMap::new();

    let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
    return_vals.insert(
        "files".to_string(),
        ReturnValue::from_traversal_value_array_with_mixin(
            G::new_from(Arc::clone(&storage), &txn, traversal.clone())
            .map_traversal(|item, _txn| {
                identifier_remapping!(remapping_vals, item.clone(), false, "field" => "new_value")?;
                Ok(item)
        })
        .collect_to::<Vec<_>>()
        .clone(),
        remapping_vals.borrow_mut(),
    ),
    );

    assert_eq!(return_vals.len(), 1);

    #[derive(Serialize, Deserialize)]
    struct Data {
        field: String,
    }

    #[derive(Serialize, Deserialize)]
    struct Response {
        files: Vec<Data>,
    }
    let value = sonic_rs::to_vec(&return_vals).unwrap();
    let value: Response = sonic_rs::from_slice(&value).unwrap();
    let value = value.files.first().unwrap();
    assert_eq!(value.field, "new_value".to_string());
}

#[test]
fn test_traversal_remapping() {
    let (storage, _temp_dir) = setup_test_db();
    let mut txn = storage.graph_env.write_txn().unwrap();

    let _node = G::new_mut(Arc::clone(&storage), &mut txn)
        .add_n(
            "person",
            Some(props! { "text" => "test", "other" => "other" }),
            None,
        )
        .collect_to_obj();
    let _other_node = G::new_mut(Arc::clone(&storage), &mut txn)
        .add_n(
            "friemd",
            Some(props! { "text" => "test", "other" => "other" }),
            None,
        )
        .collect_to_obj();
    let _edge = G::new_mut(Arc::clone(&storage), &mut txn)
        .add_e(
            "knows",
            Some(props!()),
            _node.id(),
            _other_node.id(),
            false,
            EdgeType::Node,
        )
        .collect_to_obj();

    txn.commit().unwrap();
    let txn = storage.graph_env.read_txn().unwrap();

    let traversal = G::new(Arc::clone(&storage), &txn)
        .n_from_type("person")
        .collect_to::<Vec<_>>();

    let remapping_vals = RemappingMap::new();

    let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
    return_vals.insert(
        "users".to_string(),
        ReturnValue::from_traversal_value_array_with_mixin(
            G::new_from(Arc::clone(&storage), &txn, traversal.clone())
            .map_traversal(|item, _txn| {
                traversal_remapping!(remapping_vals, item.clone(), false, "friends" => G::new_from(Arc::clone(&storage), &txn, vec![item.clone()]).out("knows", &EdgeType::Node).collect_to::<Vec<_>>())?;
                Ok(item)
            })
            .collect_to::<Vec<_>>()
            .clone(),
            remapping_vals.borrow_mut(),
        ),
    );

    assert_eq!(return_vals.len(), 1);

    let value = match return_vals.get("users").unwrap() {
        ReturnValue::Array(array) => match array.first().unwrap() {
            ReturnValue::Object(object) => object,
            _ => panic!("Expected Node"),
        },
        _ => panic!("Expected Array"),
    };
    // assert_eq!(
    //     *value.get("id").unwrap(),
    //     ReturnValue::from(_other_node.uuid())
    // );
    // assert_eq!(
    //     *value.get("label").unwrap(),
    //     ReturnValue::from("person".to_string())
    // );

    match value.get("friends").unwrap() {
        ReturnValue::Array(array) => {
            assert_eq!(array.len(), 1);
            let node_id = match array.first().unwrap() {
                ReturnValue::Object(object) => object.get("id").unwrap(),
                _ => panic!("Expected Object"),
            };
            assert_eq!(*node_id, ReturnValue::from(_other_node.uuid()));
        }
        _ => panic!("Expected Array"),
    }
}

#[test]
fn test_value_remapping() {
    let (storage, _temp_dir) = setup_test_db();
    let mut txn = storage.graph_env.write_txn().unwrap();

    let _node = G::new_mut(Arc::clone(&storage), &mut txn)
        .add_n(
            "person",
            Some(props! { "text" => "test", "other" => "other" }),
            None,
        )
        .collect_to_obj();

    let traversal = G::new(Arc::clone(&storage), &txn)
        .n_from_type("person")
        .collect_to::<Vec<_>>();

    let remapping_vals = RemappingMap::new();

    let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
    return_vals.insert(
        "users".to_string(),
        ReturnValue::from_traversal_value_array_with_mixin(
            G::new_from(Arc::clone(&storage), &txn, traversal.clone())
                .map_traversal(|item, _txn| {
                    value_remapping!(remapping_vals, item.clone(), false, "text" => "new_name")?;
                    Ok(item)
                })
                .collect_to::<Vec<_>>()
                .clone(),
            remapping_vals.borrow_mut(),
        ),
    );

    assert_eq!(return_vals.len(), 1);

    let value = match return_vals.get("users").unwrap() {
        ReturnValue::Array(array) => match array.first().unwrap() {
            ReturnValue::Object(object) => object,
            _ => panic!("Expected Node"),
        },
        _ => panic!("Expected Array"),
    };

    assert_eq!(
        *value.get("text").unwrap(),
        ReturnValue::from("new_name".to_string())
    );
}

#[test]
fn test_exists_remapping() {
    let (storage, _temp_dir) = setup_test_db();
    let mut txn = storage.graph_env.write_txn().unwrap();

    let _node = G::new_mut(Arc::clone(&storage), &mut txn)
        .add_n(
            "person",
            Some(props! { "text" => "test", "other" => "other" }),
            None,
        )
        .collect_to_obj();
    let _other_node = G::new_mut(Arc::clone(&storage), &mut txn)
        .add_n(
            "person",
            Some(props! { "text" => "test", "other" => "other" }),
            None,
        )
        .collect_to_obj();
    let _edge = G::new_mut(Arc::clone(&storage), &mut txn)
        .add_e(
            "knows",
            Some(props!()),
            _node.id(),
            _other_node.id(),
            false,
            EdgeType::Node,
        )
        .collect_to_obj();

    let traversal = G::new(Arc::clone(&storage), &txn)
        .n_from_type("person")
        .collect_to::<Vec<_>>();

    let remapping_vals = RemappingMap::new();

    let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
    return_vals.insert(
        "users".to_string(),
        ReturnValue::from_traversal_value_array_with_mixin(
            G::new_from(Arc::clone(&storage), &txn, traversal.clone())
            .map_traversal(|item, _txn| {
                exists_remapping!(remapping_vals, item.clone(), false, "has_friends" => G::new_from(Arc::clone(&storage), &txn, vec![item.clone()]).out("knows", &EdgeType::Node).collect_to::<Vec<_>>())?;
                Ok(item)
            })
            .collect_to::<Vec<_>>()
            .clone(),
            remapping_vals.borrow_mut(),
        ),
    );

    assert_eq!(return_vals.len(), 1);

    let value = match return_vals.get("users").unwrap() {
        ReturnValue::Array(array) => match array.first().unwrap() {
            ReturnValue::Object(object) => object,
            _ => panic!("Expected Node"),
        },
        _ => panic!("Expected Array"),
    };

    assert_eq!(*value.get("has_friends").unwrap(), ReturnValue::from(true));
}

#[test]
fn test_one_of_each_remapping() {
    let (storage, _temp_dir) = setup_test_db();
    let mut txn = storage.graph_env.write_txn().unwrap();

    let _node = G::new_mut(Arc::clone(&storage), &mut txn)
        .add_n("person", Some(props! { "old_name" => "test" }), None)
        .collect_to_obj();
    let _other_node = G::new_mut(Arc::clone(&storage), &mut txn)
        .add_n("person", Some(props! {}), None)
        .collect_to_obj();
    let _edge = G::new_mut(Arc::clone(&storage), &mut txn)
        .add_e(
            "knows",
            Some(props!()),
            _node.id(),
            _other_node.id(),
            false,
            EdgeType::Node,
        )
        .collect_to_obj();

    let traversal = G::new(Arc::clone(&storage), &txn)
        .n_from_type("person")
        .collect_to::<Vec<_>>();

    let remapping_vals = RemappingMap::new();

    let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
    return_vals.insert(
        "users".to_string(),
        ReturnValue::from_traversal_value_array_with_mixin(
            G::new_from(Arc::clone(&storage), &txn, traversal.clone())
            .map_traversal(|item, _txn| {
                field_remapping!(remapping_vals, item.clone(), false, "old_name" => "new_name")?;
                traversal_remapping!(remapping_vals, item.clone(), false, "traversal" => G::new_from(Arc::clone(&storage), &txn, vec![item.clone()]).out("knows", &EdgeType::Node).collect_to::<Vec<_>>())?;
                identifier_remapping!(remapping_vals, item.clone(), false, "identifier" => "new_value")?;
                value_remapping!(remapping_vals, item.clone(), false, "value" => "new_value")?;
                exists_remapping!(remapping_vals, item.clone(), false, "exists" => G::new_from(Arc::clone(&storage), &txn, vec![item.clone()]).out("knows", &EdgeType::Node).collect_to::<Vec<_>>())?;
                Ok(item)
            })
            .collect_to::<Vec<_>>()
            .clone(),
            remapping_vals.borrow_mut(),
        ),
    );

    assert_eq!(return_vals.len(), 1);

    let value = match return_vals.get("users").unwrap() {
        ReturnValue::Array(array) => match array.first().unwrap() {
            ReturnValue::Object(object) => object,
            _ => panic!("Expected Node"),
        },
        _ => panic!("Expected Array"),
    };

    assert_eq!(
        *value.get("new_name").unwrap(),
        ReturnValue::from("test".to_string())
    );
    assert_eq!(
        *match value.get("traversal").unwrap() {
            ReturnValue::Array(array) => match array.first().unwrap() {
                ReturnValue::Object(object) => object.get("id").unwrap(),
                _ => panic!("Expected Object"),
            },
            _ => panic!("Expected Array"),
        },
        ReturnValue::from(_other_node.uuid())
    );
    assert_eq!(
        *value.get("identifier").unwrap(),
        ReturnValue::from("new_value".to_string())
    );
    assert_eq!(
        *value.get("value").unwrap(),
        ReturnValue::from("new_value".to_string())
    );
    assert_eq!(*value.get("exists").unwrap(), ReturnValue::from(true));
}

#[test]
fn test_nested_remapping() {
    let (storage, _temp_dir) = setup_test_db();
    let mut txn = storage.graph_env.write_txn().unwrap();

    let _node = G::new_mut(Arc::clone(&storage), &mut txn)
        .add_n("person", Some(props! { "old_name" => "test" }), None)
        .collect_to_obj();
    let _other_node = G::new_mut(Arc::clone(&storage), &mut txn)
        .add_n("thing", Some(props! {}), None)
        .collect_to_obj();
    let _edge = G::new_mut(Arc::clone(&storage), &mut txn)
        .add_e(
            "knows",
            Some(props!()),
            _node.id(),
            _other_node.id(),
            false,
            EdgeType::Node,
        )
        .collect_to_obj();

    let user = G::new(Arc::clone(&storage), &txn)
        .n_from_id(&_node.id())
        .collect_to::<Vec<_>>();

    let remapping_vals = RemappingMap::new();

    let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
    let traversal = G::new_from(Arc::clone(&storage), &txn, user.clone())
    .map_traversal(|item, _txn| {
        traversal_remapping!(remapping_vals, item.clone(), false, "nested" => 
            G::new_from(Arc::clone(&storage), &txn, vec![item.clone()]).out("knows", &EdgeType::Node).map_traversal(|node, _txn| {
                println!("node: {node:?}");
                value_remapping!(remapping_vals, node.clone(), false, "old_name" => "new_name")?;
                Ok(node)
            }).collect_to::<Vec<_>>())?;
        Ok(item)
    })
    .collect_to::<Vec<_>>();

    println!("traversal: {traversal:#?}");
    println!("remapping_vals: {:#?}", remapping_vals.borrow_mut());

    return_vals.insert(
        "user".to_string(),
        ReturnValue::from_traversal_value_array_with_mixin(traversal, remapping_vals.borrow_mut()),
    );

    assert_eq!(return_vals.len(), 1);
    println!("value: {return_vals:#?}");

    let value = match return_vals.get("user").unwrap() {
        ReturnValue::Array(array) => match array.first().unwrap() {
            ReturnValue::Object(object) => object,
            _ => panic!("Expected Node"),
        },
        _ => panic!("Expected Array"),
    };

    assert_eq!(
        *value.get("nested").unwrap(),
        ReturnValue::Array(vec![ReturnValue::Object(HashMap::from([(
            "old_name".to_string(),
            ReturnValue::from("new_name".to_string())
        )]))])
    );
}

#[test]
fn test_double_nested_remapping() {
    let (storage, _temp_dir) = setup_test_db();
    let mut txn = storage.graph_env.write_txn().unwrap();

    let _node = G::new_mut(Arc::clone(&storage), &mut txn)
        .add_n("person", Some(props! {}), None)
        .collect_to_obj();
    let _other_node = G::new_mut(Arc::clone(&storage), &mut txn)
        .add_n("thing", Some(props! {}), None)
        .collect_to_obj();
    let _edge = G::new_mut(Arc::clone(&storage), &mut txn)
        .add_e(
            "knows",
            Some(props!()),
            _node.id(),
            _other_node.id(),
            false,
            EdgeType::Node,
        )
        .collect_to_obj();

    let user = G::new(Arc::clone(&storage), &txn)
        .n_from_id(&_node.id())
        .collect_to::<Vec<_>>();

    let remapping_vals = RemappingMap::new();

    let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
    let traversal = G::new_from(Arc::clone(&storage), &txn, user.clone())
    .map_traversal(|item, _txn| {
        traversal_remapping!(remapping_vals, item.clone(), false, "nested" =>
            G::new_from(Arc::clone(&storage), &txn, vec![item.clone()]).out("knows", &EdgeType::Node).map_traversal(|node, _txn| {
                traversal_remapping!(remapping_vals, node.clone(), false, "nested" => 
                    G::new_from(Arc::clone(&storage), &txn, vec![node.clone()]).in_("knows", &EdgeType::Node).map_traversal(|node, _txn| {
                        println!("node: {node:?}");
                        value_remapping!(remapping_vals, node.clone(), false, "old_name" => "new_name")?;
                        Ok(node)
                    }).collect_to::<Vec<_>>())?;
                Ok(node)
            }).collect_to::<Vec<_>>())?;
        
        Ok(item)
    })
    .collect_to::<Vec<_>>();

    println!("traversal: {traversal:#?}");
    println!("remapping_vals: {:#?}", remapping_vals.borrow_mut());

    return_vals.insert(
        "user".to_string(),
        ReturnValue::from_traversal_value_array_with_mixin(traversal, remapping_vals.borrow_mut()),
    );

    assert_eq!(return_vals.len(), 1);
    println!("value: {return_vals:#?}");

    let to_object = |value: &ReturnValue| match value {
        ReturnValue::Array(array) => match array.first().unwrap() {
            ReturnValue::Object(object) => object.clone(),
            _ => panic!("Expected Node"),
        },
        _ => panic!("Expected Array"),
    };

    let value = to_object(return_vals.get("user").unwrap());
    let nested = to_object(value.get("nested").unwrap());


    assert_eq!(
        *nested.get("nested").unwrap(),
        ReturnValue::Array(vec![ReturnValue::Object(HashMap::from([(
            "old_name".to_string(),
            ReturnValue::from("new_name".to_string())
        )]))])
    );
}

#[test]
fn test_nested_with_other_remapping() {
    let (storage, _temp_dir) = setup_test_db();
    let mut txn = storage.graph_env.write_txn().unwrap();

    let _node = G::new_mut(Arc::clone(&storage), &mut txn)
        .add_n("person", Some(props! { "old_name" => "test" }), None)
        .collect_to_obj();
    let _other_node = G::new_mut(Arc::clone(&storage), &mut txn)
        .add_n("person", Some(props! {}), None)
        .collect_to_obj();
    let _edge = G::new_mut(Arc::clone(&storage), &mut txn)
        .add_e(
            "knows",
            Some(props!()),
            _node.id(),
            _other_node.id(),
            false,
            EdgeType::Node,
        )
        .collect_to_obj();

    let traversal = G::new(Arc::clone(&storage), &txn)
        .n_from_type("person")
        .collect_to::<Vec<_>>();

    let remapping_vals = RemappingMap::new();

    let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
    return_vals.insert(
        "users".to_string(),
        ReturnValue::from_traversal_value_array_with_mixin(
            G::new_from(Arc::clone(&storage), &txn, traversal.clone())
            .map_traversal(|item, _txn| {
                field_remapping!(remapping_vals, item.clone(), false, "old_name" => "new_name")?;
                traversal_remapping!(remapping_vals, item.clone(), false, "traversal" => G::new_from(Arc::clone(&storage), &txn, vec![item.clone()]).out("knows", &EdgeType::Node).map_traversal(|node, _txn| {
                    value_remapping!(remapping_vals, node.clone(), false, "new_field" => "new_name")?;
                    Ok(node)
                }).collect_to::<Vec<_>>())?;
                identifier_remapping!(remapping_vals, item.clone(), false, "identifier" => "new_value")?;
                value_remapping!(remapping_vals, item.clone(), false, "value" => "new_value")?;
                exists_remapping!(remapping_vals, item.clone(), false, "exists" => G::new_from(Arc::clone(&storage), &txn, vec![item.clone()]).out("knows", &EdgeType::Node).collect_to::<Vec<_>>())?;
                Ok(item)
            })
            .collect_to::<Vec<_>>()
            .clone(),
            remapping_vals.borrow_mut(),
        ),
    );

    assert_eq!(return_vals.len(), 1);

    let value = match return_vals.get("users").unwrap() {
        ReturnValue::Array(array) => match array.first().unwrap() {
            ReturnValue::Object(object) => object,
            _ => panic!("Expected Node"),
        },
        _ => panic!("Expected Array"),
    };

    assert_eq!(
        *value.get("new_name").unwrap(),
        ReturnValue::from("test".to_string())
    );
    assert_eq!(
        *match value.get("traversal").unwrap() {
            ReturnValue::Array(array) => match array.first().unwrap() {
                ReturnValue::Object(object) => object.get("new_field").unwrap(),
                _ => panic!("Expected Object"),
            },
            _ => panic!("Expected Array"),
        },
        ReturnValue::from("new_name".to_string())
    );
    assert!(
        match value.get("traversal").unwrap() {
            ReturnValue::Array(array) => match array.first().unwrap() {
                ReturnValue::Object(object) => {
                    println!("object: {:#?}", object);
                    !object.contains_key("id")
                },
                _ => {
                    panic!("Expected Object")
                }
            },
            _ => panic!("Expected Array"),
        }
    );
    assert_eq!(
        *value.get("identifier").unwrap(),
        ReturnValue::from("new_value".to_string())
    );
    assert_eq!(
        *value.get("value").unwrap(),
        ReturnValue::from("new_value".to_string())
    );
    assert_eq!(*value.get("exists").unwrap(), ReturnValue::from(true));
}


#[test]
#[ignore]
fn test_remapping_with_traversal_from_source() {
    let (storage, _temp_dir) = setup_test_db();
    let mut txn = storage.graph_env.write_txn().unwrap();

    let _node = G::new_mut(Arc::clone(&storage), &mut txn)
        .add_n("person", Some(props! { "old_name" => "test" }), None)
        .collect_to_obj();
    let _other_node = G::new_mut(Arc::clone(&storage), &mut txn)
        .add_n("person", Some(props! { "other_name" => "other" }), None)
        .collect_to_obj();
    let _edge = G::new_mut(Arc::clone(&storage), &mut txn)
        .add_e(
            "knows",
            Some(props!()),
            _node.id(),
            _other_node.id(),
            false,
            EdgeType::Node,
        )
        .collect_to_obj();

        let remapping_vals = RemappingMap::new();

        let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
        return_vals.insert(
            "users".to_string(),
            ReturnValue::from_traversal_value_array_with_mixin(
G::new(Arc::clone(&storage), &txn)
        .n_from_type("person")
        .map_traversal(|item, _txn| {
            traversal_remapping!(remapping_vals, item.clone(), false, "traversal" => G::new(Arc::clone(&storage), &txn).n_from_id(&_other_node.id()).collect_to::<Vec<_>>())?;
            Ok(item)
        })
        .collect_to::<Vec<_>>()
        .clone(),
            remapping_vals.borrow_mut(),
        ),
    );


    assert_eq!(return_vals.len(), 1);

    let value = match return_vals.get("users").unwrap() {
        ReturnValue::Array(array) => match array.first().unwrap() {
            ReturnValue::Object(object) => object,
            _ => panic!("Expected Node"),
        },
        _ => panic!("Expected Array"),
    };

    assert_eq!(
        *value.get("new_name").unwrap(),
        ReturnValue::from("test".to_string())
    );
}