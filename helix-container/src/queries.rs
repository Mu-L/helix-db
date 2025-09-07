
// DEFAULT CODE
// use helix_db::helix_engine::traversal_core::config::Config;

// pub fn config() -> Option<Config> {
//     None
// }



use heed3::RoTxn;
use helix_macros::{handler, tool_call, mcp_handler, migration};
use helix_db::{
    helix_engine::{
        traversal_core::{
            config::{Config, GraphConfig, VectorConfig},
            ops::{
                bm25::search_bm25::SearchBM25Adapter,
                g::G,
                in_::{in_::InAdapter, in_e::InEdgesAdapter, to_n::ToNAdapter, to_v::ToVAdapter},
                out::{
                    from_n::FromNAdapter, from_v::FromVAdapter, out::OutAdapter, out_e::OutEdgesAdapter,
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
                    dedup::DedupAdapter, drop::Drop, exist::Exist, filter_mut::FilterMut,
                    filter_ref::FilterRefAdapter, map::MapAdapter, paths::ShortestPathAdapter,
                    props::PropsAdapter, range::RangeAdapter, update::UpdateAdapter, order::OrderByAdapter,
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
        embedding_providers::embedding_providers::{EmbeddingModel, get_embedding_model},
        router::router::{HandlerInput, IoContFn},
        mcp::mcp::{MCPHandlerSubmission, MCPToolInput, MCPHandler}
    },
    node_matches, props, embed, embed_async,
    field_remapping, identifier_remapping, 
    traversal_remapping, exclude_field, value_remapping, 
    field_addition_from_old_field, field_type_cast, field_addition_from_value,
    protocol::{
        remapping::{Remapping, RemappingMap, ResponseRemapping},
        response::Response,
        return_values::ReturnValue,
        value::{Value, casting::{CastType, cast}},
        format::Format,
    },
    utils::{
        count::Count,
        filterable::Filterable,
        id::ID,
        items::{Edge, Node},
    },
};
use sonic_rs::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;
use chrono::{DateTime, Utc};
    
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
schema: Some(r#"{
  "schema": {
    "nodes": [
      {
        "name": "User",
        "properties": {
          "id": "ID",
          "name": "String",
          "age": "I32"
        }
      }
    ],
    "vectors": [
      {
        "name": "UserVec",
        "properties": {
          "id": "ID",
          "content": "String"
        }
      },
      {
        "name": "Document",
        "properties": {
          "id": "ID",
          "created_at": "I64",
          "content": "String"
        }
      }
    ],
    "edges": [
      {
        "name": "EdgeUser",
        "from": "User",
        "to": "UserVec",
        "properties": {}
      }
    ]
  },
  "queries": [
    {
      "name": "SearchText",
      "parameters": {
        "query": "String",
        "limit": "I64"
      },
      "returns": [
        "results"
      ]
    },
    {
      "name": "user",
      "parameters": {
        "vec": "Array(F64)"
      },
      "returns": []
    },
    {
      "name": "user_with_embed",
      "parameters": {
        "text": "String"
      },
      "returns": [
        "vecs"
      ]
    }
  ]
}"#.to_string()),
embedding_model: None,
graphvis_node_label: None,
})
}

pub struct User {
    pub name: String,
    pub age: i32,
}

pub struct EdgeUser {
    pub from: User,
    pub to: UserVec,
}

pub struct UserVec {
    pub content: String,
}

pub struct Document {
    pub content: String,
    pub created_at: i64,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SearchTextInput {

pub query: String,
pub limit: i64
}
#[handler]
pub fn SearchText (input: HandlerInput) -> Result<Response, GraphError> {
let db = Arc::clone(&input.graph.storage);
let data = input.request.in_fmt.deserialize::<SearchTextInput>(&input.request.body)?.into_owned();
Err(IoContFn::create_err(move |__internal_cont_tx, __internal_ret_chan| Box::pin(async move {
let __internal_embed_data_0 = embed_async!(db, &data.query);
__internal_cont_tx.send_async((__internal_ret_chan, Box::new(move || {
let __internal_embed_data_0: Vec<f64> = __internal_embed_data_0?;
let mut remapping_vals = RemappingMap::new();
let txn = db.graph_env.read_txn().unwrap();
    let results = G::new(Arc::clone(&db), &txn)
.search_v::<fn(&HVector, &RoTxn) -> bool, _>(&__internal_embed_data_0, data.limit.clone(), "Document", None).collect_to::<Vec<_>>();
let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
        return_vals.insert("results".to_string(), ReturnValue::from_traversal_value_array_with_mixin(results.clone().clone(), remapping_vals.borrow_mut()));

txn.commit().unwrap();
Ok(input.request.out_fmt.create_response(&return_vals))
}))).await.expect("Cont Channel should be alive")
})))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct userInput {

pub vec: Vec<f64>
}
#[handler]
pub fn user (input: HandlerInput) -> Result<Response, GraphError> {
let db = Arc::clone(&input.graph.storage);
let data = input.request.in_fmt.deserialize::<userInput>(&input.request.body)?;
let mut remapping_vals = RemappingMap::new();
let txn = db.graph_env.read_txn().unwrap();
    let vecs = G::new(Arc::clone(&db), &txn)
.search_v::<fn(&HVector, &RoTxn) -> bool, _>(&data.vec, 10, "UserVec", None).collect_to::<Vec<_>>();
let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
        return_vals.insert("hello".to_string(), ReturnValue::from(Value::from("hello")));

txn.commit().unwrap();
Ok(input.request.out_fmt.create_response(&return_vals))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct user_with_embedInput {

pub text: String
}
#[handler]
pub fn user_with_embed (input: HandlerInput) -> Result<Response, GraphError> {
let db = Arc::clone(&input.graph.storage);
let data = input.request.in_fmt.deserialize::<user_with_embedInput>(&input.request.body)?.into_owned();
Err(IoContFn::create_err(move |__internal_cont_tx, __internal_ret_chan| Box::pin(async move {
let __internal_embed_data_0 = embed_async!(db, &data.text);
__internal_cont_tx.send_async((__internal_ret_chan, Box::new(move || {
let __internal_embed_data_0: Vec<f64> = __internal_embed_data_0?;
let mut remapping_vals = RemappingMap::new();
let txn = db.graph_env.read_txn().unwrap();
    let vecs = G::new(Arc::clone(&db), &txn)
.search_v::<fn(&HVector, &RoTxn) -> bool, _>(&__internal_embed_data_0, 10, "UserVec", None).collect_to::<Vec<_>>();
let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
        return_vals.insert("vecs".to_string(), ReturnValue::from_traversal_value_array_with_mixin(vecs.clone().clone(), remapping_vals.borrow_mut()));

txn.commit().unwrap();
Ok(input.request.out_fmt.create_response(&return_vals))
}))).await.expect("Cont Channel should be alive")
})))
}


