
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
                },
                vectors::{
                    brute_force_search::BruteForceSearchVAdapter, insert::InsertVAdapter,
                    search::SearchVAdapter,
                },
            },
            traversal_value::TraversalValue,
        },
        types::GraphError,
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
        count::Count,
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
secondary_indices: Some(vec!["gh_id".to_string()]),
}),
db_max_size_gb: Some(10),
mcp: Some(true),
bm25: Some(true),
schema: Some(r#"{
  "schema": {
    "nodes": [
      {
        "name": "User",
        "properties": {
          "created_at": "Date",
          "updated_at": "Date",
          "gh_id": "U64",
          "id": "ID",
          "label": "String",
          "name": "String",
          "gh_login": "String",
          "email": "String"
        }
      },
      {
        "name": "Cluster",
        "properties": {
          "api_url": "String",
          "status": "String",
          "label": "String",
          "region": "String",
          "id": "ID",
          "updated_at": "Date",
          "created_at": "Date"
        }
      },
      {
        "name": "Instance",
        "properties": {
          "api_url": "String",
          "updated_at": "Date",
          "storage_gb": "I64",
          "created_at": "Date",
          "ram_gb": "I64",
          "label": "String",
          "region": "String",
          "status": "String",
          "id": "ID",
          "instance_type": "String"
        }
      }
    ],
    "vectors": [],
    "edges": [
      {
        "name": "CreatedCluster",
        "from": "User",
        "to": "Cluster",
        "properties": {}
      },
      {
        "name": "CreatedInstance",
        "from": "Cluster",
        "to": "Instance",
        "properties": {}
      }
    ]
  },
  "queries": [
    {
      "name": "ListAllUsersWithClusters",
      "parameters": {},
      "returns": []
    },
    {
      "name": "CreateUser",
      "parameters": {
        "name": "String",
        "gh_id": "U64",
        "email": "String",
        "gh_login": "String"
      },
      "returns": [
        "user"
      ]
    },
    {
      "name": "CreateCluster",
      "parameters": {
        "instance_type": "String",
        "storage_gb": "I64",
        "region": "String",
        "ram_gb": "I64",
        "user_id": "ID"
      },
      "returns": [
        "new_cluster"
      ]
    },
    {
      "name": "UpdateClusterApiUrl",
      "parameters": {
        "api_url": "String",
        "cluster_id": "ID"
      },
      "returns": [
        "clusters"
      ]
    },
    {
      "name": "UpdateClusterStatus",
      "parameters": {
        "cluster_id": "ID",
        "status": "String"
      },
      "returns": [
        "clusters"
      ]
    },
    {
      "name": "LookupUser",
      "parameters": {
        "gh_id": "U64"
      },
      "returns": [
        "user"
      ]
    },
    {
      "name": "GetClusterURL",
      "parameters": {
        "cluster_id": "ID"
      },
      "returns": []
    },
    {
      "name": "VerifyUserAccessToCluster",
      "parameters": {
        "user_id": "ID",
        "cluster_id": "ID"
      },
      "returns": [
        "can_access"
      ]
    },
    {
      "name": "GetInstancesForUser",
      "parameters": {
        "user_id": "ID"
      },
      "returns": [
        "instances"
      ]
    }
  ]
}"#.to_string()),
embedding_model: Some("text-embedding-ada-002".to_string()),
graphvis_node_label: None,
})
}

