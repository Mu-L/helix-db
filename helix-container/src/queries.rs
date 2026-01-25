
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
                    filter_ref::FilterRefAdapter, map::MapAdapter, paths::{PathAlgorithm, ShortestPathAdapter},
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
secondary_indices: None,
}),
db_max_size_gb: Some(10),
mcp: Some(true),
bm25: Some(true),
schema: Some(r#"{
  "schema": {
    "nodes": [
      {
        "name": "App",
        "properties": {
          "archived": "Boolean",
          "name": "String",
          "label": "String",
          "created_at": "Date",
          "id": "ID",
          "description": "String",
          "favorite": "Boolean"
        }
      },
      {
        "name": "User",
        "properties": {
          "name": "String",
          "label": "String",
          "id": "ID",
          "age": "I32"
        }
      }
    ],
    "vectors": [],
    "edges": [
      {
        "name": "User_Has_Access_To",
        "from": "User",
        "to": "App",
        "properties": {
          "created_at": "Date",
          "modified_at": "Date"
        }
      }
    ]
  },
  "queries": [
    {
      "name": "GetUsers2",
      "parameters": {
        "user_id": "ID"
      },
      "returns": []
    },
    {
      "name": "GetUsers",
      "parameters": {},
      "returns": [
        "users"
      ]
    },
    {
      "name": "GetAppsByUserId",
      "parameters": {
        "user_id": "ID"
      },
      "returns": []
    }
  ]
}"#.to_string()),
embedding_model: Some("text-embedding-ada-002".to_string()),
graphvis_node_label: None,
})
}
pub struct User {
    pub name: String,
    pub age: i32,
}

pub struct App {
    pub name: String,
    pub description: String,
    pub created_at: DateTime<Utc>,
    pub favorite: bool,
    pub archived: bool,
}

pub struct User_Has_Access_To {
    pub from: User,
    pub to: App,
    pub modified_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}


#[derive(Serialize, Deserialize, Clone)]
pub struct GetUsers2Input {

pub user_id: ID
}
#[derive(Serialize, Default)]
pub struct GetUsers2AppsAccess_modified_atReturnType {
}


#[derive(Serialize, Default)]
pub struct GetUsers2AppsReturnType {
    pub access_modified_at: GetUsers2AppsAccess_modified_atReturnType,
}

#[handler]
pub fn GetUsers2 (input: HandlerInput) -> Result<Response, GraphError> {
let db = Arc::clone(&input.graph.storage);
let data = input.request.in_fmt.deserialize::<GetUsers2Input>(&input.request.body)?;
let arena = Bump::new();
let txn = db.graph_env.read_txn().map_err(|e| GraphError::New(format!("Failed to start read transaction: {:?}", e)))?;
    let user = G::new(&db, &txn, &arena)
.n_from_id(&data.user_id).collect_to_obj()?;
    let apps = G::from_iter(&db, &txn, std::iter::once(user.clone()), &arena)

.out_node("User_Has_Access_To").collect::<Result<Vec<_>, _>>()?;
let response = json!({
    "apps": apps.iter().map(|app| Ok::<_, GraphError>(GetUsers2AppsReturnType {
        access_modified_at: G::from_iter(&db, &txn, std::iter::once(app.clone()), &arena)

.in_e("User_Has_Access_To").map(|item| item.map(|item| GetUsers2AppsAccess_modified_atReturnType {
                    })).next().unwrap_or(Ok(Default::default()))?,
    })).collect::<Result<Vec<_>, GraphError>>()?
});
txn.commit().map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
Ok(input.request.out_fmt.create_response(&response))
}

#[derive(Serialize, Default)]
pub struct GetUsersUsersReturnType<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub name: Option<&'a Value>,
    pub age: Option<&'a Value>,
}

#[handler]
pub fn GetUsers (input: HandlerInput) -> Result<Response, GraphError> {
let db = Arc::clone(&input.graph.storage);
let arena = Bump::new();
let txn = db.graph_env.read_txn().map_err(|e| GraphError::New(format!("Failed to start read transaction: {:?}", e)))?;
    let users = G::new(&db, &txn, &arena)
.n_from_type("User").collect_to_obj()?;
let response = json!({
    "users": GetUsersUsersReturnType {
        id: uuid_str(users.id(), &arena),
        label: users.label(),
        name: users.get_property("name"),
        age: users.get_property("age"),
    }
});
txn.commit().map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
Ok(input.request.out_fmt.create_response(&response))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct GetAppsByUserIdInput {

pub user_id: ID
}
#[derive(Serialize, Default)]
pub struct GetAppsByUserIdAppsAccess_modified_atReturnType<'a> {
    pub modified_at: Option<&'a Value>,
}


#[derive(Serialize, Default)]
pub struct GetAppsByUserIdAppsAccess_modified_at_vecReturnType<'a> {
    pub modified_at: Option<&'a Value>,
    pub created_at: Option<&'a Value>,
}


#[derive(Serialize, Default)]
pub struct GetAppsByUserIdAppsReturnType<'a> {
    pub id: &'a str,
    pub name: Option<&'a Value>,
    pub description: Option<&'a Value>,
    pub created_at: Option<&'a Value>,
    pub favorite: Option<&'a Value>,
    pub archived: Option<&'a Value>,
    pub access_modified_at2: Option<&'a Value>,
    pub access_modified_at: GetAppsByUserIdAppsAccess_modified_atReturnType<'a>,
    pub access_modified_at_vec: Vec<GetAppsByUserIdAppsAccess_modified_at_vecReturnType<'a>>,
}

#[handler]
pub fn GetAppsByUserId (input: HandlerInput) -> Result<Response, GraphError> {
let db = Arc::clone(&input.graph.storage);
let data = input.request.in_fmt.deserialize::<GetAppsByUserIdInput>(&input.request.body)?;
let arena = Bump::new();
let txn = db.graph_env.read_txn().map_err(|e| GraphError::New(format!("Failed to start read transaction: {:?}", e)))?;
    let user = G::new(&db, &txn, &arena)
.n_from_id(&data.user_id).collect_to_obj()?;
    let apps = G::from_iter(&db, &txn, std::iter::once(user.clone()), &arena)

.out_node("User_Has_Access_To").collect::<Result<Vec<_>, _>>()?;
let response = json!({
    "apps": apps.iter().map(|app| Ok::<_, GraphError>(GetAppsByUserIdAppsReturnType {
        id: uuid_str(app.id(), &arena),
        name: app.get_property("name"),
        description: app.get_property("description"),
        created_at: app.get_property("created_at"),
        favorite: app.get_property("favorite"),
        archived: app.get_property("archived"),
        access_modified_at2: app.get_property("description"),
        access_modified_at: G::from_iter(&db, &txn, std::iter::once(app.clone()), &arena)

.in_e("User_Has_Access_To").map(|item| item.map(|item| GetAppsByUserIdAppsAccess_modified_atReturnType {
                        modified_at: item.get_property("modified_at"),
                    })).next().unwrap_or(Ok(Default::default()))?,
        access_modified_at_vec: G::from_iter(&db, &txn, std::iter::once(app.clone()), &arena)

.in_e("User_Has_Access_To").map(|item| item.map(|item| GetAppsByUserIdAppsAccess_modified_at_vecReturnType {
                        modified_at: item.get_property("modified_at"),
                        created_at: item.get_property("created_at"),
                    })).collect::<Result<Vec<_>, _>>()?,
    })).collect::<Result<Vec<_>, GraphError>>()?
});
txn.commit().map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
Ok(input.request.out_fmt.create_response(&response))
}


