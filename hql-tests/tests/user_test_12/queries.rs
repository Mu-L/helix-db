
// DEFAULT CODE
// use helix_db::helix_engine::traversal_core::config::Config;

// pub fn config() -> Option<Config> {
//     None
// }



use bumpalo::Bump;
use heed3::RoTxn;
use helix_macros::{handler, tool_call, mcp_handler, migration};
use helix_db::{
    helix_engine::{
        reranker::{
            RerankAdapter,
            fusion::{RRFReranker, MMRReranker, DistanceMethod},
        },
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
                    add_e::AddEAdapter,
                    add_n::AddNAdapter,
                    e_from_id::EFromIdAdapter,
                    e_from_type::EFromTypeAdapter,
                    n_from_id::NFromIdAdapter,
                    n_from_index::NFromIndexAdapter,
                    n_from_type::NFromTypeAdapter,
                    v_from_id::VFromIdAdapter,
                    v_from_type::VFromTypeAdapter
                },
                util::{
                    dedup::DedupAdapter, drop::Drop, exist::Exist, filter_mut::FilterMut,
                    filter_ref::FilterRefAdapter, intersect::IntersectAdapter, map::MapAdapter, paths::{PathAlgorithm, ShortestPathAdapter},
                    range::RangeAdapter, update::UpdateAdapter, order::OrderByAdapter,
                    aggregate::AggregateAdapter, group_by::GroupByAdapter, count::CountAdapter,
                    upsert::UpsertAdapter,
                },
                vectors::{
                    brute_force_search::BruteForceSearchVAdapter, insert::InsertVAdapter,
                    search::SearchVAdapter,
                },
            },
            traversal_value::TraversalValue,
        },
        types::{GraphError, SecondaryIndex},
        vector_core::vector::HVector,
    },
    helix_gateway::{
        embedding_providers::{EmbeddingModel, get_embedding_model},
        router::router::{HandlerInput, IoContFn},
        mcp::mcp::{MCPHandlerSubmission, MCPToolInput, MCPHandler}
    },
    node_matches, props, embed, embed_async,
    field_addition_from_old_field, field_type_cast, field_addition_from_value,
    protocol::{
        response::Response,
        value::{casting::{cast, CastType}, Value},
        date::Date,
        format::Format,
    },
    utils::{
        id::{ID, uuid_str},
        items::{Edge, Node},
        properties::ImmutablePropertiesMap,
    },
};
use sonic_rs::{Deserialize, Serialize, json};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;
use chrono::{DateTime, Utc};

// Re-export scalar types for generated code
type I8 = i8;
type I16 = i16;
type I32 = i32;
type I64 = i64;
type U8 = u8;
type U16 = u16;
type U32 = u32;
type U64 = u64;
type U128 = u128;
type F32 = f32;
type F64 = f64;
    
