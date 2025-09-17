use std::sync::Arc;

use crate::{
    helix_engine::{
        storage_core::HelixGraphStorage,
        traversal_core::{
            ops::{
                g::G,
                source::{add_n::AddNAdapter, n_from_id::NFromIdAdapter},
                util::update::UpdateAdapter,
            },
            traversal_value::Traversable,
        },
    },
    props,
};

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
fn test_update_node() {
    let (storage, _temp_dir) = setup_test_db();
    let mut txn = storage.graph_env.write_txn().unwrap();

    let node = G::new_mut(Arc::clone(&storage), &mut txn)
        .add_n("person", Some(props!("name" => "test")), None)
        .collect_to_obj();
    let _ = G::new_mut(Arc::clone(&storage), &mut txn)
        .add_n("person", Some(props!("name" => "test2")), None)
        .collect_to_obj();

    txn.commit().unwrap();
    let mut txn = storage.graph_env.write_txn().unwrap();
    let _ = {
        let update_tr = G::new(Arc::clone(&storage), &txn)
            .n_from_id(&node.id())
            .collect_to::<Vec<_>>();
        G::new_mut_from(Arc::clone(&storage), &mut txn, update_tr)
            .update(Some(props! { "name" => "john"}))
            .collect_to::<Vec<_>>()
    };
    txn.commit().unwrap();
    let txn = storage.graph_env.read_txn().unwrap();
    let updated_users = G::new(Arc::clone(&storage), &txn)
        .n_from_id(&node.id())
        .collect_to::<Vec<_>>();
    assert_eq!(updated_users.len(), 1);
    assert_eq!(
        updated_users[0]
            .check_property("name")
            .unwrap()
            .into_owned()
            .inner_stringify(),
        "john"
    );
}