pub struct Cluster {
    pub region: String,
    pub api_url: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct Instance {
    pub region: String,
    pub instance_type: String,
    pub storage_gb: i64,
    pub ram_gb: i64,
    pub status: String,
    pub api_url: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct User {
    pub gh_id: u64,
    pub gh_login: String,
    pub name: String,
    pub email: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct CreatedCluster {
    pub from: User,
    pub to: Cluster,
}

pub struct CreatedInstance {
    pub from: Cluster,
    pub to: Instance,
}


#[derive(Serialize, Traversable)]
pub struct UsersReturnType<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub updated_at: Option<&'a Value>,
    pub email: Option<&'a Value>,
    pub gh_login: Option<&'a Value>,
    pub gh_id: Option<&'a Value>,
    pub name: Option<&'a Value>,
    pub created_at: Option<&'a Value>,
}

#[handler]
pub fn ListAllUsersWithClusters (input: HandlerInput) -> Result<Response, GraphError> {
let db = Arc::clone(&input.graph.storage);
let arena = Bump::new();
let txn = db.graph_env.read_txn().map_err(|e| GraphError::New(format!("Failed to start read transaction: {:?}", e)))?;
    let users = G::new(&db, &txn, &arena)
.n_from_type("User")
    .map(|val| UsersReturnType {
            id: uuid_str(user.id(), &arena),
            label: user.label(),
            updated_at: user.get_property("updated_at").unwrap(),
            email: user.get_property("email").unwrap(),
            gh_login: user.get_property("gh_login").unwrap(),
            gh_id: user.get_property("gh_id").unwrap(),
            name: user.get_property("name").unwrap(),
            created_at: user.get_property("created_at").unwrap(),
        }).collect_to::<Vec<_>>();
txn.commit().map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
let response = users;
Ok(input.request.out_fmt.create_response(&response))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct CreateUserInput {

pub gh_id: u64,
pub gh_login: String,
pub name: String,
pub email: String
}
#[derive(Serialize, Traversable)]
pub struct UserReturnType<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub updated_at: Option<&'a Value>,
    pub email: Option<&'a Value>,
    pub gh_login: Option<&'a Value>,
    pub gh_id: Option<&'a Value>,
    pub name: Option<&'a Value>,
    pub created_at: Option<&'a Value>,
}

#[handler]
pub fn CreateUser (input: HandlerInput) -> Result<Response, GraphError> {
let db = Arc::clone(&input.graph.storage);
let data = input.request.in_fmt.deserialize::<CreateUserInput>(&input.request.body)?;
let arena = Bump::new();
let mut txn = db.graph_env.write_txn().map_err(|e| GraphError::New(format!("Failed to start write transaction: {:?}", e)))?;
    let user = G::new_mut(&db, &arena, &mut txn)
.add_n("User", Some(ImmutablePropertiesMap::new(6, vec![("email", Value::from(&data.email)), ("gh_login", Value::from(&data.gh_login)), ("name", Value::from(&data.name)), ("gh_id", Value::from(&data.gh_id)), ("created_at", Value::from(chrono::Utc::now().to_rfc3339())), ("updated_at", Value::from(chrono::Utc::now().to_rfc3339()))].into_iter(), &arena)), Some(&["gh_id"])).collect_to_obj();
txn.commit().map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
let response = user;
Ok(input.request.out_fmt.create_response(&response))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct CreateClusterInput {

pub user_id: ID,
pub region: String,
pub instance_type: String,
pub storage_gb: i64,
pub ram_gb: i64
}
#[derive(Serialize, Traversable)]
pub struct New_clusterReturnType<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub region: Option<&'a Value>,
    pub updated_at: Option<&'a Value>,
    pub status: Option<&'a Value>,
    pub created_at: Option<&'a Value>,
    pub api_url: Option<&'a Value>,
}

#[handler]
pub fn CreateCluster (input: HandlerInput) -> Result<Response, GraphError> {
let db = Arc::clone(&input.graph.storage);
let data = input.request.in_fmt.deserialize::<CreateClusterInput>(&input.request.body)?;
let arena = Bump::new();
let mut txn = db.graph_env.write_txn().map_err(|e| GraphError::New(format!("Failed to start write transaction: {:?}", e)))?;
    let user = G::new(&db, &txn, &arena)
.n_from_id(&data.user_id).collect_to_obj();
    let new_cluster = G::new_mut(&db, &arena, &mut txn)
.add_n("Cluster", Some(ImmutablePropertiesMap::new(5, vec![("updated_at", Value::from(chrono::Utc::now().to_rfc3339())), ("status", Value::from("pending")), ("api_url", Value::from("")), ("created_at", Value::from(chrono::Utc::now().to_rfc3339())), ("region", Value::from(&data.region))].into_iter(), &arena)), None).collect_to_obj();
    let new_instance = G::new_mut(&db, &arena, &mut txn)
.add_n("Instance", Some(ImmutablePropertiesMap::new(8, vec![("status", Value::from("pending")), ("storage_gb", Value::from(&data.storage_gb)), ("ram_gb", Value::from(&data.ram_gb)), ("region", Value::from(&data.region)), ("api_url", Value::from("")), ("instance_type", Value::from(&data.instance_type)), ("created_at", Value::from(chrono::Utc::now().to_rfc3339())), ("updated_at", Value::from(chrono::Utc::now().to_rfc3339()))].into_iter(), &arena)), None).collect_to_obj();
    G::new_mut(&db, &arena, &mut txn)
.add_edge("CreatedCluster", None, user.id(), new_cluster.id(), false).collect_to_obj();
    G::new_mut(&db, &arena, &mut txn)
.add_edge("CreatedInstance", None, new_cluster.id(), new_instance.id(), false).collect_to_obj();
txn.commit().map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
let response = new_cluster;
Ok(input.request.out_fmt.create_response(&response))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct UpdateClusterApiUrlInput {

pub cluster_id: ID,
pub api_url: String
}
#[derive(Serialize, Traversable)]
pub struct ClustersReturnType<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub region: Option<&'a Value>,
    pub updated_at: Option<&'a Value>,
    pub status: Option<&'a Value>,
    pub created_at: Option<&'a Value>,
    pub api_url: Option<&'a Value>,
}

#[handler]
pub fn UpdateClusterApiUrl (input: HandlerInput) -> Result<Response, GraphError> {
let db = Arc::clone(&input.graph.storage);
let data = input.request.in_fmt.deserialize::<UpdateClusterApiUrlInput>(&input.request.body)?;
let arena = Bump::new();
let mut txn = db.graph_env.write_txn().map_err(|e| GraphError::New(format!("Failed to start write transaction: {:?}", e)))?;
    let clusters = {let update_tr = G::new(&db, &txn, &arena)
.n_from_id(&data.cluster_id)
    .collect_to::<Vec<_>>();G::new_mut_from_iter(&db, &mut txn, update_tr.iter().cloned(), &arena)
    .update(&[("api_url", Value::from(&data.api_url))])
    .collect_to_obj()}
    .map(|val| ClustersReturnType {
            id: uuid_str(cluster.id(), &arena),
            label: cluster.label(),
            region: cluster.get_property("region").unwrap(),
            updated_at: cluster.get_property("updated_at").unwrap(),
            status: cluster.get_property("status").unwrap(),
            created_at: cluster.get_property("created_at").unwrap(),
            api_url: cluster.get_property("api_url").unwrap(),
        });
txn.commit().map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
let response = clusters;
Ok(input.request.out_fmt.create_response(&response))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct UpdateClusterStatusInput {

pub cluster_id: ID,
pub status: String
}
#[derive(Serialize, Traversable)]
pub struct ClustersReturnType<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub region: Option<&'a Value>,
    pub updated_at: Option<&'a Value>,
    pub status: Option<&'a Value>,
    pub created_at: Option<&'a Value>,
    pub api_url: Option<&'a Value>,
}

#[handler]
pub fn UpdateClusterStatus (input: HandlerInput) -> Result<Response, GraphError> {
let db = Arc::clone(&input.graph.storage);
let data = input.request.in_fmt.deserialize::<UpdateClusterStatusInput>(&input.request.body)?;
let arena = Bump::new();
let mut txn = db.graph_env.write_txn().map_err(|e| GraphError::New(format!("Failed to start write transaction: {:?}", e)))?;
    let clusters = {let update_tr = G::new(&db, &txn, &arena)
.n_from_id(&data.cluster_id)
    .collect_to::<Vec<_>>();G::new_mut_from_iter(&db, &mut txn, update_tr.iter().cloned(), &arena)
    .update(&[("status", Value::from(&data.status))])
    .collect_to_obj()}
    .map(|val| ClustersReturnType {
            id: uuid_str(cluster.id(), &arena),
            label: cluster.label(),
            region: cluster.get_property("region").unwrap(),
            updated_at: cluster.get_property("updated_at").unwrap(),
            status: cluster.get_property("status").unwrap(),
            created_at: cluster.get_property("created_at").unwrap(),
            api_url: cluster.get_property("api_url").unwrap(),
        });
txn.commit().map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
let response = clusters;
Ok(input.request.out_fmt.create_response(&response))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct LookupUserInput {

pub gh_id: u64
}
#[derive(Serialize, Traversable)]
pub struct UserReturnType<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub updated_at: Option<&'a Value>,
    pub email: Option<&'a Value>,
    pub gh_login: Option<&'a Value>,
    pub gh_id: Option<&'a Value>,
    pub name: Option<&'a Value>,
    pub created_at: Option<&'a Value>,
}

#[handler]
pub fn LookupUser (input: HandlerInput) -> Result<Response, GraphError> {
let db = Arc::clone(&input.graph.storage);
let data = input.request.in_fmt.deserialize::<LookupUserInput>(&input.request.body)?;
let arena = Bump::new();
let txn = db.graph_env.read_txn().map_err(|e| GraphError::New(format!("Failed to start read transaction: {:?}", e)))?;
    let user = G::new(&db, &txn, &arena)
.n_from_index("User", "gh_id", &data.gh_id).collect_to_obj();
txn.commit().map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
let response = user;
Ok(input.request.out_fmt.create_response(&response))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct GetClusterURLInput {

pub cluster_id: ID
}
#[derive(Serialize, Traversable)]
pub struct ClustersReturnType<'a> {
}

#[handler]
pub fn GetClusterURL (input: HandlerInput) -> Result<Response, GraphError> {
let db = Arc::clone(&input.graph.storage);
let data = input.request.in_fmt.deserialize::<GetClusterURLInput>(&input.request.body)?;
let arena = Bump::new();
let txn = db.graph_env.read_txn().map_err(|e| GraphError::New(format!("Failed to start read transaction: {:?}", e)))?;
    let clusters = G::new(&db, &txn, &arena)
.n_from_id(&data.cluster_id).collect_to_obj();
txn.commit().map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
let response = clusters;
Ok(input.request.out_fmt.create_response(&response))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct VerifyUserAccessToClusterInput {

pub user_id: ID,
pub cluster_id: ID
}
#[derive(Serialize, Traversable)]
pub struct Can_accessReturnType<'a> {
}

#[handler]
pub fn VerifyUserAccessToCluster (input: HandlerInput) -> Result<Response, GraphError> {
let db = Arc::clone(&input.graph.storage);
let data = input.request.in_fmt.deserialize::<VerifyUserAccessToClusterInput>(&input.request.body)?;
let arena = Bump::new();
let txn = db.graph_env.read_txn().map_err(|e| GraphError::New(format!("Failed to start read transaction: {:?}", e)))?;
    let user = G::new(&db, &txn, &arena)
.n_from_id(&data.user_id).collect_to_obj();
    let cluster = G::new(&db, &txn, &arena)
.n_from_id(&data.cluster_id).collect_to_obj();
    let can_access = Exist::exists(&mut G::from_iter(&db, &txn, std::iter::once(user.clone()), &arena)

.out_node("CreatedCluster")

.filter_ref(|val, txn|{
                if let Ok(val) = val {
                    Ok(val
                    .get_property("id")
                    .map_or(false, |v| *v == data.cluster_id.clone()))
                } else {
                    Ok(false)
                }
            }));
txn.commit().map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
let response = can_access;
Ok(input.request.out_fmt.create_response(&response))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct GetInstancesForUserInput {

pub user_id: ID
}
#[derive(Serialize, Traversable)]
pub struct InstancesReturnType<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub ram_gb: Option<&'a Value>,
    pub instance_type: Option<&'a Value>,
    pub api_url: Option<&'a Value>,
    pub created_at: Option<&'a Value>,
    pub region: Option<&'a Value>,
    pub status: Option<&'a Value>,
    pub storage_gb: Option<&'a Value>,
    pub updated_at: Option<&'a Value>,
}

#[handler]
pub fn GetInstancesForUser (input: HandlerInput) -> Result<Response, GraphError> {
let db = Arc::clone(&input.graph.storage);
let data = input.request.in_fmt.deserialize::<GetInstancesForUserInput>(&input.request.body)?;
let arena = Bump::new();
let txn = db.graph_env.read_txn().map_err(|e| GraphError::New(format!("Failed to start read transaction: {:?}", e)))?;
    let instances = G::new(&db, &txn, &arena)
.n_from_id(&data.user_id)

.out_node("CreatedCluster")

.out_node("CreatedInstance")
    .map(|val| InstancesReturnType {
            id: uuid_str(instance.id(), &arena),
            label: instance.label(),
            ram_gb: instance.get_property("ram_gb").unwrap(),
            instance_type: instance.get_property("instance_type").unwrap(),
            api_url: instance.get_property("api_url").unwrap(),
            created_at: instance.get_property("created_at").unwrap(),
            region: instance.get_property("region").unwrap(),
            status: instance.get_property("status").unwrap(),
            storage_gb: instance.get_property("storage_gb").unwrap(),
            updated_at: instance.get_property("updated_at").unwrap(),
        }).collect_to::<Vec<_>>();
txn.commit().map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
let response = instances;
Ok(input.request.out_fmt.create_response(&response))
}