pub fn config() -> Option<Config> {
return Some(Config {
vector_config: Some(VectorConfig {
m: Some(16),
ef_construction: Some(128),
ef_search: Some(768),
}),
graph_config: Some(GraphConfig {
secondary_indices: Some(vec![SecondaryIndex::Unique("name".to_string()), SecondaryIndex::Index("source".to_string()), SecondaryIndex::Index("family".to_string())]),
}),
db_max_size_gb: Some(10),
mcp: Some(true),
bm25: Some(true),
schema: Some(r#"{
  "schema": {
    "nodes": [
      {
        "name": "TimeParameter",
        "properties": {
          "name": "String",
          "value": "String",
          "label": "String",
          "classification": "String",
          "id": "ID"
        }
      },
      {
        "name": "Indicator",
        "properties": {
          "id": "ID",
          "run_name": "String",
          "measure_type": "String",
          "source": "String",
          "tz": "String",
          "currency_code": "String",
          "tenor": "String",
          "run_start": "Date",
          "indicator_class": "String",
          "label": "String",
          "forward_tenor": "String",
          "config_origin": "String",
          "index_name": "String",
          "name": "String",
          "description": "String",
          "username": "String",
          "family": "String",
          "asset_class": "String",
          "run_end": "Date",
          "tags": "String"
        }
      }
    ],
    "vectors": [],
    "edges": [
      {
        "name": "HasTimeParameter",
        "from": "Indicator",
        "to": "TimeParameter",
        "properties": {}
      }
    ]
  },
  "queries": [
    {
      "name": "GetIndicatorsWithTimeParams",
      "parameters": {
        "time_vals": "Array(String)"
      },
      "returns": [
        "indicators"
      ]
    },
    {
      "name": "LinkIndicatorToTimeParameter",
      "parameters": {
        "time_parameter_id": "ID",
        "indicator_id": "ID"
      },
      "returns": [
        "link"
      ]
    },
    {
      "name": "CreateIndicator",
      "parameters": {
        "tz": "String",
        "run_name": "String",
        "username": "String",
        "run_start": "Date",
        "index_name": "String",
        "indicator_class": "String",
        "currency_code": "String",
        "measure_type": "String",
        "forward_tenor": "String",
        "family": "String",
        "description": "String",
        "name": "String",
        "source": "String",
        "tags": "String",
        "asset_class": "String",
        "tenor": "String",
        "run_end": "Date",
        "config_origin": "String"
      },
      "returns": [
        "indicator"
      ]
    },
    {
      "name": "CreateTimeParameter",
      "parameters": {
        "name": "String",
        "value": "String",
        "classification": "String"
      },
      "returns": [
        "time_parameter"
      ]
    }
  ]
}"#.to_string()),
embedding_model: Some("text-embedding-ada-002".to_string()),
graphvis_node_label: None,
})
}
pub struct TimeParameter {
    pub name: String,
    pub value: String,
    pub classification: String,
}

pub struct Indicator {
    pub name: String,
    pub description: String,
    pub source: String,
    pub username: String,
    pub run_name: String,
    pub run_start: DateTime<Utc>,
    pub run_end: DateTime<Utc>,
    pub tz: String,
    pub indicator_class: String,
    pub config_origin: String,
    pub tags: String,
    pub asset_class: String,
    pub family: String,
    pub measure_type: String,
    pub currency_code: String,
    pub index_name: String,
    pub forward_tenor: String,
    pub tenor: String,
}

pub struct HasTimeParameter {
    pub from: Indicator,
    pub to: TimeParameter,
}


#[derive(Serialize, Deserialize, Clone)]
pub struct GetIndicatorsWithTimeParamsInput {

pub time_vals: Vec<String>
}
#[derive(Serialize, Default)]
pub struct GetIndicatorsWithTimeParamsIndicatorsReturnType<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub name: Option<&'a Value>,
    pub description: Option<&'a Value>,
    pub source: Option<&'a Value>,
    pub username: Option<&'a Value>,
    pub run_name: Option<&'a Value>,
    pub run_start: Option<&'a Value>,
    pub run_end: Option<&'a Value>,
    pub tz: Option<&'a Value>,
    pub indicator_class: Option<&'a Value>,
    pub config_origin: Option<&'a Value>,
    pub tags: Option<&'a Value>,
    pub asset_class: Option<&'a Value>,
    pub family: Option<&'a Value>,
    pub measure_type: Option<&'a Value>,
    pub currency_code: Option<&'a Value>,
    pub index_name: Option<&'a Value>,
    pub forward_tenor: Option<&'a Value>,
    pub tenor: Option<&'a Value>,
}

