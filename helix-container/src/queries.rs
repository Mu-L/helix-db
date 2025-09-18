// DEFAULT CODE
// use helix_db::helix_engine::traversal_core::config::Config;

// pub fn config() -> Option<Config> {
//     None
// }

use chrono::{DateTime, Utc};
use heed3::RoTxn;
use helix_db::{
    embed, embed_async, exclude_field, field_addition_from_old_field, field_addition_from_value,
    field_remapping, field_type_cast,
    helix_engine::{
        traversal_core::{
            config::{Config, GraphConfig, VectorConfig},
            ops::{
                bm25::search_bm25::SearchBM25Adapter,
                g::G,
                in_::{in_::InAdapter, in_e::InEdgesAdapter, to_n::ToNAdapter, to_v::ToVAdapter},
                out::{
                    from_n::FromNAdapter, from_v::FromVAdapter, out::OutAdapter,
                    out_e::OutEdgesAdapter,
                },
                source::{
                    add_e::{AddEAdapter, EdgeType},
                    add_n::AddNAdapter,
                    e_from_id::EFromIdAdapter,
                    e_from_type::EFromTypeAdapter,
                    n_from_id::NFromIdAdapter,
                    n_from_index::NFromIndexAdapter,
                    n_from_type::NFromTypeAdapter,
                },
                util::{
                    aggregate::AggregateAdapter, dedup::DedupAdapter, drop::Drop, exist::Exist,
                    filter_mut::FilterMut, filter_ref::FilterRefAdapter, group_by::GroupByAdapter,
                    map::MapAdapter, order::OrderByAdapter, paths::ShortestPathAdapter,
                    props::PropsAdapter, range::RangeAdapter, update::UpdateAdapter,
                },
                vectors::{
                    brute_force_search::BruteForceSearchVAdapter, insert::InsertVAdapter,
                    search::SearchVAdapter,
                },
            },
            traversal_value::{Traversable, TraversalValue},
        },
        types::GraphError,
        vector_core::vector::HVector,
    },
    helix_gateway::{
        embedding_providers::embedding_providers::{get_embedding_model, EmbeddingModel},
        mcp::mcp::{MCPHandler, MCPHandlerSubmission, MCPToolInput},
        router::router::{HandlerInput, IoContFn},
    },
    identifier_remapping, node_matches, props,
    protocol::{
        format::Format,
        remapping::{Remapping, RemappingMap, ResponseRemapping},
        response::Response,
        return_values::ReturnValue,
        value::{
            casting::{cast, CastType},
            Value,
        },
    },
    traversal_remapping,
    utils::{
        count::Count,
        filterable::Filterable,
        id::ID,
        items::{Edge, Node},
    },
    value_remapping,
};
use helix_macros::{handler, mcp_handler, migration, tool_call};
use sonic_rs::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

pub fn config() -> Option<Config> {
    return Some(Config {
        vector_config: Some(VectorConfig {
            m: Some(16),
            ef_construction: Some(128),
            ef_search: Some(768),
        }),
        graph_config: Some(GraphConfig {
            secondary_indices: Some(vec![]),
        }),
        db_max_size_gb: Some(20),
        mcp: Some(true),
        bm25: Some(true),
        schema: Some(
            r#"{
  "schema": {
    "nodes": [
      {
        "name": "User",
        "properties": {
          "id": "ID",
          "email": "String",
          "age": "U8",
          "name": "String"
        }
      }
    ],
    "vectors": [],
    "edges": []
  },
  "queries": [
    {
      "name": "GetUsersWithAlias",
      "parameters": {},
      "returns": []
    },
    {
      "name": "GetFilteredUsers",
      "parameters": {},
      "returns": [
        "users"
      ]
    },
    {
      "name": "CreateUser",
      "parameters": {
        "age": "U8",
        "name": "String",
        "email": "String"
      },
      "returns": [
        "user"
      ]
    },
    {
      "name": "GetUserDisplayInfo",
      "parameters": {},
      "returns": []
    }
  ]
}"#
            .to_string(),
        ),
        embedding_model: None,
        graphvis_node_label: None,
    });
}

