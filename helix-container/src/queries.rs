// DEFAULT CODE
// use helix_db::helix_engine::traversal_core::config::Config;

// pub fn config() -> Option<Config> {
//     None
// }

use bumpalo::Bump;
use chrono::{DateTime, Utc};
use heed3::RoTxn;
use helix_db::{
    embed, embed_async, field_addition_from_old_field, field_addition_from_value, field_type_cast,
    helix_engine::{
        reranker::{
            RerankAdapter,
            fusion::{DistanceMethod, MMRReranker, RRFReranker},
        },
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
        mcp::mcp::{MCPHandler, MCPHandlerSubmission, MCPToolInput},
        router::router::{HandlerInput, IoContFn},
    },
    node_matches, props,
    protocol::{
        format::Format,
        response::Response,
        value::{
            Value,
            casting::{CastType, cast},
        },
    },
    utils::{
        id::{ID, uuid_str},
        items::{Edge, Node},
        properties::ImmutablePropertiesMap,
    },
};
use helix_macros::{handler, mcp_handler, migration, tool_call};
use sonic_rs::{Deserialize, Serialize, json};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

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
secondary_indices: Some(vec![SecondaryIndex::Index("github_id".to_string()), SecondaryIndex::Index("url_slug".to_string()), SecondaryIndex::Index("railway_project_id".to_string())]),
}),
db_max_size_gb: Some(10),
mcp: Some(true),
bm25: Some(true),
schema: Some(r#"{
  "schema": {
    "nodes": [
      {
        "name": "Project",
        "properties": {
          "created_at": "Date",
          "updated_at": "Date",
          "label": "String",
          "id": "ID",
          "name": "String"
        }
      },
      {
        "name": "ApiKey",
        "properties": {
          "id": "ID",
          "label": "String",
          "created_at": "Date",
          "unkey_key_id": "String"
        }
      },
      {
        "name": "Cluster",
        "properties": {
          "db_url": "String",
          "railway_project_id": "String",
          "updated_at": "Date",
          "railway_region": "String",
          "id": "ID",
          "label": "String",
          "build_mode": "String",
          "created_at": "Date",
          "cluster_name": "String"
        }
      },
      {
        "name": "Instance",
        "properties": {
          "railway_service_id": "String",
          "ram_gb": "U64",
          "id": "ID",
          "instance_type": "String",
          "railway_environment_id": "String",
          "created_at": "Date",
          "label": "String",
          "storage_gb": "U64",
          "updated_at": "Date"
        }
      },
      {
        "name": "Workspace",
        "properties": {
          "url_slug": "String",
          "updated_at": "Date",
          "id": "ID",
          "workspace_type": "String",
          "label": "String",
          "created_at": "Date",
          "plan": "String",
          "icon": "String",
          "name": "String"
        }
      },
      {
        "name": "User",
        "properties": {
          "github_login": "String",
          "label": "String",
          "created_at": "Date",
          "id": "ID",
          "updated_at": "Date",
          "github_email": "String",
          "github_name": "String",
          "github_id": "U64"
        }
      }
    ],
    "vectors": [],
    "edges": [
      {
        "name": "MemberOf",
        "from": "User",
        "to": "Workspace",
        "properties": {
          "joined_at": "Date",
          "role": "String"
        }
      },
      {
        "name": "HasCluster",
        "from": "Project",
        "to": "Cluster",
        "properties": {}
      },
      {
        "name": "HasProject",
        "from": "Workspace",
        "to": "Project",
        "properties": {}
      },
      {
        "name": "HasInstance",
        "from": "Cluster",
        "to": "Instance",
        "properties": {}
      },
      {
        "name": "CreatedApiKey",
        "from": "User",
        "to": "ApiKey",
        "properties": {}
      }
    ]
  },
  "queries": [
    {
      "name": "GetProjectClusters",
      "parameters": {
        "project_id": "ID"
      },
      "returns": [
        "clusters"
      ]
    },
    {
      "name": "UpdateCluster",
      "parameters": {
        "cluster_id": "ID",
        "db_url": "String",
        "timestamp": "Date"
      },
      "returns": [
        "cluster"
      ]
    },
    {
      "name": "CreateProject",
      "parameters": {
        "workspace_id": "ID",
        "name": "String"
      },
      "returns": []
    },
    {
      "name": "DeleteCluster",
      "parameters": {
        "cluster_id": "ID"
      },
      "returns": []
    },
    {
      "name": "CreateUserGetUserId",
      "parameters": {
        "github_id": "U64",
        "github_email": "String",
        "github_login": "String",
        "github_name": "String"
      },
      "returns": []
    },
    {
      "name": "UpdateUsername",
      "parameters": {
        "github_name": "String",
        "timestamp": "Date",
        "user_id": "ID"
      },
      "returns": [
        "user"
      ]
    },
    {
      "name": "AddWorkspaceMember",
      "parameters": {
        "user_id": "ID",
        "role": "String",
        "workspace_id": "ID"
      },
      "returns": []
    },
    {
      "name": "GetUserById",
      "parameters": {
        "user_id": "ID"
      },
      "returns": [
        "user"
      ]
    },
    {
      "name": "CreateClustersInProject",
      "parameters": {
        "project_id": "ID",
        "clusters": "Array({db_url: Stringbuild_mode: Stringcluster_name: Stringrailway_project_id: String})"
      },
      "returns": [
        "final_clusters"
      ]
    },
    {
      "name": "DeleteProject",
      "parameters": {
        "project_id": "ID"
      },
      "returns": []
    },
    {
      "name": "UpdateWorkspace",
      "parameters": {
        "workspace_id": "ID",
        "timestamp": "Date",
        "icon": "String",
        "name": "String"
      },
      "returns": [
        "workspace"
      ]
    },
    {
      "name": "UpdateWorkspaceMemberRole",
      "parameters": {
        "role_id": "ID",
        "role": "String"
      },
      "returns": []
    },
    {
      "name": "UserHasProjectAccess",
      "parameters": {
        "user_id": "ID",
        "project_id": "ID"
      },
      "returns": [
        "has_access"
      ]
    },
    {
      "name": "UpdateProject",
      "parameters": {
        "name": "String",
        "project_id": "ID",
        "timestamp": "Date"
      },
      "returns": [
        "project"
      ]
    },
    {
      "name": "GetWorkspaceMembers",
      "parameters": {
        "workspace_id": "ID"
      },
      "returns": []
    },
    {
      "name": "ExistsUserByGithubId",
      "parameters": {
        "github_id": "U64"
      },
      "returns": [
        "user_exists"
      ]
    },
    {
      "name": "UserIdByGithubId",
      "parameters": {
        "github_id": "U64"
      },
      "returns": []
    },
    {
      "name": "GetWorkspace",
      "parameters": {
        "workspace_id": "ID"
      },
      "returns": [
        "workspace"
      ]
    },
    {
      "name": "GetCluster",
      "parameters": {
        "cluster_id": "ID"
      },
      "returns": [
        "cluster"
      ]
    },
    {
      "name": "CreateWorkspace",
      "parameters": {
        "user_id": "ID",
        "name": "String",
        "url_slug": "String"
      },
      "returns": []
    },
    {
      "name": "GetUserWorkspaces",
      "parameters": {
        "user_id": "ID"
      },
      "returns": []
    },
    {
      "name": "GetProject",
      "parameters": {
        "project_id": "ID"
      },
      "returns": [
        "project"
      ]
    },
    {
      "name": "RemoveWorkspaceMember",
      "parameters": {
        "role_id": "ID"
      },
      "returns": []
    },
    {
      "name": "GetWorkspaceProjects",
      "parameters": {
        "workspace_id": "ID"
      },
      "returns": []
    },
    {
      "name": "DeleteWorkspace",
      "parameters": {
        "workspace_id": "ID"
      },
      "returns": []
    }
  ]
}"#.to_string()),
embedding_model: Some("text-embedding-ada-002".to_string()),
graphvis_node_label: None,
});
}
pub struct User {
    pub github_id: u64,
    pub github_login: String,
    pub github_name: String,
    pub github_email: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct Workspace {
    pub url_slug: String,
    pub name: String,
    pub workspace_type: String,
    pub icon: String,
    pub plan: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct Project {
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct Cluster {
    pub railway_project_id: String,
    pub cluster_name: String,
    pub railway_region: String,
    pub db_url: String,
    pub build_mode: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct Instance {
    pub railway_service_id: String,
    pub railway_environment_id: String,
    pub instance_type: String,
    pub storage_gb: u64,
    pub ram_gb: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct ApiKey {
    pub unkey_key_id: String,
    pub created_at: DateTime<Utc>,
}

pub struct MemberOf {
    pub from: User,
    pub to: Workspace,
    pub role: String,
    pub joined_at: DateTime<Utc>,
}

pub struct HasProject {
    pub from: Workspace,
    pub to: Project,
}

pub struct HasCluster {
    pub from: Project,
    pub to: Cluster,
}

pub struct HasInstance {
    pub from: Cluster,
    pub to: Instance,
}

pub struct CreatedApiKey {
    pub from: User,
    pub to: ApiKey,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct GetProjectClustersInput {
    pub project_id: ID,
}
#[derive(Serialize)]
pub struct GetProjectClustersClustersReturnType<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub build_mode: Option<&'a Value>,
    pub db_url: Option<&'a Value>,
    pub created_at: Option<&'a Value>,
    pub railway_project_id: Option<&'a Value>,
    pub cluster_name: Option<&'a Value>,
    pub railway_region: Option<&'a Value>,
    pub updated_at: Option<&'a Value>,
}

#[handler]
pub fn GetProjectClusters(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<GetProjectClustersInput>(&input.request.body)?;
    let arena = Bump::new();
    let txn = db
        .graph_env
        .read_txn()
        .map_err(|e| GraphError::New(format!("Failed to start read transaction: {:?}", e)))?;
    let project = G::new(&db, &txn, &arena)
        .n_from_id(&data.project_id)
        .collect_to_obj()?;
    let clusters = G::from_iter(&db, &txn, std::iter::once(project.clone()), &arena)
        .out_node("HasCluster")
        .collect::<Result<Vec<_>, _>>()?;
    let response = json!({
        "clusters": clusters.iter().map(|cluster| GetProjectClustersClustersReturnType {
            id: uuid_str(cluster.id(), &arena),
            label: cluster.label(),
            build_mode: cluster.get_property("build_mode"),
            db_url: cluster.get_property("db_url"),
            created_at: cluster.get_property("created_at"),
            railway_project_id: cluster.get_property("railway_project_id"),
            cluster_name: cluster.get_property("cluster_name"),
            railway_region: cluster.get_property("railway_region"),
            updated_at: cluster.get_property("updated_at"),
        }).collect::<Vec<_>>()
    });
    txn.commit()
        .map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
    Ok(input.request.out_fmt.create_response(&response))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct UpdateClusterInput {
    pub cluster_id: ID,
    pub db_url: String,
    pub timestamp: DateTime<Utc>,
}
#[derive(Serialize)]
pub struct UpdateClusterClusterReturnType<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub build_mode: Option<&'a Value>,
    pub db_url: Option<&'a Value>,
    pub created_at: Option<&'a Value>,
    pub railway_project_id: Option<&'a Value>,
    pub cluster_name: Option<&'a Value>,
    pub railway_region: Option<&'a Value>,
    pub updated_at: Option<&'a Value>,
}

#[handler(is_write)]
pub fn UpdateCluster(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<UpdateClusterInput>(&input.request.body)?;
    let arena = Bump::new();
    let mut txn = db
        .graph_env
        .write_txn()
        .map_err(|e| GraphError::New(format!("Failed to start write transaction: {:?}", e)))?;
    let cluster = {
        let update_tr = G::new(&db, &txn, &arena)
            .n_from_id(&data.cluster_id)
            .collect::<Result<Vec<_>, _>>()?;
        G::new_mut_from_iter(&db, &mut txn, update_tr.iter().cloned(), &arena)
            .update(&[
                ("db_url", Value::from(&data.db_url)),
                ("updated_at", Value::from(&data.timestamp)),
            ])
            .collect_to_obj()?
    };
    let response = json!({
        "cluster": UpdateClusterClusterReturnType {
            id: uuid_str(cluster.id(), &arena),
            label: cluster.label(),
            build_mode: cluster.get_property("build_mode"),
            db_url: cluster.get_property("db_url"),
            created_at: cluster.get_property("created_at"),
            railway_project_id: cluster.get_property("railway_project_id"),
            cluster_name: cluster.get_property("cluster_name"),
            railway_region: cluster.get_property("railway_region"),
            updated_at: cluster.get_property("updated_at"),
        }
    });
    txn.commit()
        .map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
    Ok(input.request.out_fmt.create_response(&response))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct CreateProjectInput {
    pub workspace_id: ID,
    pub name: String,
}
#[handler(is_write)]
pub fn CreateProject(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<CreateProjectInput>(&input.request.body)?;
    let arena = Bump::new();
    let mut txn = db
        .graph_env
        .write_txn()
        .map_err(|e| GraphError::New(format!("Failed to start write transaction: {:?}", e)))?;
    let workspace = G::new(&db, &txn, &arena)
        .n_from_id(&data.workspace_id)
        .collect_to_obj()?;
    let project = G::new_mut(&db, &arena, &mut txn)
        .add_n(
            "Project",
            Some(ImmutablePropertiesMap::new(
                3,
                vec![
                    ("name", Value::from(&data.name)),
                    ("created_at", Value::from(chrono::Utc::now().to_rfc3339())),
                    ("updated_at", Value::from(chrono::Utc::now().to_rfc3339())),
                ]
                .into_iter(),
                &arena,
            )),
            None,
        )
        .collect_to_obj()?;
    G::new_mut(&db, &arena, &mut txn)
        .add_edge(
            "HasProject",
            None,
            workspace.id(),
            project.id(),
            false,
            false,
        )
        .collect_to_obj()?;
    let response = json!({
        "project": uuid_str(project.id(), &arena)
    });
    txn.commit()
        .map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
    Ok(input.request.out_fmt.create_response(&response))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct DeleteClusterInput {
    pub cluster_id: ID,
}
#[handler(is_write)]
pub fn DeleteCluster(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<DeleteClusterInput>(&input.request.body)?;
    let arena = Bump::new();
    let mut txn = db
        .graph_env
        .write_txn()
        .map_err(|e| GraphError::New(format!("Failed to start write transaction: {:?}", e)))?;
    Drop::drop_traversal(
        G::new(&db, &txn, &arena)
            .n_from_id(&data.cluster_id)
            .out_node("HasInstance")
            .collect::<Vec<_>>()
            .into_iter(),
        &db,
        &mut txn,
    )?;
    Drop::drop_traversal(
        G::new(&db, &txn, &arena)
            .n_from_id(&data.cluster_id)
            .collect::<Vec<_>>()
            .into_iter(),
        &db,
        &mut txn,
    )?;
    txn.commit()
        .map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
    Ok(input.request.out_fmt.create_response(&()))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct CreateUserGetUserIdInput {
    pub github_id: u64,
    pub github_login: String,
    pub github_name: String,
    pub github_email: String,
}
#[handler(is_write)]
pub fn CreateUserGetUserId(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<CreateUserGetUserIdInput>(&input.request.body)?;
    let arena = Bump::new();
    let mut txn = db
        .graph_env
        .write_txn()
        .map_err(|e| GraphError::New(format!("Failed to start write transaction: {:?}", e)))?;
    let user = G::new_mut(&db, &arena, &mut txn)
        .add_n(
            "User",
            Some(ImmutablePropertiesMap::new(
                6,
                vec![
                    ("github_login", Value::from(&data.github_login)),
                    ("github_email", Value::from(&data.github_email)),
                    ("github_id", Value::from(&data.github_id)),
                    ("github_name", Value::from(&data.github_name)),
                    ("updated_at", Value::from(chrono::Utc::now().to_rfc3339())),
                    ("created_at", Value::from(chrono::Utc::now().to_rfc3339())),
                ]
                .into_iter(),
                &arena,
            )),
            Some(&["github_id"]),
        )
        .collect_to_obj()?;
    let workspace = G::new_mut(&db, &arena, &mut txn)
        .add_n(
            "Workspace",
            Some(ImmutablePropertiesMap::new(
                7,
                vec![
                    ("updated_at", Value::from(chrono::Utc::now().to_rfc3339())),
                    ("workspace_type", Value::from("personal")),
                    ("url_slug", Value::from(&data.github_login)),
                    ("icon", Value::from("")),
                    ("name", Value::from("Personal")),
                    ("plan", Value::from("none")),
                    ("created_at", Value::from(chrono::Utc::now().to_rfc3339())),
                ]
                .into_iter(),
                &arena,
            )),
            Some(&["url_slug"]),
        )
        .collect_to_obj()?;
    G::new_mut(&db, &arena, &mut txn)
        .add_edge(
            "MemberOf",
            Some(ImmutablePropertiesMap::new(
                2,
                vec![
                    ("joined_at", Value::from(chrono::Utc::now().to_rfc3339())),
                    ("role", Value::from("owner")),
                ]
                .into_iter(),
                &arena,
            )),
            user.id(),
            workspace.id(),
            false,
            false,
        )
        .collect_to_obj()?;
    let response = json!({
        "user": uuid_str(user.id(), &arena)
    });
    txn.commit()
        .map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
    Ok(input.request.out_fmt.create_response(&response))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct UpdateUsernameInput {
    pub user_id: ID,
    pub github_name: String,
    pub timestamp: DateTime<Utc>,
}
#[derive(Serialize)]
pub struct UpdateUsernameUserReturnType<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub github_name: Option<&'a Value>,
    pub github_id: Option<&'a Value>,
    pub github_login: Option<&'a Value>,
    pub github_email: Option<&'a Value>,
    pub updated_at: Option<&'a Value>,
    pub created_at: Option<&'a Value>,
}

#[handler(is_write)]
pub fn UpdateUsername(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<UpdateUsernameInput>(&input.request.body)?;
    let arena = Bump::new();
    let mut txn = db
        .graph_env
        .write_txn()
        .map_err(|e| GraphError::New(format!("Failed to start write transaction: {:?}", e)))?;
    let user = {
        let update_tr = G::new(&db, &txn, &arena)
            .n_from_id(&data.user_id)
            .collect::<Result<Vec<_>, _>>()?;
        G::new_mut_from_iter(&db, &mut txn, update_tr.iter().cloned(), &arena)
            .update(&[
                ("github_name", Value::from(&data.github_name)),
                ("updated_at", Value::from(&data.timestamp)),
            ])
            .collect_to_obj()?
    };
    let response = json!({
        "user": UpdateUsernameUserReturnType {
            id: uuid_str(user.id(), &arena),
            label: user.label(),
            github_name: user.get_property("github_name"),
            github_id: user.get_property("github_id"),
            github_login: user.get_property("github_login"),
            github_email: user.get_property("github_email"),
            updated_at: user.get_property("updated_at"),
            created_at: user.get_property("created_at"),
        }
    });
    txn.commit()
        .map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
    Ok(input.request.out_fmt.create_response(&response))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct AddWorkspaceMemberInput {
    pub workspace_id: ID,
    pub user_id: ID,
    pub role: String,
}
#[handler(is_write)]
pub fn AddWorkspaceMember(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<AddWorkspaceMemberInput>(&input.request.body)?;
    let arena = Bump::new();
    let mut txn = db
        .graph_env
        .write_txn()
        .map_err(|e| GraphError::New(format!("Failed to start write transaction: {:?}", e)))?;
    let workspace = G::new(&db, &txn, &arena)
        .n_from_id(&data.workspace_id)
        .collect_to_obj()?;
    let user = G::new(&db, &txn, &arena)
        .n_from_id(&data.user_id)
        .collect_to_obj()?;
    G::new_mut(&db, &arena, &mut txn)
        .add_edge(
            "MemberOf",
            Some(ImmutablePropertiesMap::new(
                2,
                vec![
                    ("joined_at", Value::from(chrono::Utc::now().to_rfc3339())),
                    ("role", Value::from(data.role.clone())),
                ]
                .into_iter(),
                &arena,
            )),
            user.id(),
            workspace.id(),
            false,
            false,
        )
        .collect_to_obj()?;
    txn.commit()
        .map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
    Ok(input.request.out_fmt.create_response(&()))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct GetUserByIdInput {
    pub user_id: ID,
}
#[derive(Serialize)]
pub struct GetUserByIdUserReturnType<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub github_name: Option<&'a Value>,
    pub github_id: Option<&'a Value>,
    pub github_login: Option<&'a Value>,
    pub github_email: Option<&'a Value>,
    pub updated_at: Option<&'a Value>,
    pub created_at: Option<&'a Value>,
}

#[handler]
pub fn GetUserById(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<GetUserByIdInput>(&input.request.body)?;
    let arena = Bump::new();
    let txn = db
        .graph_env
        .read_txn()
        .map_err(|e| GraphError::New(format!("Failed to start read transaction: {:?}", e)))?;
    let user = G::new(&db, &txn, &arena)
        .n_from_id(&data.user_id)
        .collect_to_obj()?;
    let response = json!({
        "user": GetUserByIdUserReturnType {
            id: uuid_str(user.id(), &arena),
            label: user.label(),
            github_name: user.get_property("github_name"),
            github_id: user.get_property("github_id"),
            github_login: user.get_property("github_login"),
            github_email: user.get_property("github_email"),
            updated_at: user.get_property("updated_at"),
            created_at: user.get_property("created_at"),
        }
    });
    txn.commit()
        .map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
    Ok(input.request.out_fmt.create_response(&response))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct CreateClustersInProjectInput {
    pub project_id: ID,
    pub clusters: Vec<CreateClustersInProjectClustersData>,
}
#[derive(Serialize, Deserialize, Clone)]
pub struct CreateClustersInProjectClustersData {
    pub db_url: String,
    pub build_mode: String,
    pub cluster_name: String,
    pub railway_project_id: String,
}
#[derive(Serialize)]
pub struct CreateClustersInProjectFinal_clustersReturnType<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub build_mode: Option<&'a Value>,
    pub db_url: Option<&'a Value>,
    pub created_at: Option<&'a Value>,
    pub railway_project_id: Option<&'a Value>,
    pub cluster_name: Option<&'a Value>,
    pub railway_region: Option<&'a Value>,
    pub updated_at: Option<&'a Value>,
}

#[handler(is_write)]
pub fn CreateClustersInProject(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<CreateClustersInProjectInput>(&input.request.body)?;
    let arena = Bump::new();
    let mut txn = db
        .graph_env
        .write_txn()
        .map_err(|e| GraphError::New(format!("Failed to start write transaction: {:?}", e)))?;
    let project = G::new(&db, &txn, &arena)
        .n_from_id(&data.project_id)
        .collect_to_obj()?;
    for CreateClustersInProjectClustersData {
        railway_project_id,
        cluster_name,
        db_url,
        build_mode,
    } in &data.clusters
    {
        let cluster = G::new_mut(&db, &arena, &mut txn)
            .add_n(
                "Cluster",
                Some(ImmutablePropertiesMap::new(
                    7,
                    vec![
                        ("build_mode", Value::from(&build_mode)),
                        ("railway_project_id", Value::from(&railway_project_id)),
                        ("railway_region", Value::from("us-east4-eqdc4a")),
                        ("cluster_name", Value::from(&cluster_name)),
                        ("created_at", Value::from(chrono::Utc::now().to_rfc3339())),
                        ("db_url", Value::from(&db_url)),
                        ("updated_at", Value::from(chrono::Utc::now().to_rfc3339())),
                    ]
                    .into_iter(),
                    &arena,
                )),
                Some(&["railway_project_id"]),
            )
            .collect_to_obj()?;
        G::new_mut(&db, &arena, &mut txn)
            .add_edge("HasCluster", None, project.id(), cluster.id(), false, false)
            .collect_to_obj()?;
    }
    let final_clusters = G::from_iter(&db, &txn, std::iter::once(project.clone()), &arena)
        .out_node("HasCluster")
        .collect::<Result<Vec<_>, _>>()?;
    let response = json!({
        "final_clusters": final_clusters.iter().map(|final_cluster| CreateClustersInProjectFinal_clustersReturnType {
            id: uuid_str(final_cluster.id(), &arena),
            label: final_cluster.label(),
            build_mode: final_cluster.get_property("build_mode"),
            db_url: final_cluster.get_property("db_url"),
            created_at: final_cluster.get_property("created_at"),
            railway_project_id: final_cluster.get_property("railway_project_id"),
            cluster_name: final_cluster.get_property("cluster_name"),
            railway_region: final_cluster.get_property("railway_region"),
            updated_at: final_cluster.get_property("updated_at"),
        }).collect::<Vec<_>>()
    });
    txn.commit()
        .map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
    Ok(input.request.out_fmt.create_response(&response))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct DeleteProjectInput {
    pub project_id: ID,
}
#[handler(is_write)]
pub fn DeleteProject(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<DeleteProjectInput>(&input.request.body)?;
    let arena = Bump::new();
    let mut txn = db
        .graph_env
        .write_txn()
        .map_err(|e| GraphError::New(format!("Failed to start write transaction: {:?}", e)))?;
    Drop::drop_traversal(
        G::new(&db, &txn, &arena)
            .n_from_id(&data.project_id)
            .out_node("HasCluster")
            .out_node("HasInstance")
            .collect::<Vec<_>>()
            .into_iter(),
        &db,
        &mut txn,
    )?;
    Drop::drop_traversal(
        G::new(&db, &txn, &arena)
            .n_from_id(&data.project_id)
            .out_node("HasCluster")
            .collect::<Vec<_>>()
            .into_iter(),
        &db,
        &mut txn,
    )?;
    Drop::drop_traversal(
        G::new(&db, &txn, &arena)
            .n_from_id(&data.project_id)
            .collect::<Vec<_>>()
            .into_iter(),
        &db,
        &mut txn,
    )?;
    txn.commit()
        .map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
    Ok(input.request.out_fmt.create_response(&()))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct UpdateWorkspaceInput {
    pub workspace_id: ID,
    pub name: String,
    pub icon: String,
    pub timestamp: DateTime<Utc>,
}
#[derive(Serialize)]
pub struct UpdateWorkspaceWorkspaceReturnType<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub icon: Option<&'a Value>,
    pub updated_at: Option<&'a Value>,
    pub name: Option<&'a Value>,
    pub url_slug: Option<&'a Value>,
    pub workspace_type: Option<&'a Value>,
    pub plan: Option<&'a Value>,
    pub created_at: Option<&'a Value>,
}

#[handler(is_write)]
pub fn UpdateWorkspace(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<UpdateWorkspaceInput>(&input.request.body)?;
    let arena = Bump::new();
    let mut txn = db
        .graph_env
        .write_txn()
        .map_err(|e| GraphError::New(format!("Failed to start write transaction: {:?}", e)))?;
    let workspace = {
        let update_tr = G::new(&db, &txn, &arena)
            .n_from_id(&data.workspace_id)
            .collect::<Result<Vec<_>, _>>()?;
        G::new_mut_from_iter(&db, &mut txn, update_tr.iter().cloned(), &arena)
            .update(&[
                ("name", Value::from(&data.name)),
                ("icon", Value::from(&data.icon)),
                ("updated_at", Value::from(&data.timestamp)),
            ])
            .collect_to_obj()?
    };
    let response = json!({
        "workspace": UpdateWorkspaceWorkspaceReturnType {
            id: uuid_str(workspace.id(), &arena),
            label: workspace.label(),
            icon: workspace.get_property("icon"),
            updated_at: workspace.get_property("updated_at"),
            name: workspace.get_property("name"),
            url_slug: workspace.get_property("url_slug"),
            workspace_type: workspace.get_property("workspace_type"),
            plan: workspace.get_property("plan"),
            created_at: workspace.get_property("created_at"),
        }
    });
    txn.commit()
        .map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
    Ok(input.request.out_fmt.create_response(&response))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct UpdateWorkspaceMemberRoleInput {
    pub role_id: ID,
    pub role: String,
}
#[handler(is_write)]
pub fn UpdateWorkspaceMemberRole(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<UpdateWorkspaceMemberRoleInput>(&input.request.body)?;
    let arena = Bump::new();
    let mut txn = db
        .graph_env
        .write_txn()
        .map_err(|e| GraphError::New(format!("Failed to start write transaction: {:?}", e)))?;
    let foo = {
        let update_tr = G::new(&db, &txn, &arena)
            .e_from_id(&data.role_id)
            .collect::<Result<Vec<_>, _>>()?;
        G::new_mut_from_iter(&db, &mut txn, update_tr.iter().cloned(), &arena)
            .update(&[("role", Value::from(&data.role))])
            .collect_to_obj()?
    };
    txn.commit()
        .map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
    Ok(input.request.out_fmt.create_response(&()))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct UserHasProjectAccessInput {
    pub user_id: ID,
    pub project_id: ID,
}
#[handler]
pub fn UserHasProjectAccess(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<UserHasProjectAccessInput>(&input.request.body)?;
    let arena = Bump::new();
    let txn = db
        .graph_env
        .read_txn()
        .map_err(|e| GraphError::New(format!("Failed to start read transaction: {:?}", e)))?;
    let user = G::new(&db, &txn, &arena)
        .n_from_id(&data.user_id)
        .collect_to_obj()?;
    let has_access = Exist::exists(
        &mut G::from_iter(&db, &txn, std::iter::once(user.clone()), &arena)
            .out_node("MemberOf")
            .out_node("HasProject")
            .filter_ref(|val, txn| {
                if let Ok(val) = val {
                    Ok(Value::Id(ID::from(val.id())) == data.project_id.clone())
                } else {
                    Ok(false)
                }
            }),
    );
    let response = json!({
        "has_access": has_access
    });
    txn.commit()
        .map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
    Ok(input.request.out_fmt.create_response(&response))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct UpdateProjectInput {
    pub project_id: ID,
    pub name: String,
    pub timestamp: DateTime<Utc>,
}
#[derive(Serialize)]
pub struct UpdateProjectProjectReturnType<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub name: Option<&'a Value>,
    pub created_at: Option<&'a Value>,
    pub updated_at: Option<&'a Value>,
}

#[handler(is_write)]
pub fn UpdateProject(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<UpdateProjectInput>(&input.request.body)?;
    let arena = Bump::new();
    let mut txn = db
        .graph_env
        .write_txn()
        .map_err(|e| GraphError::New(format!("Failed to start write transaction: {:?}", e)))?;
    let project = {
        let update_tr = G::new(&db, &txn, &arena)
            .n_from_id(&data.project_id)
            .collect::<Result<Vec<_>, _>>()?;
        G::new_mut_from_iter(&db, &mut txn, update_tr.iter().cloned(), &arena)
            .update(&[
                ("name", Value::from(&data.name)),
                ("updated_at", Value::from(&data.timestamp)),
            ])
            .collect_to_obj()?
    };
    let response = json!({
        "project": UpdateProjectProjectReturnType {
            id: uuid_str(project.id(), &arena),
            label: project.label(),
            name: project.get_property("name"),
            created_at: project.get_property("created_at"),
            updated_at: project.get_property("updated_at"),
        }
    });
    txn.commit()
        .map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
    Ok(input.request.out_fmt.create_response(&response))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct GetWorkspaceMembersInput {
    pub workspace_id: ID,
}
#[derive(Serialize)]
pub struct GetWorkspaceMembersMembersReturnType<'a> {
    pub role: Option<&'a Value>,
    pub role_id: &'a str,
    pub user: TraversalValue<'a>,
}

#[handler]
pub fn GetWorkspaceMembers(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<GetWorkspaceMembersInput>(&input.request.body)?;
    let arena = Bump::new();
    let txn = db
        .graph_env
        .read_txn()
        .map_err(|e| GraphError::New(format!("Failed to start read transaction: {:?}", e)))?;
    let workspace = G::new(&db, &txn, &arena)
        .n_from_id(&data.workspace_id)
        .collect_to_obj()?;
    let members = G::from_iter(&db, &txn, std::iter::once(workspace.clone()), &arena)
        .in_e("MemberOf")
        .collect::<Result<Vec<_>, _>>()?;
    let response = json!({
        "members": members.iter().map(|member| Ok::<_, GraphError>(GetWorkspaceMembersMembersReturnType {
            role: member.get_property("role"),
            role_id: uuid_str(member.id(), &arena),
            user: G::from_iter(&db, &txn, std::iter::once(member.clone()), &arena)

    .from_n().collect_to_obj()?,
        })).collect::<Result<Vec<_>, GraphError>>()?
    });
    txn.commit()
        .map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
    Ok(input.request.out_fmt.create_response(&response))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ExistsUserByGithubIdInput {
    pub github_id: u64,
}
#[handler]
pub fn ExistsUserByGithubId(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<ExistsUserByGithubIdInput>(&input.request.body)?;
    let arena = Bump::new();
    let txn = db
        .graph_env
        .read_txn()
        .map_err(|e| GraphError::New(format!("Failed to start read transaction: {:?}", e)))?;
    let user_exists = Exist::exists(&mut G::new(&db, &txn, &arena).n_from_index(
        "User",
        "github_id",
        &data.github_id,
    ));
    let response = json!({
        "user_exists": user_exists
    });
    txn.commit()
        .map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
    Ok(input.request.out_fmt.create_response(&response))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct UserIdByGithubIdInput {
    pub github_id: u64,
}
#[handler]
pub fn UserIdByGithubId(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<UserIdByGithubIdInput>(&input.request.body)?;
    let arena = Bump::new();
    let txn = db
        .graph_env
        .read_txn()
        .map_err(|e| GraphError::New(format!("Failed to start read transaction: {:?}", e)))?;
    let user_id = G::new(&db, &txn, &arena)
        .n_from_index("User", "github_id", &data.github_id)
        .collect_to_obj()?;
    let response = json!({
        "user_id": uuid_str(user_id.id(), &arena)
    });
    txn.commit()
        .map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
    Ok(input.request.out_fmt.create_response(&response))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct GetWorkspaceInput {
    pub workspace_id: ID,
}
#[derive(Serialize)]
pub struct GetWorkspaceWorkspaceReturnType<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub icon: Option<&'a Value>,
    pub updated_at: Option<&'a Value>,
    pub name: Option<&'a Value>,
    pub url_slug: Option<&'a Value>,
    pub workspace_type: Option<&'a Value>,
    pub plan: Option<&'a Value>,
    pub created_at: Option<&'a Value>,
}

#[handler]
pub fn GetWorkspace(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<GetWorkspaceInput>(&input.request.body)?;
    let arena = Bump::new();
    let txn = db
        .graph_env
        .read_txn()
        .map_err(|e| GraphError::New(format!("Failed to start read transaction: {:?}", e)))?;
    let workspace = G::new(&db, &txn, &arena)
        .n_from_id(&data.workspace_id)
        .collect_to_obj()?;
    let response = json!({
        "workspace": GetWorkspaceWorkspaceReturnType {
            id: uuid_str(workspace.id(), &arena),
            label: workspace.label(),
            icon: workspace.get_property("icon"),
            updated_at: workspace.get_property("updated_at"),
            name: workspace.get_property("name"),
            url_slug: workspace.get_property("url_slug"),
            workspace_type: workspace.get_property("workspace_type"),
            plan: workspace.get_property("plan"),
            created_at: workspace.get_property("created_at"),
        }
    });
    txn.commit()
        .map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
    Ok(input.request.out_fmt.create_response(&response))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct GetClusterInput {
    pub cluster_id: ID,
}
#[derive(Serialize)]
pub struct GetClusterClusterReturnType<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub build_mode: Option<&'a Value>,
    pub db_url: Option<&'a Value>,
    pub created_at: Option<&'a Value>,
    pub railway_project_id: Option<&'a Value>,
    pub cluster_name: Option<&'a Value>,
    pub railway_region: Option<&'a Value>,
    pub updated_at: Option<&'a Value>,
}

#[handler]
pub fn GetCluster(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<GetClusterInput>(&input.request.body)?;
    let arena = Bump::new();
    let txn = db
        .graph_env
        .read_txn()
        .map_err(|e| GraphError::New(format!("Failed to start read transaction: {:?}", e)))?;
    let cluster = G::new(&db, &txn, &arena)
        .n_from_id(&data.cluster_id)
        .collect_to_obj()?;
    let response = json!({
        "cluster": GetClusterClusterReturnType {
            id: uuid_str(cluster.id(), &arena),
            label: cluster.label(),
            build_mode: cluster.get_property("build_mode"),
            db_url: cluster.get_property("db_url"),
            created_at: cluster.get_property("created_at"),
            railway_project_id: cluster.get_property("railway_project_id"),
            cluster_name: cluster.get_property("cluster_name"),
            railway_region: cluster.get_property("railway_region"),
            updated_at: cluster.get_property("updated_at"),
        }
    });
    txn.commit()
        .map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
    Ok(input.request.out_fmt.create_response(&response))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct CreateWorkspaceInput {
    pub user_id: ID,
    pub name: String,
    pub url_slug: String,
}
#[handler(is_write)]
pub fn CreateWorkspace(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<CreateWorkspaceInput>(&input.request.body)?;
    let arena = Bump::new();
    let mut txn = db
        .graph_env
        .write_txn()
        .map_err(|e| GraphError::New(format!("Failed to start write transaction: {:?}", e)))?;
    let user = G::new(&db, &txn, &arena)
        .n_from_id(&data.user_id)
        .collect_to_obj()?;
    let workspace = G::new_mut(&db, &arena, &mut txn)
        .add_n(
            "Workspace",
            Some(ImmutablePropertiesMap::new(
                7,
                vec![
                    ("name", Value::from(&data.name)),
                    ("updated_at", Value::from(chrono::Utc::now().to_rfc3339())),
                    ("icon", Value::from("")),
                    ("workspace_type", Value::from("organization")),
                    ("created_at", Value::from(chrono::Utc::now().to_rfc3339())),
                    ("plan", Value::from("pro")),
                    ("url_slug", Value::from(&data.url_slug)),
                ]
                .into_iter(),
                &arena,
            )),
            Some(&["url_slug"]),
        )
        .collect_to_obj()?;
    G::new_mut(&db, &arena, &mut txn)
        .add_edge(
            "MemberOf",
            Some(ImmutablePropertiesMap::new(
                2,
                vec![
                    ("role", Value::from("owner")),
                    ("joined_at", Value::from(chrono::Utc::now().to_rfc3339())),
                ]
                .into_iter(),
                &arena,
            )),
            user.id(),
            workspace.id(),
            false,
            false,
        )
        .collect_to_obj()?;
    let response = json!({
        "workspace": uuid_str(workspace.id(), &arena)
    });
    txn.commit()
        .map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
    Ok(input.request.out_fmt.create_response(&response))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct GetUserWorkspacesInput {
    pub user_id: ID,
}
#[derive(Serialize)]
pub struct GetUserWorkspacesWorkspace_edgesReturnType<'a> {
    pub workspace: TraversalValue<'a>,
    pub role: Option<&'a Value>,
    pub role_id: &'a str,
}

#[handler]
pub fn GetUserWorkspaces(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<GetUserWorkspacesInput>(&input.request.body)?;
    let arena = Bump::new();
    let txn = db
        .graph_env
        .read_txn()
        .map_err(|e| GraphError::New(format!("Failed to start read transaction: {:?}", e)))?;
    let user = G::new(&db, &txn, &arena)
        .n_from_id(&data.user_id)
        .collect_to_obj()?;
    let workspace_edges = G::from_iter(&db, &txn, std::iter::once(user.clone()), &arena)
        .out_e("MemberOf")
        .collect::<Result<Vec<_>, _>>()?;
    let response = json!({
        "workspace_edges": workspace_edges.iter().map(|workspace_edge| Ok::<_, GraphError>(GetUserWorkspacesWorkspace_edgesReturnType {
            workspace: G::from_iter(&db, &txn, std::iter::once(workspace_edge.clone()), &arena)

    .to_n().collect_to_obj()?,
            role: workspace_edge.get_property("role"),
            role_id: uuid_str(workspace_edge.id(), &arena),
        })).collect::<Result<Vec<_>, GraphError>>()?
    });
    txn.commit()
        .map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
    Ok(input.request.out_fmt.create_response(&response))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct GetProjectInput {
    pub project_id: ID,
}
#[derive(Serialize)]
pub struct GetProjectProjectReturnType<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub name: Option<&'a Value>,
    pub created_at: Option<&'a Value>,
    pub updated_at: Option<&'a Value>,
}

#[handler]
pub fn GetProject(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<GetProjectInput>(&input.request.body)?;
    let arena = Bump::new();
    let txn = db
        .graph_env
        .read_txn()
        .map_err(|e| GraphError::New(format!("Failed to start read transaction: {:?}", e)))?;
    let project = G::new(&db, &txn, &arena)
        .n_from_id(&data.project_id)
        .collect_to_obj()?;
    let response = json!({
        "project": GetProjectProjectReturnType {
            id: uuid_str(project.id(), &arena),
            label: project.label(),
            name: project.get_property("name"),
            created_at: project.get_property("created_at"),
            updated_at: project.get_property("updated_at"),
        }
    });
    txn.commit()
        .map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
    Ok(input.request.out_fmt.create_response(&response))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct RemoveWorkspaceMemberInput {
    pub role_id: ID,
}
#[handler(is_write)]
pub fn RemoveWorkspaceMember(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<RemoveWorkspaceMemberInput>(&input.request.body)?;
    let arena = Bump::new();
    let mut txn = db
        .graph_env
        .write_txn()
        .map_err(|e| GraphError::New(format!("Failed to start write transaction: {:?}", e)))?;
    Drop::drop_traversal(
        G::new(&db, &txn, &arena)
            .e_from_id(&data.role_id)
            .collect::<Vec<_>>()
            .into_iter(),
        &db,
        &mut txn,
    )?;
    txn.commit()
        .map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
    Ok(input.request.out_fmt.create_response(&()))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct GetWorkspaceProjectsInput {
    pub workspace_id: ID,
}
#[derive(Serialize)]
pub struct GetWorkspaceProjectsProjectsReturnType<'a> {
    pub name: Option<&'a Value>,
    pub created_at: Option<&'a Value>,
    pub label: &'a str,
    pub id: &'a str,
    pub updated_at: Option<&'a Value>,
    pub clusters: Value,
}

#[handler]
pub fn GetWorkspaceProjects(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<GetWorkspaceProjectsInput>(&input.request.body)?;
    let arena = Bump::new();
    let txn = db
        .graph_env
        .read_txn()
        .map_err(|e| GraphError::New(format!("Failed to start read transaction: {:?}", e)))?;
    let workspace = G::new(&db, &txn, &arena)
        .n_from_id(&data.workspace_id)
        .collect_to_obj()?;
    let projects = G::from_iter(&db, &txn, std::iter::once(workspace.clone()), &arena)
        .out_node("HasProject")
        .collect::<Result<Vec<_>, _>>()?;
    let response = json!({
        "projects": projects.iter().map(|project| Ok::<_, GraphError>(GetWorkspaceProjectsProjectsReturnType {
            name: project.get_property("name"),
            created_at: project.get_property("created_at"),
            label: project.label(),
            id: uuid_str(project.id(), &arena),
            updated_at: project.get_property("updated_at"),
            clusters: G::from_iter(&db, &txn, std::iter::once(project.clone()), &arena)

    .out_node("HasCluster")

    .count_to_val(),
        })).collect::<Result<Vec<_>, GraphError>>()?
    });
    txn.commit()
        .map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
    Ok(input.request.out_fmt.create_response(&response))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct DeleteWorkspaceInput {
    pub workspace_id: ID,
}
#[handler(is_write)]
pub fn DeleteWorkspace(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<DeleteWorkspaceInput>(&input.request.body)?;
    let arena = Bump::new();
    let mut txn = db
        .graph_env
        .write_txn()
        .map_err(|e| GraphError::New(format!("Failed to start write transaction: {:?}", e)))?;
    Drop::drop_traversal(
        G::new(&db, &txn, &arena)
            .n_from_id(&data.workspace_id)
            .out_node("HasProject")
            .out_node("HasCluster")
            .collect::<Vec<_>>()
            .into_iter(),
        &db,
        &mut txn,
    )?;
    Drop::drop_traversal(
        G::new(&db, &txn, &arena)
            .n_from_id(&data.workspace_id)
            .out_node("HasProject")
            .collect::<Vec<_>>()
            .into_iter(),
        &db,
        &mut txn,
    )?;
    Drop::drop_traversal(
        G::new(&db, &txn, &arena)
            .n_from_id(&data.workspace_id)
            .collect::<Vec<_>>()
            .into_iter(),
        &db,
        &mut txn,
    )?;
    txn.commit()
        .map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
    Ok(input.request.out_fmt.create_response(&()))
}
