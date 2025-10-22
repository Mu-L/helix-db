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
                    from_n::FromNAdapter, from_v::FromVAdapter, out::OutAdapter,
                    out_e::OutEdgesAdapter,
                },
                source::{
                    add_e::AddEAdapter, add_n::AddNAdapter, e_from_id::EFromIdAdapter,
                    e_from_type::EFromTypeAdapter, n_from_id::NFromIdAdapter,
                    n_from_index::NFromIndexAdapter, n_from_type::NFromTypeAdapter,
                    v_from_id::VFromIdAdapter, v_from_type::VFromTypeAdapter,
                },
                util::{
                    aggregate::AggregateAdapter,
                    count::CountAdapter,
                    dedup::DedupAdapter,
                    drop::Drop,
                    exist::Exist,
                    filter_mut::FilterMut,
                    filter_ref::FilterRefAdapter,
                    group_by::GroupByAdapter,
                    map::MapAdapter,
                    order::OrderByAdapter,
                    paths::{PathAlgorithm, ShortestPathAdapter},
                    range::RangeAdapter,
                    update::UpdateAdapter,
                },
                vectors::{
                    brute_force_search::BruteForceSearchVAdapter, insert::InsertVAdapter,
                    search::SearchVAdapter,
                },
            },
            traversal_value::TraversalValue,
        },
        types::GraphError,
        vector_core::{vector::HVector, vector_core::VectorCore},
    },
    helix_gateway::{
        embedding_providers::{EmbeddingModel, get_embedding_model},
        mcp::mcp::{MCPHandler, MCPHandlerSubmission, MCPToolInput},
        router::router::{HandlerInput, IoContFn},
    },
    identifier_remapping, node_matches, props,
    protocol::{
        format::Format,
        response::Response,
        value::{
            Value,
            casting::{CastType, cast},
        },
    },
    traversal_remapping,
    utils::{
        count::Count,
        id::{ID, uuid_str},
        items::{self, Edge, Node},
        properties::ImmutablePropertiesMap,
    },
    value_remapping,
};
use helix_macros::{handler, mcp_handler, migration, tool_call};
use sonic_rs::{Deserialize, Serialize, json};
use std::sync::Arc;
use std::time::Instant;
use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
};
use tracing::info;

pub fn config() -> Option<Config> {
    return Some(Config {
        vector_config: Some(VectorConfig {
            m: Some(16),
            ef_construction: Some(128),
            ef_search: Some(768),
        }),
        graph_config: Some(GraphConfig {
            secondary_indices: Some(vec!["key".to_string()]),
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
    let txn = db
        .graph_env
        .read_txn()
        .map_err(|e| GraphError::New(format!("Failed to start read transaction: {:?}", e)))?;

    let items = G::new(&db, &txn, &arena)
        .n_from_id(&data.user_id)
        .out_vec("Interacted", false)
        .filter(|val| {
            if let Ok(val) = val {
                val.get_property("category")
                    .map_or(false, |v| *v == data.category.clone())
            } else {
                false
            }
        })
        .filter_map(|item| item.ok())
        .map(|item: TraversalValue| OneHopOutput {
            id: uuid_str(item.id(), &arena),
            category: item.get_property("category").unwrap().inner_stringify(),
        })
        .collect::<Vec<_>>();

    let items: ReturnOneHopOutput = ReturnOneHopOutput { items };

    txn.commit()
        .map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
    Ok(input.request.out_fmt.create_response(&items))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct OneHopInput {
    pub user_id: ID,
}

#[derive(Serialize, Clone)]
pub struct OneHopOutput<'arena> {
    pub id: &'arena str,
    pub category: String,
}

#[derive(Serialize, Clone)]
pub struct ReturnOneHopOutput<'arena> {
    pub items: Vec<OneHopOutput<'arena>>,
}

#[handler]
pub fn OneHop(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let arena = Bump::new();
    let data = input
        .request
        .in_fmt
        .deserialize::<OneHopInput>(&input.request.body)?;
    let txn = db
        .graph_env
        .read_txn()
        .map_err(|e| GraphError::New(format!("Failed to start read transaction: {:?}", e)))?;

    let items = G::new(&db, &txn, &arena)
        .n_from_id(&data.user_id)
        .out_vec("Interacted", false)
        .filter_map(|item| item.ok())
        .map(|item: TraversalValue| OneHopOutput {
            id: uuid_str(item.id(), &arena),
            category: item.get_property("category").unwrap().inner_stringify(),
        })
        .collect::<Vec<_>>();

    let items: ReturnOneHopOutput = ReturnOneHopOutput { items };

    txn.commit()
        .map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
    Ok(input.request.out_fmt.create_response(&items))
}

pub struct Metadata {
    pub key: String,
    pub value: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct CreateDatasetIdInput {
    pub dataset_id: String,
}
#[handler]
pub fn CreateDatasetId(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let arena = Bump::new();
    let data = input
        .request
        .in_fmt
        .deserialize::<CreateDatasetIdInput>(&input.request.body)?;
    let mut txn = db
        .graph_env
        .write_txn()
        .map_err(|e| GraphError::New(format!("Failed to start write transaction: {:?}", e)))?;
    let metadata = G::new_mut(&db, &arena, &mut txn)
        .add_n(
            "Metadata",
            //Some(props_option(&arena, vec![("value", Value::String(data.dataset_id))])),
            Some(ImmutablePropertiesMap::new(
                1,
                vec![("value", Value::String(data.dataset_id.clone()))].into_iter(),
                &arena,
            )),
            Some(&["key"]),
        )
        .collect_to_obj();
    let response = json!({
        "metadata": metadata,
    });
    txn.commit()
        .map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
    Ok(input.request.out_fmt.create_response(&response))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct UpdateDatasetIdInput {
    pub dataset_id: String,
}
#[handler]
pub fn UpdateDatasetId(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let arena = Bump::new();
    let data = input
        .request
        .in_fmt
        .deserialize::<UpdateDatasetIdInput>(&input.request.body)?;
    let mut txn = db
        .graph_env
        .write_txn()
        .map_err(|e| GraphError::New(format!("Failed to start write transaction: {:?}", e)))?;
    Drop::drop_traversal(
        G::new(&db, &txn, &arena)
            .n_from_index("Metadata", "key", &"dataset_id")
            .collect_to::<Vec<_>>()
            .into_iter()
            .map(Ok),
        &db,
        &mut txn,
    )?;
    let metadata = G::new_mut(&db, &arena, &mut txn)
        .add_n(
            "Metadata",
            Some(ImmutablePropertiesMap::new(
                1,
                vec![("value", Value::String(data.dataset_id.clone()))].into_iter(),
                &arena,
            )),
            Some(&["key"]),
        )
        .collect_to_obj();
    let response = json!({
        "metadata": metadata,
    });

    txn.commit()
        .map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
    Ok(input.request.out_fmt.create_response(&response))
}
#[handler]
pub fn GetDatasetId(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let arena = Bump::new();
    let txn = db
        .graph_env
        .read_txn()
        .map_err(|e| GraphError::New(format!("Failed to start read transaction: {:?}", e)))?;
    let dataset_id = G::new(&db, &txn, &arena)
        .n_from_index("Metadata", "key", &"dataset_id")
        .collect_to_obj();

    let response = json!({
        "dataset_id": dataset_id.get_property("value").unwrap().inner_stringify(),
    });

    txn.commit()
        .map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
    Ok(input.request.out_fmt.create_response(&response))
}