pub struct User {
    pub name: String,
    pub age: u8,
    pub email: String,
}

#[handler]
pub fn GetUsersWithAlias(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let mut remapping_vals = RemappingMap::new();
    let txn = db.graph_env.read_txn().unwrap();
    let users = G::new(Arc::clone(&db), &txn)
        .n_from_type("User")
        .range(0, 5)
        .collect_to::<Vec<_>>();
    let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
    return_vals.insert("users".to_string(), ReturnValue::from_traversal_value_array_with_mixin(G::new_from(Arc::clone(&db), &txn, users.clone())

.map_traversal(|item, txn| { identifier_remapping!(remapping_vals, item.clone(), true, "userID" => G::new_from(Arc::clone(&db), &txn, vec![item.clone()])
.check_property("ID").collect_to_obj())?;
 Ok(item) }).collect_to::<Vec<_>>(), remapping_vals.borrow_mut()));

    txn.commit().unwrap();
    Ok(input.request.out_fmt.create_response(&return_vals))
}

#[handler]
pub fn GetFilteredUsers(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let mut remapping_vals = RemappingMap::new();
    let txn = db.graph_env.read_txn().unwrap();
    let users = G::new(Arc::clone(&db), &txn)
        .n_from_type("User")
        .filter_ref(|val, txn| {
            if let Ok(val) = val {
                Ok((G::new_from(Arc::clone(&db), &txn, val.clone())
                    .check_property("age")
                    .map_value_or(false, |v| *v > 18)?
                    && (G::new_from(Arc::clone(&db), &txn, val.clone())
                        .check_property("name")
                        .map_value_or(false, |v| *v == "Alice")?
                        || G::new_from(Arc::clone(&db), &txn, val.clone())
                            .check_property("name")
                            .map_value_or(false, |v| *v == "Bob")?)))
            } else {
                Ok(false)
            }
        })
        .collect_to::<Vec<_>>();
    let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
    return_vals.insert(
        "users".to_string(),
        ReturnValue::from_traversal_value_array_with_mixin(
            users.clone(),
            remapping_vals.borrow_mut(),
        ),
    );

    txn.commit().unwrap();
    Ok(input.request.out_fmt.create_response(&return_vals))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct CreateUserInput {
    pub name: String,
    pub age: u8,
    pub email: String,
}
#[handler]
pub fn CreateUser(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<CreateUserInput>(&input.request.body)?;
    let mut remapping_vals = RemappingMap::new();
    let mut txn = db.graph_env.write_txn().unwrap();
    let user = G::new_mut(Arc::clone(&db), &mut txn)
        .add_n(
            "User",
            Some(props! { "email" => &data.email, "name" => &data.name, "age" => &data.age }),
            None,
        )
        .collect_to_obj();
    let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
    return_vals.insert(
        "user".to_string(),
        ReturnValue::from_traversal_value_with_mixin(user.clone(), remapping_vals.borrow_mut()),
    );

    txn.commit().unwrap();
    Ok(input.request.out_fmt.create_response(&return_vals))
}

#[handler]
pub fn GetUserDisplayInfo(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let mut remapping_vals = RemappingMap::new();
    let txn = db.graph_env.read_txn().unwrap();
    let users = G::new(Arc::clone(&db), &txn)
        .n_from_type("User")
        .range(0, 10)
        .collect_to::<Vec<_>>();
    let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
    return_vals.insert("users".to_string(), ReturnValue::from_traversal_value_array_with_mixin(G::new_from(Arc::clone(&db), &txn, users.clone())

.map_traversal(|item, txn| { identifier_remapping!(remapping_vals, item.clone(), false, "displayName" => G::new_from(Arc::clone(&db), &txn, vec![item.clone()])

.check_property("name").collect_to_obj())?;
identifier_remapping!(remapping_vals, item.clone(), false, "userAge" => G::new_from(Arc::clone(&db), &txn, vec![item.clone()])

.check_property("age").collect_to_obj())?;
 Ok(item) }).collect_to::<Vec<_>>(), remapping_vals.borrow_mut()));

    txn.commit().unwrap();
    Ok(input.request.out_fmt.create_response(&return_vals))
}
