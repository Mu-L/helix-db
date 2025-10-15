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
                    n_from_type::{NFromTypeAdapter, NFromTypeAdapterArena},
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
                    range::{RangeAdapter, RangeAdapterArena},
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
        vector_core::{vector::HVector, vector_core::VectorCore},
    },
    helix_gateway::{
        embedding_providers::{get_embedding_model, EmbeddingModel},
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
            casting::{cast, CastType}, Value
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
use tracing::info;
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
            field_remapping!(remapping_vals, item, false, "category" => "category")?;
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

#[derive(Serialize, Deserialize, Clone)]
pub struct OneHopInput {
    pub user_id: ID,
}
#[handler]
pub fn OneHop(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let arena = Bump::new();
    let data = input
        .request
        .in_fmt
        .deserialize::<OneHopInput>(&input.request.body)?;
    let mut remapping_vals = RemappingMap::new();
    let txn = db
        .graph_env
        .read_txn()
        .map_err(|e| GraphError::New(format!("Failed to start read transaction: {:?}", e)))?;

    let items = G::new_with_arena(&arena, &db, &txn)
        .n_from_id(&data.user_id)
        .out_vec("Interacted")
        .map_traversal(|item: TraversalValueArena, txn| {
            println!("got to map traversal");
            field_remapping!(remapping_vals, item, false, "id" => "id")?;
            field_remapping!(remapping_vals, item, false, "category" => "category")?;
            println!("completed remapping");
            Ok(item)
        })
        .collect_to::<Vec<_>>();
    let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
    println!("completed traversal");
    return_vals.insert(
        "items".to_string(),
        ReturnValue::from_traversal_value_array_arena_with_mixin(
            items,
            remapping_vals.borrow_mut(),
        ),
    );

    println!("completed return values");

    txn.commit()
        .map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
    Ok(input.request.out_fmt.create_response(&return_vals))
}


#[handler]
pub fn OneHopNoInput(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let arena = Bump::new();
    info!("got to arena");
    let mut remapping_vals = RemappingMap::new();
    info!("got to remapping vals");
    let txn = db
        .graph_env
        .read_txn()
        .map_err(|e| GraphError::New(format!("Failed to start read transaction: {:?}", e)))?;

    info!("got to new with arena");
    let items = G::new_with_arena(&arena, &db, &txn)
        .n_from_type("User")
        .range(0, 1)
        .collect_to::<Vec<_>>();
    let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
    println!("completed traversal");

    println!("got to return value");
    println!("items: {:?}", items);
    info!("items: {:?}", items);
    let items = ReturnValue::from_traversal_value_array_arena_with_mixin(
        items,
        remapping_vals.borrow_mut(),
    );
    println!("return values: {:?}", items);
    info!("return values: {:?}", items);
    return_vals.insert(
        "items".to_string(),
        items,
    );

    println!("completed return values");

    txn.commit()
        .map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
    Ok(input.request.out_fmt.create_response(&return_vals))
}


#[handler]
pub fn ConvertAllVectors(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let arena = Bump::new();

    let mut txn = db
        .graph_env
        .write_txn()
        .map_err(|e| GraphError::New(format!("Failed to start read transaction: {:?}", e)))?;

    let mut vectors = Vec::with_capacity(5000);

    let vectors_iter = db.vectors.vectors_db.iter(&txn)?;
    for vector in vectors_iter {
        let (key, value) = vector?;
        if key == b"entry_point" {
            continue;
        }

        let id = u128::from_be_bytes(key[1..17].try_into().unwrap());
        let level = usize::from_be_bytes(key[17..25].try_into().unwrap());

        let vector = HVector::from_bytes(id, level, &value)?;
        vectors.push(vector);
    }

    for vector in vectors {
        db.vectors.vectors_db.put(
            &mut txn,
            &VectorCore::vector_key(vector.get_id(), vector.get_level()),
            &vector.to_le_bytes(),
        )?;
    }
    txn.commit()?;

    Ok(input
        .request
        .out_fmt
        .create_response(&ReturnValue::from("Success")))
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
    let data = input
        .request
        .in_fmt
        .deserialize::<CreateDatasetIdInput>(&input.request.body)?;
    let mut remapping_vals = RemappingMap::new();
    let mut txn = db
        .graph_env
        .write_txn()
        .map_err(|e| GraphError::New(format!("Failed to start write transaction: {:?}", e)))?;
    let metadata = G::new_mut(Arc::clone(&db), &mut txn)
        .add_n(
            "Metadata",
            Some(props! { "value" => &data.dataset_id, "key" => "dataset_id" }),
            Some(&["key"]),
        )
        .collect_to_obj();
    let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
    return_vals.insert(
        "metadata".to_string(),
        ReturnValue::from_traversal_value_with_mixin(metadata.clone(), remapping_vals.borrow_mut()),
    );

    txn.commit()
        .map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
    Ok(input.request.out_fmt.create_response(&return_vals))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct UpdateDatasetIdInput {
    pub dataset_id: String,
}
#[handler]
pub fn UpdateDatasetId(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<UpdateDatasetIdInput>(&input.request.body)?;
    let mut remapping_vals = RemappingMap::new();
    let mut txn = db
        .graph_env
        .write_txn()
        .map_err(|e| GraphError::New(format!("Failed to start write transaction: {:?}", e)))?;
    Drop::<Vec<_>>::drop_traversal(
        G::new(Arc::clone(&db), &txn)
            .n_from_index("Metadata", "key", &"dataset_id")
            .collect_to_obj(),
        Arc::clone(&db),
        &mut txn,
    )?;
    let metadata = G::new_mut(Arc::clone(&db), &mut txn)
        .add_n(
            "Metadata",
            Some(props! { "value" => &data.dataset_id, "key" => "dataset_id" }),
            Some(&["key"]),
        )
        .collect_to_obj();
    let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
    return_vals.insert(
        "metadata".to_string(),
        ReturnValue::from_traversal_value_with_mixin(metadata.clone(), remapping_vals.borrow_mut()),
    );

    txn.commit()
        .map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
    Ok(input.request.out_fmt.create_response(&return_vals))
}
#[handler]
pub fn GetDatasetId(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let mut remapping_vals = RemappingMap::new();
    let txn = db
        .graph_env
        .read_txn()
        .map_err(|e| GraphError::New(format!("Failed to start read transaction: {:?}", e)))?;
    let dataset_id = G::new(Arc::clone(&db), &txn)
        .n_from_index("Metadata", "key", &"dataset_id")
        .collect_to_obj();
    let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
    return_vals.insert(
        "dataset_id".to_string(),
        ReturnValue::from_traversal_value_with_mixin(
            G::new_from(Arc::clone(&db), &txn, dataset_id.clone())
                .check_property("value")
                .collect_to_obj(),
            remapping_vals.borrow_mut(),
        ),
    );

    txn.commit()
        .map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
    Ok(input.request.out_fmt.create_response(&return_vals))
}
