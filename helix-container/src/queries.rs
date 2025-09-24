
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
                    aggregate::AggregateAdapter, group_by::GroupByAdapter, count::CountAdapter,
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
db_max_size_gb: Some(20),
mcp: Some(true),
bm25: Some(true),
schema: Some(r#"{
  "schema": {
    "nodes": [
      {
        "name": "FUNCTION",
        "properties": {
          "name": "String",
          "updated_at": "String",
          "id": "ID",
          "created_at": "String",
          "code": "String"
        }
      }
    ],
    "vectors": [
      {
        "name": "CODE_CHUNK",
        "properties": {
          "id": "ID"
        }
      }
    ],
    "edges": [
      {
        "name": "CALLS",
        "from": "FUNCTION",
        "to": "FUNCTION",
        "properties": {
          "created_at": "String"
        }
      },
      {
        "name": "HAS_EMBEDDING",
        "from": "FUNCTION",
        "to": "CODE_CHUNK",
        "properties": {
          "created_at": "String"
        }
      }
    ]
  },
  "queries": [
    {
      "name": "find_relevant_callees",
      "parameters": {
        "query_text": "String",
        "k": "I64",
        "function_id": "ID"
      },
      "returns": []
    }
  ]
}"#.to_string()),
embedding_model: Some("text-embedding-3-small".to_string()),
graphvis_node_label: None,
})
}

pub struct FUNCTION {
    pub name: String,
    pub code: String,
    pub created_at: String,
    pub updated_at: String,
}

pub struct CALLS {
    pub from: FUNCTION,
    pub to: FUNCTION,
    pub created_at: String,
}

pub struct HAS_EMBEDDING {
    pub from: FUNCTION,
    pub to: CODE_CHUNK,
    pub created_at: String,
}

pub struct CODE_CHUNK {
}

#[derive(Serialize, Deserialize, Clone)]
pub struct find_relevant_calleesInput {

pub function_id: ID,
pub query_text: String,
pub k: i64
}
#[handler]
pub fn find_relevant_callees (input: HandlerInput) -> Result<Response, GraphError> {
let db = Arc::clone(&input.graph.storage);
let data = input.request.in_fmt.deserialize::<find_relevant_calleesInput>(&input.request.body)?.into_owned();
Err(IoContFn::create_err(move |__internal_cont_tx, __internal_ret_chan| Box::pin(async move {
let __internal_embed_data_0 = embed_async!(db, &data.query_text);
__internal_cont_tx.send_async((__internal_ret_chan, Box::new(move || {
let __internal_embed_data_0: Vec<f64> = __internal_embed_data_0?;
let mut remapping_vals = RemappingMap::new();
let txn = db.graph_env.read_txn().unwrap();
    let callees = G::new(Arc::clone(&db), &txn)
.n_from_id(&data.function_id)

.out("CALLS",&EdgeType::Node).collect_to::<Vec<_>>();
    let similar_chunks = G::new(Arc::clone(&db), &txn)
.search_v::<fn(&HVector, &RoTxn) -> bool, _>(&__internal_embed_data_0, data.k.clone(), "CODE_CHUNK", None).collect_to::<Vec<_>>();
    let similar_functions = G::new_from(Arc::clone(&db), &txn, similar_chunks.clone())

.in_("HAS_EMBEDDING",&EdgeType::Node).collect_to::<Vec<_>>();
    let relevant_callees = G::new_from(Arc::clone(&db), &txn, callees.clone())

.filter_ref(|val, txn|{
                if let Ok(val) = val { 
                    Ok(Exist::exists(&mut G::new_from(Arc::clone(&db), &txn, similar_functions.clone())

.filter_ref(|val, txn|{
                if let Ok(val) = val { 
                    Ok(G::new_from(Arc::clone(&db), &txn, val.clone())

.check_property("id")

.map_value_or(false, |v| *v == G::new_from(Arc::clone(&db), &txn, callees.clone())

.check_property("id").collect_to_value())?)
                } else {
                    Ok(false)
                }
            })))
                } else {
                    Ok(false)
                }
            }).collect_to::<Vec<_>>();
let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
        return_vals.insert("relevant_callees".to_string(), ReturnValue::from_traversal_value_array_with_mixin(G::new_from(Arc::clone(&db), &txn, relevant_callees.clone())

.map_traversal(|item, txn| { identifier_remapping!(remapping_vals, item.clone(), false, "id" => G::new_from(Arc::clone(&db), &txn, vec![item.clone()])

.check_property("id").collect_to_obj())?;
identifier_remapping!(remapping_vals, item.clone(), false, "name" => G::new_from(Arc::clone(&db), &txn, vec![item.clone()])

.check_property("name").collect_to_obj())?;
 Ok(item) }).collect_to::<Vec<_>>(), remapping_vals.borrow_mut()));

txn.commit().unwrap();
Ok(input.request.out_fmt.create_response(&return_vals))
}))).await.expect("Cont Channel should be alive")
})))
}


