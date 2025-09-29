
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
                    filter_ref::FilterRefAdapter, map::MapAdapter, paths::{PathAlgorithm, ShortestPathAdapter},
                    props::PropsAdapter, range::RangeAdapter, update::UpdateAdapter, order::OrderByAdapter,
                    aggregate::AggregateAdapter, group_by::GroupByAdapter,
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
        value::{casting::{cast, CastType}, Value},
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
db_max_size_gb: Some(10),
mcp: Some(true),
bm25: Some(true),
schema: Some(r#"{
  "schema": {
    "nodes": [
      {
        "name": "City",
        "properties": {
          "name": "String",
          "id": "ID"
        }
      }
    ],
    "vectors": [],
    "edges": [
      {
        "name": "Road",
        "from": "City",
        "to": "City",
        "properties": {
          "weight": "F64"
        }
      }
    ]
  },
  "queries": [
    {
      "name": "shortestPathDefault",
      "parameters": {
        "from": "ID",
        "to": "ID"
      },
      "returns": [
        "path"
      ]
    },
    {
      "name": "shortestPathDijkstra",
      "parameters": {
        "from": "ID",
        "to": "ID"
      },
      "returns": [
        "path"
      ]
    },
    {
      "name": "shortestPathBFS",
      "parameters": {
        "from": "ID",
        "to": "ID"
      },
      "returns": [
        "path"
      ]
    }
  ]
}"#.to_string()),
embedding_model: None,
graphvis_node_label: None,
})
}

pub struct City {
    pub name: String,
}

pub struct Road {
    pub from: City,
    pub to: City,
    pub weight: f64,
}


#[derive(Serialize, Deserialize, Clone)]
pub struct shortestPathDefaultInput {

pub from: ID,
pub to: ID
}
#[handler]
pub fn shortestPathDefault (input: HandlerInput) -> Result<Response, GraphError> {
let db = Arc::clone(&input.graph.storage);
let data = input.request.in_fmt.deserialize::<shortestPathDefaultInput>(&input.request.body)?;
let mut remapping_vals = RemappingMap::new();
let txn = db.graph_env.read_txn().unwrap();
    let path = G::new(Arc::clone(&db), &txn)
.n_from_id(&data.from)

.shortest_path(Some("Road"), None, Some(&data.to)).collect_to::<Vec<_>>();
let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
        return_vals.insert("path".to_string(), ReturnValue::from_traversal_value_array_with_mixin(path.clone(), remapping_vals.borrow_mut()));

txn.commit().unwrap();
Ok(input.request.out_fmt.create_response(&return_vals))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct shortestPathDijkstraInput {

pub from: ID,
pub to: ID
}
#[handler]
pub fn shortestPathDijkstra (input: HandlerInput) -> Result<Response, GraphError> {
let db = Arc::clone(&input.graph.storage);
let data = input.request.in_fmt.deserialize::<shortestPathDijkstraInput>(&input.request.body)?;
let mut remapping_vals = RemappingMap::new();
let txn = db.graph_env.read_txn().unwrap();
    let path = G::new(Arc::clone(&db), &txn)
.n_from_id(&data.from)

.shortest_path(Some("Road"), None, Some(&data.to)).collect_to::<Vec<_>>();
let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
        return_vals.insert("path".to_string(), ReturnValue::from_traversal_value_array_with_mixin(path.clone(), remapping_vals.borrow_mut()));

txn.commit().unwrap();
Ok(input.request.out_fmt.create_response(&return_vals))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct shortestPathBFSInput {

pub from: ID,
pub to: ID
}
#[handler]
pub fn shortestPathBFS (input: HandlerInput) -> Result<Response, GraphError> {
let db = Arc::clone(&input.graph.storage);
let data = input.request.in_fmt.deserialize::<shortestPathBFSInput>(&input.request.body)?;
let mut remapping_vals = RemappingMap::new();
let txn = db.graph_env.read_txn().unwrap();
    let path = G::new(Arc::clone(&db), &txn)
.n_from_id(&data.from)

.shortest_path(Some("Road"), None, Some(&data.to)).collect_to::<Vec<_>>();
let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
        return_vals.insert("path".to_string(), ReturnValue::from_traversal_value_array_with_mixin(path.clone(), remapping_vals.borrow_mut()));

txn.commit().unwrap();
Ok(input.request.out_fmt.create_response(&return_vals))
}