#[handler]
pub fn GetIndicatorsWithTimeParams (input: HandlerInput) -> Result<Response, GraphError> {
let db = Arc::clone(&input.graph.storage);
let data = input.request.in_fmt.deserialize::<GetIndicatorsWithTimeParamsInput>(&input.request.body)?;
let arena = Bump::new();
let txn = db.graph_env.read_txn().map_err(|e| GraphError::New(format!("Failed to start read transaction: {:?}", e)))?;
    let indicators = G::new(&db, &txn, &arena)
.n_from_type("TimeParameter")

.filter_ref(|val, txn|{
                if let Ok(val) = val {
                    Ok(val
                    .get_property("value")
                    .map_or(false, |v| v.is_in(&data.time_vals)))
                } else {
                    Ok(false)
                }
            })

.intersect(|val, db, txn, arena| {G::from_iter(&db, &txn, std::iter::once(val), &arena)

.in_node("HasTimeParameter").filter_map(|r| r.ok()).collect::<Vec<_>>()}).collect::<Result<Vec<_>, _>>()?;
let response = json!({
    "indicators": indicators.iter().map(|indicator| GetIndicatorsWithTimeParamsIndicatorsReturnType {
        id: uuid_str(indicator.id(), &arena),
        label: indicator.label(),
        name: indicator.get_property("name"),
        description: indicator.get_property("description"),
        source: indicator.get_property("source"),
        username: indicator.get_property("username"),
        run_name: indicator.get_property("run_name"),
        run_start: indicator.get_property("run_start"),
        run_end: indicator.get_property("run_end"),
        tz: indicator.get_property("tz"),
        indicator_class: indicator.get_property("indicator_class"),
        config_origin: indicator.get_property("config_origin"),
        tags: indicator.get_property("tags"),
        asset_class: indicator.get_property("asset_class"),
        family: indicator.get_property("family"),
        measure_type: indicator.get_property("measure_type"),
        currency_code: indicator.get_property("currency_code"),
        index_name: indicator.get_property("index_name"),
        forward_tenor: indicator.get_property("forward_tenor"),
        tenor: indicator.get_property("tenor"),
    }).collect::<Vec<_>>()
});
txn.commit().map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
Ok(input.request.out_fmt.create_response(&response))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct LinkIndicatorToTimeParameterInput {

pub indicator_id: ID,
pub time_parameter_id: ID
}
#[derive(Serialize, Default)]
pub struct LinkIndicatorToTimeParameterLinkReturnType<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub from_node: &'a str,
    pub to_node: &'a str,
}

#[handler(is_write)]
pub fn LinkIndicatorToTimeParameter (input: HandlerInput) -> Result<Response, GraphError> {
let db = Arc::clone(&input.graph.storage);
let data = input.request.in_fmt.deserialize::<LinkIndicatorToTimeParameterInput>(&input.request.body)?;
let arena = Bump::new();
let mut txn = db.graph_env.write_txn().map_err(|e| GraphError::New(format!("Failed to start write transaction: {:?}", e)))?;
    let link = G::new_mut(&db, &arena, &mut txn)
.add_edge("HasTimeParameter", None, *data.indicator_id, *data.time_parameter_id, false, false).collect_to_obj()?;
let response = json!({
    "link": LinkIndicatorToTimeParameterLinkReturnType {
        id: uuid_str(link.id(), &arena),
        label: link.label(),
        from_node: uuid_str(link.from_node(), &arena),
        to_node: uuid_str(link.to_node(), &arena),
    }
});
txn.commit().map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
Ok(input.request.out_fmt.create_response(&response))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct CreateIndicatorInput {

pub name: String,
pub description: String,
pub source: String,
pub username: String,
pub run_name: String,
pub run_start: DateTime<Utc>,
pub run_end: DateTime<Utc>,
pub tz: String,
pub indicator_class: String,
pub config_origin: String,
pub tags: String,
pub asset_class: String,
pub family: String,
pub measure_type: String,
pub currency_code: String,
pub index_name: String,
pub forward_tenor: String,
pub tenor: String
}
#[derive(Serialize, Default)]
pub struct CreateIndicatorIndicatorReturnType<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub name: Option<&'a Value>,
    pub description: Option<&'a Value>,
    pub source: Option<&'a Value>,
    pub username: Option<&'a Value>,
    pub run_name: Option<&'a Value>,
    pub run_start: Option<&'a Value>,
    pub run_end: Option<&'a Value>,
    pub tz: Option<&'a Value>,
    pub indicator_class: Option<&'a Value>,
    pub config_origin: Option<&'a Value>,
    pub tags: Option<&'a Value>,
    pub asset_class: Option<&'a Value>,
    pub family: Option<&'a Value>,
    pub measure_type: Option<&'a Value>,
    pub currency_code: Option<&'a Value>,
    pub index_name: Option<&'a Value>,
    pub forward_tenor: Option<&'a Value>,
    pub tenor: Option<&'a Value>,
}

