// DEFAULT CODE
// use helix_db::helix_engine::traversal_core::config::Config;

// pub fn config() -> Option<Config> {
//     None
// }

use bumpalo::Bump;
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
                    from_n::FromNAdapter,
                    from_v::FromVAdapter,
                    out::{OutAdapter, OutAdapterArena},
                    out_e::OutEdgesAdapter,
                },
                source::{
                    add_e::{AddEAdapter, EdgeType},
                    add_n::AddNAdapter,
                    e_from_id::EFromIdAdapter,
                    e_from_type::EFromTypeAdapter,
                    n_from_id::{NFromIdAdapter, NFromIdAdapterArena},
                    n_from_index::NFromIndexAdapter,
                    n_from_type::NFromTypeAdapter,
                    v_from_id::VFromIdAdapter,
                    v_from_type::VFromTypeAdapter,
                },
                util::{
                    aggregate::AggregateAdapter,
                    count::CountAdapter,
                    dedup::DedupAdapter,
                    drop::Drop,
                    exist::Exist,
                    filter_mut::FilterMut,
                    filter_ref::{FilterRefAdapter, FilterRefAdapterArena},
                    group_by::GroupByAdapter,
                    map::{MapAdapter, MapAdapterArena},
                    order::OrderByAdapter,
                    paths::{PathAlgorithm, ShortestPathAdapter},
                    props::PropsAdapter,
                    range::RangeAdapter,
                    update::UpdateAdapter,
                },
                vectors::{
                    brute_force_search::BruteForceSearchVAdapter, insert::InsertVAdapter,
                    search::SearchVAdapter,
                },
            },
            traversal_value::{Traversable, TraversalValue},
            traversal_value_arena::{Traversable as TraversableArena, TraversalValueArena},
        },
        types::GraphError,
        vector_core::vector::HVector,
    },
    helix_gateway::{
        embedding_providers::{EmbeddingModel, get_embedding_model},
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
            Value,
            casting::{CastType, cast},
        },
    },
    traversal_remapping,
    utils::{
        count::Count,
        filterable::Filterable,
        id::ID,
        items::{self, Edge, Node},
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
        db_max_size_gb: Some(10),
        mcp: Some(true),
        bm25: Some(true),
        schema: Some(
            r#"{
  "schema": {
    "nodes": [
      {
        "name": "User",
        "properties": {
          "label": "String",
          "country": "U8",
          "id": "ID"
        }
      }
    ],
    "vectors": [
      {
        "name": "Item",
        "properties": {
          "id": "ID",
          "category": "U16",
          "data": "Array(F64)",
          "score": "F64",
          "label": "String"
        }
      }
    ],
    "edges": [
      {
        "name": "Interacted",
        "from": "User",
        "to": "Item",
        "properties": {}
      }
    ]
  },
  "queries": [
    {
      "name": "OneHopFilter",
      "parameters": {
        "user_id": "ID",
        "category": "U16"
      },
      "returns": []
    }
  ]
}"#
            .to_string(),
        ),
        embedding_model: Some("text-embedding-ada-002".to_string()),
        graphvis_node_label: None,
    });
}

pub struct User {
    pub country: u8,
}

pub struct Interacted {
    pub from: User,
    pub to: Item,
}

pub struct Item {
    pub category: u16,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct OneHopFilterInput {
    pub user_id: ID,
    pub category: u16,
}
#[handler]
pub fn OneHopFilter(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let arena = Bump::new();
    let data = input
        .request
        .in_fmt
        .deserialize::<OneHopFilterInput>(&input.request.body)?;
    let mut remapping_vals = RemappingMap::new();
    let txn = db
        .graph_env
        .read_txn()
        .map_err(|e| GraphError::New(format!("Failed to start read transaction: {:?}", e)))?;

    let items = G::new_with_arena(&arena, &db, &txn)
        .n_from_id(&data.user_id)
        .out_vec("Interacted")
        .filter_ref(|val, txn| {
            if let Ok(val) = val {
                Ok(val
                    .check_property("category")
                    .map_or(false, |v| *v == data.category.clone()))
            } else {
                Ok(false)
            }
        })
        .map_traversal(|item: TraversalValueArena, txn| {
            field_remapping!(remapping_vals, item, false, "id" => "id")?;
            identifier_remapping!(remapping_vals, item, false, "category" => "category")?;
            Ok(item)
        })
        .collect_to::<Vec<_>>();
    let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
    return_vals.insert(
        "items".to_string(),
        ReturnValue::from_traversal_value_array_arena_with_mixin(
            items,
            remapping_vals.borrow_mut(),
        ),
    );

    txn.commit()
        .map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
    Ok(input.request.out_fmt.create_response(&return_vals))
}