#[handler(is_write)]
pub fn CreateIndicator (input: HandlerInput) -> Result<Response, GraphError> {
let db = Arc::clone(&input.graph.storage);
let data = input.request.in_fmt.deserialize::<CreateIndicatorInput>(&input.request.body)?;
let arena = Bump::new();
let mut txn = db.graph_env.write_txn().map_err(|e| GraphError::New(format!("Failed to start write transaction: {:?}", e)))?;
    let indicator = G::new_mut(&db, &arena, &mut txn)
.add_n("Indicator", Some(ImmutablePropertiesMap::new(18, vec![("forward_tenor", Value::from(&data.forward_tenor)), ("family", Value::from(&data.family)), ("run_end", Value::from(&data.run_end)), ("tz", Value::from(&data.tz)), ("measure_type", Value::from(&data.measure_type)), ("run_name", Value::from(&data.run_name)), ("tenor", Value::from(&data.tenor)), ("tags", Value::from(&data.tags)), ("currency_code", Value::from(&data.currency_code)), ("index_name", Value::from(&data.index_name)), ("config_origin", Value::from(&data.config_origin)), ("asset_class", Value::from(&data.asset_class)), ("username", Value::from(&data.username)), ("run_start", Value::from(&data.run_start)), ("source", Value::from(&data.source)), ("name", Value::from(&data.name)), ("description", Value::from(&data.description)), ("indicator_class", Value::from(&data.indicator_class))].into_iter(), &arena)), Some(&["name", "source", "family"])).collect_to_obj()?;
let response = json!({
    "indicator": CreateIndicatorIndicatorReturnType {
        id: uuid_str(indicator.id(), &arena),
        label: indicator.label(),
        name: indicator.get_property("name"),
        description: indicator.get_property("description"),
        source: indicator.get_property("source"),
        username: indicator.get_property("username"),
        run_name: indicator.get_property("run_name"),
        run_start: indicator.get_property("run_start"),
        run_end: indicator.get_property("run_end"),
        tz: indicator.get_property("tz"),
        indicator_class: indicator.get_property("indicator_class"),
        config_origin: indicator.get_property("config_origin"),
        tags: indicator.get_property("tags"),
        asset_class: indicator.get_property("asset_class"),
        family: indicator.get_property("family"),
        measure_type: indicator.get_property("measure_type"),
        currency_code: indicator.get_property("currency_code"),
        index_name: indicator.get_property("index_name"),
        forward_tenor: indicator.get_property("forward_tenor"),
        tenor: indicator.get_property("tenor"),
    }
});
txn.commit().map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
Ok(input.request.out_fmt.create_response(&response))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct CreateTimeParameterInput {

pub name: String,
pub value: String,
pub classification: String
}
#[derive(Serialize, Default)]
pub struct CreateTimeParameterTime_parameterReturnType<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub name: Option<&'a Value>,
    pub value: Option<&'a Value>,
    pub classification: Option<&'a Value>,
}

#[handler(is_write)]
pub fn CreateTimeParameter (input: HandlerInput) -> Result<Response, GraphError> {
let db = Arc::clone(&input.graph.storage);
let data = input.request.in_fmt.deserialize::<CreateTimeParameterInput>(&input.request.body)?;
let arena = Bump::new();
let mut txn = db.graph_env.write_txn().map_err(|e| GraphError::New(format!("Failed to start write transaction: {:?}", e)))?;
    let time_parameter = G::new_mut(&db, &arena, &mut txn)
.add_n("TimeParameter", Some(ImmutablePropertiesMap::new(3, vec![("name", Value::from(&data.name)), ("classification", Value::from(&data.classification)), ("value", Value::from(&data.value))].into_iter(), &arena)), Some(&["name"])).collect_to_obj()?;
let response = json!({
    "time_parameter": CreateTimeParameterTime_parameterReturnType {
        id: uuid_str(time_parameter.id(), &arena),
        label: time_parameter.label(),
        name: time_parameter.get_property("name"),
        value: time_parameter.get_property("value"),
        classification: time_parameter.get_property("classification"),
    }
});
txn.commit().map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
Ok(input.request.out_fmt.create_response(&response))
}


