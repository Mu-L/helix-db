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
            secondary_indices: Some(vec![
                SecondaryIndex::Index("github_id".to_string()),
                SecondaryIndex::Index("url_slug".to_string()),
                SecondaryIndex::Index("railway_project_id".to_string()),
            ]),
        }),
        db_max_size_gb: Some(10),
        mcp: Some(true),
        bm25: Some(true),
        schema: Some(
            r#"{
  "schema": {
    "nodes": [
      {
        "name": "Cluster",
        "properties": {
          "label": "String",
          "id": "ID",
          "build_mode": "String",
          "updated_at": "Date",
          "railway_project_id": "String",
          "created_at": "Date",
          "cluster_name": "String"
        }
      },
      {
        "name": "ApiKey",
        "properties": {
          "unkey_key_id": "String",
          "id": "ID",
          "label": "String",
          "created_at": "Date"
        }
      },
      {
        "name": "Project",
        "properties": {
          "id": "ID",
          "label": "String",
          "created_at": "Date",
          "updated_at": "Date",
          "name": "String"
        }
      },
      {
        "name": "RailwayInstance",
        "properties": {
          "updated_at": "Date",
          "ram_gb": "U64",
          "railway_service_id": "String",
          "railway_environment_id": "String",
          "created_at": "Date",
          "label": "String",
          "id": "ID",
          "cpu_cores": "U64"
        }
      },
      {
        "name": "Workspace",
        "properties": {
          "updated_at": "Date",
          "name": "String",
          "id": "ID",
          "url_slug": "String",
          "created_at": "Date",
          "workspace_type": "String",
          "icon": "String",
          "label": "String",
          "plan": "String"
        }
      },
      {
        "name": "User",
        "properties": {
          "label": "String",
          "github_id": "U64",
          "created_at": "Date",
          "updated_at": "Date",
          "id": "ID",
          "github_name": "String",
          "github_login": "String",
          "github_email": "String"
        }
      }
    ],
    "vectors": [],
    "edges": [
      {
        "name": "CreatedApiKey",
        "from": "User",
        "to": "ApiKey",
        "properties": {}
      },
      {
        "name": "MemberOf",
        "from": "User",
        "to": "Workspace",
        "properties": {
          "role": "String",
          "joined_at": "Date"
        }
      },
      {
        "name": "HasInstance",
        "from": "Cluster",
        "to": "RailwayInstance",
        "properties": {}
      },
      {
        "name": "HasProject",
        "from": "Workspace",
        "to": "Project",
        "properties": {}
      },
      {
        "name": "HasCluster",
        "from": "Project",
        "to": "Cluster",
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
      "name": "UpdateWorkspaceIcon",
      "parameters": {
        "workspace_id": "ID",
        "timestamp": "Date",
        "icon": "String"
      },
      "returns": [
        "workspace"
      ]
    },
    {
      "name": "IsNewWorkspace",
      "parameters": {
        "workspace_id": "ID"
      },
      "returns": [
        "is_new"
      ]
    },
    {
      "name": "UpdateWorkspaceMemberRole",
      "parameters": {
        "role": "String",
        "role_id": "ID"
      },
      "returns": []
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
      "name": "GetUserById",
      "parameters": {
        "user_id": "ID"
      },
      "returns": [
        "user"
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
      "name": "UpdateUsername",
      "parameters": {
        "github_name": "String",
        "user_id": "ID",
        "timestamp": "Date"
      },
      "returns": [
        "user"
      ]
    },
    {
      "name": "GetProject",
      "parameters": {
        "project_id": "ID"
      },
      "returns": []
    },
    {
      "name": "UpdateWorkspace",
      "parameters": {
        "icon": "String",
        "timestamp": "Date",
        "url_slug": "String",
        "workspace_id": "ID",
        "name": "String"
      },
      "returns": [
        "workspace"
      ]
    },
    {
      "name": "ChangeWorkspacePlan",
      "parameters": {
        "plan": "String",
        "workspace_id": "ID"
      },
      "returns": [
        "workspace"
      ]
    },
    {
      "name": "CreateProject",
      "parameters": {
        "name": "String",
        "workspace_id": "ID"
      },
      "returns": []
    },
    {
      "name": "CreateWorkspace",
      "parameters": {
        "name": "String",
        "plan": "String",
        "url_slug": "String",
        "user_id": "ID"
      },
      "returns": []
    },
    {
      "name": "CreateClustersInProject",
      "parameters": {
        "project_id": "ID",
        "clusters": "Array({build_mode: Stringcluster_name: Stringrailway_project_id: String})"
      },
      "returns": [
        "final_clusters"
      ]
    },
    {
      "name": "ExistsWorkspaceBySlug",
      "parameters": {
        "url_slug": "String"
      },
      "returns": [
        "exists"
      ]
    },
    {
      "name": "UserHasProjectAccess",
      "parameters": {
        "project_id": "ID",
        "user_id": "ID"
      },
      "returns": [
        "has_access"
      ]
    },
    {
      "name": "CreateWorkspaceOwner",
      "parameters": {
        "workspace_id": "ID",
        "user_id": "ID"
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
      "name": "DeleteWorkspace",
      "parameters": {
        "workspace_id": "ID"
      },
      "returns": []
    },
    {
      "name": "AddWorkspaceMember",
      "parameters": {
        "workspace_id": "ID",
        "role": "String",
        "user_id": "ID"
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
      "name": "GetWorkspaceMembers",
      "parameters": {
        "workspace_id": "ID"
      },
      "returns": []
    },
    {
      "name": "GetUserByGithubLogin",
      "parameters": {
        "github_login": "String"
      },
      "returns": [
        "user"
      ]
    },
    {
      "name": "GetUserByGithubEmail",
      "parameters": {
        "github_email": "String"
      },
      "returns": [
        "user"
      ]
    },
    {
      "name": "UrlSlugSearch",
      "parameters": {
        "url_slug": "String"
      },
      "returns": [
        "url_slug_exists"
      ]
    },
    {
      "name": "UpdateCluster",
      "parameters": {
        "timestamp": "Date",
        "build_mode": "String",
        "cluster_id": "ID",
        "cluster_name": "String"
      },
      "returns": [
        "cluster"
      ]
    },
    {
      "name": "GetUserWorkspaces",
      "parameters": {
        "user_id": "ID"
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
      "name": "DeleteProject",
      "parameters": {
        "project_id": "ID"
      },
      "returns": []
    },
    {
      "name": "UpdateProject",
      "parameters": {
        "name": "String",
        "timestamp": "Date",
        "project_id": "ID"
      },
      "returns": [
        "project"
      ]
    },
    {
      "name": "GetUserApiTokens",
      "parameters": {
        "user_id": "ID"
      },
      "returns": [
        "tokens"
      ]
    },
    {
      "name": "GetWorkspaceProjects",
      "parameters": {
        "workspace_id": "ID"
      },
      "returns": []
    },
    {
      "name": "GetAllUsers",
      "parameters": {},
      "returns": [
        "users"
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
      "name": "CreateUserGetUserId",
      "parameters": {
        "github_id": "U64",
        "github_login": "String",
        "github_name": "String",
        "github_email": "String",
        "github_avatar": "String"
      },
      "returns": []
    },
    {
      "name": "StoreApiKeyRef",
      "parameters": {
        "unkey_key_id": "String",
        "user_id": "ID"
      },
      "returns": []
    },
    {
      "name": "DeleteApiToken",
      "parameters": {
        "token_id": "ID"
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
    pub build_mode: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct RailwayInstance {
    pub railway_service_id: String,
    pub railway_environment_id: String,
    pub cpu_cores: u64,
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
    pub to: RailwayInstance,
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
    pub cluster_name: Option<&'a Value>,
    pub build_mode: Option<&'a Value>,
    pub created_at: Option<&'a Value>,
    pub updated_at: Option<&'a Value>,
    pub railway_project_id: Option<&'a Value>,
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
            cluster_name: cluster.get_property("cluster_name"),
            build_mode: cluster.get_property("build_mode"),
            created_at: cluster.get_property("created_at"),
            updated_at: cluster.get_property("updated_at"),
            railway_project_id: cluster.get_property("railway_project_id"),
        }).collect::<Vec<_>>()
    });
    txn.commit()
        .map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
    Ok(input.request.out_fmt.create_response(&response))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct UpdateWorkspaceIconInput {
    pub workspace_id: ID,
    pub icon: String,
    pub timestamp: DateTime<Utc>,
}
#[derive(Serialize)]
pub struct UpdateWorkspaceIconWorkspaceReturnType<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub name: Option<&'a Value>,
    pub icon: Option<&'a Value>,
    pub created_at: Option<&'a Value>,
    pub workspace_type: Option<&'a Value>,
    pub plan: Option<&'a Value>,
    pub url_slug: Option<&'a Value>,
    pub updated_at: Option<&'a Value>,
}

#[handler(is_write)]
pub fn UpdateWorkspaceIcon(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<UpdateWorkspaceIconInput>(&input.request.body)?;
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
                ("icon", Value::from(&data.icon)),
                ("updated_at", Value::from(&data.timestamp)),
            ])
            .collect_to_obj()?
    };
    let response = json!({
        "workspace": UpdateWorkspaceIconWorkspaceReturnType {
            id: uuid_str(workspace.id(), &arena),
            label: workspace.label(),
            name: workspace.get_property("name"),
            icon: workspace.get_property("icon"),
            created_at: workspace.get_property("created_at"),
            workspace_type: workspace.get_property("workspace_type"),
            plan: workspace.get_property("plan"),
            url_slug: workspace.get_property("url_slug"),
            updated_at: workspace.get_property("updated_at"),
        }
    });
    txn.commit()
        .map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
    Ok(input.request.out_fmt.create_response(&response))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct IsNewWorkspaceInput {
    pub workspace_id: ID,
}
#[handler]
pub fn IsNewWorkspace(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<IsNewWorkspaceInput>(&input.request.body)?;
    let arena = Bump::new();
    let txn = db
        .graph_env
        .read_txn()
        .map_err(|e| GraphError::New(format!("Failed to start read transaction: {:?}", e)))?;
    let is_new = Exist::exists(
        &mut G::new(&db, &txn, &arena)
            .n_from_id(&data.workspace_id)
            .in_e("MemberOf"),
    );
    let response = json!({
        "is_new": is_new
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
pub struct GetClusterInput {
    pub cluster_id: ID,
}
#[derive(Serialize)]
pub struct GetClusterClusterReturnType<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub cluster_name: Option<&'a Value>,
    pub build_mode: Option<&'a Value>,
    pub created_at: Option<&'a Value>,
    pub updated_at: Option<&'a Value>,
    pub railway_project_id: Option<&'a Value>,
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
            cluster_name: cluster.get_property("cluster_name"),
            build_mode: cluster.get_property("build_mode"),
            created_at: cluster.get_property("created_at"),
            updated_at: cluster.get_property("updated_at"),
            railway_project_id: cluster.get_property("railway_project_id"),
        }
    });
    txn.commit()
        .map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
    Ok(input.request.out_fmt.create_response(&response))
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
    pub github_email: Option<&'a Value>,
    pub updated_at: Option<&'a Value>,
    pub github_id: Option<&'a Value>,
    pub created_at: Option<&'a Value>,
    pub github_login: Option<&'a Value>,
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
            github_email: user.get_property("github_email"),
            updated_at: user.get_property("updated_at"),
            github_id: user.get_property("github_id"),
            created_at: user.get_property("created_at"),
            github_login: user.get_property("github_login"),
        }
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
    pub github_email: Option<&'a Value>,
    pub updated_at: Option<&'a Value>,
    pub github_id: Option<&'a Value>,
    pub created_at: Option<&'a Value>,
    pub github_login: Option<&'a Value>,
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
            github_email: user.get_property("github_email"),
            updated_at: user.get_property("updated_at"),
            github_id: user.get_property("github_id"),
            created_at: user.get_property("created_at"),
            github_login: user.get_property("github_login"),
        }
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
    pub name: Option<&'a Value>,
    pub clusters: Vec<TraversalValue<'a>>,
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
            name: project.get_property("name"),
            clusters: G::from_iter(&db, &txn, std::iter::once(project.clone()), &arena)

    .out_node("HasCluster").collect::<Result<Vec<_>, _>>()?,
        }
    });
    txn.commit()
        .map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
    Ok(input.request.out_fmt.create_response(&response))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct UpdateWorkspaceInput {
    pub workspace_id: ID,
    pub url_slug: String,
    pub name: String,
    pub icon: String,
    pub timestamp: DateTime<Utc>,
}
#[derive(Serialize)]
pub struct UpdateWorkspaceWorkspaceReturnType<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub name: Option<&'a Value>,
    pub icon: Option<&'a Value>,
    pub created_at: Option<&'a Value>,
    pub workspace_type: Option<&'a Value>,
    pub plan: Option<&'a Value>,
    pub url_slug: Option<&'a Value>,
    pub updated_at: Option<&'a Value>,
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
                ("url_slug", Value::from(&data.url_slug)),
                ("icon", Value::from(&data.icon)),
                ("updated_at", Value::from(&data.timestamp)),
            ])
            .collect_to_obj()?
    };
    let response = json!({
        "workspace": UpdateWorkspaceWorkspaceReturnType {
            id: uuid_str(workspace.id(), &arena),
            label: workspace.label(),
            name: workspace.get_property("name"),
            icon: workspace.get_property("icon"),
            created_at: workspace.get_property("created_at"),
            workspace_type: workspace.get_property("workspace_type"),
            plan: workspace.get_property("plan"),
            url_slug: workspace.get_property("url_slug"),
            updated_at: workspace.get_property("updated_at"),
        }
    });
    txn.commit()
        .map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
    Ok(input.request.out_fmt.create_response(&response))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ChangeWorkspacePlanInput {
    pub workspace_id: ID,
    pub plan: String,
}
#[derive(Serialize)]
pub struct ChangeWorkspacePlanWorkspaceReturnType<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub name: Option<&'a Value>,
    pub icon: Option<&'a Value>,
    pub created_at: Option<&'a Value>,
    pub workspace_type: Option<&'a Value>,
    pub plan: Option<&'a Value>,
    pub url_slug: Option<&'a Value>,
    pub updated_at: Option<&'a Value>,
}

#[handler(is_write)]
pub fn ChangeWorkspacePlan(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<ChangeWorkspacePlanInput>(&input.request.body)?;
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
            .update(&[("plan", Value::from(&data.plan))])
            .collect_to_obj()?
    };
    let response = json!({
        "workspace": ChangeWorkspacePlanWorkspaceReturnType {
            id: uuid_str(workspace.id(), &arena),
            label: workspace.label(),
            name: workspace.get_property("name"),
            icon: workspace.get_property("icon"),
            created_at: workspace.get_property("created_at"),
            workspace_type: workspace.get_property("workspace_type"),
            plan: workspace.get_property("plan"),
            url_slug: workspace.get_property("url_slug"),
            updated_at: workspace.get_property("updated_at"),
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
                    ("created_at", Value::from(chrono::Utc::now().to_rfc3339())),
                    ("updated_at", Value::from(chrono::Utc::now().to_rfc3339())),
                    ("name", Value::from(&data.name)),
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
pub struct CreateWorkspaceInput {
    pub user_id: ID,
    pub name: String,
    pub url_slug: String,
    pub plan: String,
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
    let workspace = G::new_mut(&db, &arena, &mut txn)
        .add_n(
            "Workspace",
            Some(ImmutablePropertiesMap::new(
                7,
                vec![
                    ("icon", Value::from("")),
                    ("created_at", Value::from(chrono::Utc::now().to_rfc3339())),
                    ("updated_at", Value::from(chrono::Utc::now().to_rfc3339())),
                    ("name", Value::from(&data.name)),
                    ("plan", Value::from(&data.plan)),
                    ("url_slug", Value::from(&data.url_slug)),
                    ("workspace_type", Value::from("organization")),
                ]
                .into_iter(),
                &arena,
            )),
            Some(&["url_slug"]),
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
pub struct CreateClustersInProjectInput {
    pub project_id: ID,
    pub clusters: Vec<CreateClustersInProjectClustersData>,
}
#[derive(Serialize, Deserialize, Clone)]
pub struct CreateClustersInProjectClustersData {
    pub build_mode: String,
    pub cluster_name: String,
    pub railway_project_id: String,
}
#[derive(Serialize)]
pub struct CreateClustersInProjectFinal_clustersReturnType<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub cluster_name: Option<&'a Value>,
    pub build_mode: Option<&'a Value>,
    pub created_at: Option<&'a Value>,
    pub updated_at: Option<&'a Value>,
    pub railway_project_id: Option<&'a Value>,
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
        build_mode,
    } in &data.clusters
    {
        let cluster = G::new_mut(&db, &arena, &mut txn)
            .add_n(
                "Cluster",
                Some(ImmutablePropertiesMap::new(
                    5,
                    vec![
                        ("cluster_name", Value::from(&cluster_name)),
                        ("build_mode", Value::from(&build_mode)),
                        ("created_at", Value::from(chrono::Utc::now().to_rfc3339())),
                        ("updated_at", Value::from(chrono::Utc::now().to_rfc3339())),
                        ("railway_project_id", Value::from(&railway_project_id)),
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
            cluster_name: final_cluster.get_property("cluster_name"),
            build_mode: final_cluster.get_property("build_mode"),
            created_at: final_cluster.get_property("created_at"),
            updated_at: final_cluster.get_property("updated_at"),
            railway_project_id: final_cluster.get_property("railway_project_id"),
        }).collect::<Vec<_>>()
    });
    txn.commit()
        .map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
    Ok(input.request.out_fmt.create_response(&response))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ExistsWorkspaceBySlugInput {
    pub url_slug: String,
}
#[handler]
pub fn ExistsWorkspaceBySlug(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<ExistsWorkspaceBySlugInput>(&input.request.body)?;
    let arena = Bump::new();
    let txn = db
        .graph_env
        .read_txn()
        .map_err(|e| GraphError::New(format!("Failed to start read transaction: {:?}", e)))?;
    let exists = Exist::exists(&mut G::new(&db, &txn, &arena).n_from_index(
        "Workspace",
        "url_slug",
        &data.url_slug,
    ));
    let response = json!({
        "exists": exists
    });
    txn.commit()
        .map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
    Ok(input.request.out_fmt.create_response(&response))
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
pub struct CreateWorkspaceOwnerInput {
    pub user_id: ID,
    pub workspace_id: ID,
}
#[handler(is_write)]
pub fn CreateWorkspaceOwner(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<CreateWorkspaceOwnerInput>(&input.request.body)?;
    let arena = Bump::new();
    let mut txn = db
        .graph_env
        .write_txn()
        .map_err(|e| GraphError::New(format!("Failed to start write transaction: {:?}", e)))?;
    let user = G::new(&db, &txn, &arena)
        .n_from_id(&data.user_id)
        .collect_to_obj()?;
    let workspace = G::new(&db, &txn, &arena)
        .n_from_id(&data.workspace_id)
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
    txn.commit()
        .map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
    Ok(input.request.out_fmt.create_response(&()))
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
pub struct GetWorkspaceMembersInput {
    pub workspace_id: ID,
}
#[derive(Serialize)]
pub struct GetWorkspaceMembersMembersReturnType<'a> {
    pub role_id: &'a str,
    pub user: TraversalValue<'a>,
    pub role: Option<&'a Value>,
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
            role_id: uuid_str(member.id(), &arena),
            user: G::from_iter(&db, &txn, std::iter::once(member.clone()), &arena)

    .from_n().collect_to_obj()?,
            role: member.get_property("role"),
        })).collect::<Result<Vec<_>, GraphError>>()?
    });
    txn.commit()
        .map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
    Ok(input.request.out_fmt.create_response(&response))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct GetUserByGithubLoginInput {
    pub github_login: String,
}
#[derive(Serialize)]
pub struct GetUserByGithubLoginUserReturnType<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub github_name: Option<&'a Value>,
    pub github_email: Option<&'a Value>,
    pub updated_at: Option<&'a Value>,
    pub github_id: Option<&'a Value>,
    pub created_at: Option<&'a Value>,
    pub github_login: Option<&'a Value>,
}

#[handler]
pub fn GetUserByGithubLogin(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<GetUserByGithubLoginInput>(&input.request.body)?;
    let arena = Bump::new();
    let txn = db
        .graph_env
        .read_txn()
        .map_err(|e| GraphError::New(format!("Failed to start read transaction: {:?}", e)))?;
    let user = G::new(&db, &txn, &arena)
        .n_from_type("User")
        .filter_ref(|val, txn| {
            if let Ok(val) = val {
                Ok(val
                    .get_property("github_login")
                    .map_or(false, |v| *v == data.github_login.clone()))
            } else {
                Ok(false)
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    let response = json!({
        "user": user.iter().map(|user| GetUserByGithubLoginUserReturnType {
            id: uuid_str(user.id(), &arena),
            label: user.label(),
            github_name: user.get_property("github_name"),
            github_email: user.get_property("github_email"),
            updated_at: user.get_property("updated_at"),
            github_id: user.get_property("github_id"),
            created_at: user.get_property("created_at"),
            github_login: user.get_property("github_login"),
        }).collect::<Vec<_>>()
    });
    txn.commit()
        .map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
    Ok(input.request.out_fmt.create_response(&response))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct GetUserByGithubEmailInput {
    pub github_email: String,
}
#[derive(Serialize)]
pub struct GetUserByGithubEmailUserReturnType<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub github_name: Option<&'a Value>,
    pub github_email: Option<&'a Value>,
    pub updated_at: Option<&'a Value>,
    pub github_id: Option<&'a Value>,
    pub created_at: Option<&'a Value>,
    pub github_login: Option<&'a Value>,
}

#[handler]
pub fn GetUserByGithubEmail(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<GetUserByGithubEmailInput>(&input.request.body)?;
    let arena = Bump::new();
    let txn = db
        .graph_env
        .read_txn()
        .map_err(|e| GraphError::New(format!("Failed to start read transaction: {:?}", e)))?;
    let user = G::new(&db, &txn, &arena)
        .n_from_type("User")
        .filter_ref(|val, txn| {
            if let Ok(val) = val {
                Ok(val
                    .get_property("github_email")
                    .map_or(false, |v| *v == data.github_email.clone()))
            } else {
                Ok(false)
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    let response = json!({
        "user": user.iter().map(|user| GetUserByGithubEmailUserReturnType {
            id: uuid_str(user.id(), &arena),
            label: user.label(),
            github_name: user.get_property("github_name"),
            github_email: user.get_property("github_email"),
            updated_at: user.get_property("updated_at"),
            github_id: user.get_property("github_id"),
            created_at: user.get_property("created_at"),
            github_login: user.get_property("github_login"),
        }).collect::<Vec<_>>()
    });
    txn.commit()
        .map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
    Ok(input.request.out_fmt.create_response(&response))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct UrlSlugSearchInput {
    pub url_slug: String,
}
#[handler]
pub fn UrlSlugSearch(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<UrlSlugSearchInput>(&input.request.body)?;
    let arena = Bump::new();
    let txn = db
        .graph_env
        .read_txn()
        .map_err(|e| GraphError::New(format!("Failed to start read transaction: {:?}", e)))?;
    let url_slug_exists = Exist::exists(
        &mut G::new(&db, &txn, &arena)
            .n_from_type("Workspace")
            .filter_ref(|val, txn| {
                if let Ok(val) = val {
                    Ok(val
                        .get_property("url_slug")
                        .map_or(false, |v| *v == data.url_slug.clone()))
                } else {
                    Ok(false)
                }
            }),
    );
    let response = json!({
        "url_slug_exists": url_slug_exists
    });
    txn.commit()
        .map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
    Ok(input.request.out_fmt.create_response(&response))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct UpdateClusterInput {
    pub cluster_id: ID,
    pub cluster_name: String,
    pub build_mode: String,
    pub timestamp: DateTime<Utc>,
}
#[derive(Serialize)]
pub struct UpdateClusterClusterReturnType<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub cluster_name: Option<&'a Value>,
    pub build_mode: Option<&'a Value>,
    pub created_at: Option<&'a Value>,
    pub updated_at: Option<&'a Value>,
    pub railway_project_id: Option<&'a Value>,
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
                ("cluster_name", Value::from(&data.cluster_name)),
                ("build_mode", Value::from(&data.build_mode)),
                ("updated_at", Value::from(&data.timestamp)),
            ])
            .collect_to_obj()?
    };
    let response = json!({
        "cluster": UpdateClusterClusterReturnType {
            id: uuid_str(cluster.id(), &arena),
            label: cluster.label(),
            cluster_name: cluster.get_property("cluster_name"),
            build_mode: cluster.get_property("build_mode"),
            created_at: cluster.get_property("created_at"),
            updated_at: cluster.get_property("updated_at"),
            railway_project_id: cluster.get_property("railway_project_id"),
        }
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
pub struct GetWorkspaceInput {
    pub workspace_id: ID,
}
#[derive(Serialize)]
pub struct GetWorkspaceWorkspaceReturnType<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub name: Option<&'a Value>,
    pub icon: Option<&'a Value>,
    pub created_at: Option<&'a Value>,
    pub workspace_type: Option<&'a Value>,
    pub plan: Option<&'a Value>,
    pub url_slug: Option<&'a Value>,
    pub updated_at: Option<&'a Value>,
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
            name: workspace.get_property("name"),
            icon: workspace.get_property("icon"),
            created_at: workspace.get_property("created_at"),
            workspace_type: workspace.get_property("workspace_type"),
            plan: workspace.get_property("plan"),
            url_slug: workspace.get_property("url_slug"),
            updated_at: workspace.get_property("updated_at"),
        }
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
pub struct UpdateProjectInput {
    pub project_id: ID,
    pub name: String,
    pub timestamp: DateTime<Utc>,
}
#[derive(Serialize)]
pub struct UpdateProjectProjectReturnType<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub created_at: Option<&'a Value>,
    pub updated_at: Option<&'a Value>,
    pub name: Option<&'a Value>,
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
            created_at: project.get_property("created_at"),
            updated_at: project.get_property("updated_at"),
            name: project.get_property("name"),
        }
    });
    txn.commit()
        .map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
    Ok(input.request.out_fmt.create_response(&response))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct GetUserApiTokensInput {
    pub user_id: ID,
}
#[derive(Serialize)]
pub struct GetUserApiTokensTokensReturnType<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub unkey_key_id: Option<&'a Value>,
    pub created_at: Option<&'a Value>,
}

#[handler]
pub fn GetUserApiTokens(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<GetUserApiTokensInput>(&input.request.body)?;
    let arena = Bump::new();
    let txn = db
        .graph_env
        .read_txn()
        .map_err(|e| GraphError::New(format!("Failed to start read transaction: {:?}", e)))?;
    let user = G::new(&db, &txn, &arena)
        .n_from_id(&data.user_id)
        .collect_to_obj()?;
    let tokens = G::from_iter(&db, &txn, std::iter::once(user.clone()), &arena)
        .out_node("CreatedApiKey")
        .collect::<Result<Vec<_>, _>>()?;
    let response = json!({
        "tokens": tokens.iter().map(|token| GetUserApiTokensTokensReturnType {
            id: uuid_str(token.id(), &arena),
            label: token.label(),
            unkey_key_id: token.get_property("unkey_key_id"),
            created_at: token.get_property("created_at"),
        }).collect::<Vec<_>>()
    });
    txn.commit()
        .map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
    Ok(input.request.out_fmt.create_response(&response))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct GetWorkspaceProjectsInput {
    pub workspace_id: ID,
}
#[derive(Serialize)]
pub struct GetWorkspaceProjectsProjectsReturnType<'a> {
    pub created_at: Option<&'a Value>,
    pub updated_at: Option<&'a Value>,
    pub name: Option<&'a Value>,
    pub id: &'a str,
    pub label: &'a str,
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
            created_at: project.get_property("created_at"),
            updated_at: project.get_property("updated_at"),
            name: project.get_property("name"),
            id: uuid_str(project.id(), &arena),
            label: project.label(),
            clusters: G::from_iter(&db, &txn, std::iter::once(project.clone()), &arena)

    .out_node("HasCluster")

    .count_to_val(),
        })).collect::<Result<Vec<_>, GraphError>>()?
    });
    txn.commit()
        .map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
    Ok(input.request.out_fmt.create_response(&response))
}

#[derive(Serialize)]
pub struct GetAllUsersUsersReturnType<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub github_name: Option<&'a Value>,
    pub github_email: Option<&'a Value>,
    pub updated_at: Option<&'a Value>,
    pub github_id: Option<&'a Value>,
    pub created_at: Option<&'a Value>,
    pub github_login: Option<&'a Value>,
}

#[handler]
pub fn GetAllUsers(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let arena = Bump::new();
    let txn = db
        .graph_env
        .read_txn()
        .map_err(|e| GraphError::New(format!("Failed to start read transaction: {:?}", e)))?;
    let users = G::new(&db, &txn, &arena)
        .n_from_type("User")
        .collect::<Result<Vec<_>, _>>()?;
    let response = json!({
        "users": users.iter().map(|user| GetAllUsersUsersReturnType {
            id: uuid_str(user.id(), &arena),
            label: user.label(),
            github_name: user.get_property("github_name"),
            github_email: user.get_property("github_email"),
            updated_at: user.get_property("updated_at"),
            github_id: user.get_property("github_id"),
            created_at: user.get_property("created_at"),
            github_login: user.get_property("github_login"),
        }).collect::<Vec<_>>()
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
pub struct CreateUserGetUserIdInput {
    pub github_id: u64,
    pub github_login: String,
    pub github_name: String,
    pub github_email: String,
    pub github_avatar: String,
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
                    ("github_name", Value::from(&data.github_name)),
                    ("github_email", Value::from(&data.github_email)),
                    ("updated_at", Value::from(chrono::Utc::now().to_rfc3339())),
                    ("github_id", Value::from(&data.github_id)),
                    ("github_login", Value::from(&data.github_login)),
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
                    ("name", Value::from("Personal")),
                    ("created_at", Value::from(chrono::Utc::now().to_rfc3339())),
                    ("url_slug", Value::from(&data.github_login)),
                    ("plan", Value::from("none")),
                    ("icon", Value::from(&data.github_avatar)),
                    ("workspace_type", Value::from("personal")),
                    ("updated_at", Value::from(chrono::Utc::now().to_rfc3339())),
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
        "user": uuid_str(user.id(), &arena)
    });
    txn.commit()
        .map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
    Ok(input.request.out_fmt.create_response(&response))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct StoreApiKeyRefInput {
    pub user_id: ID,
    pub unkey_key_id: String,
}
#[handler(is_write)]
pub fn StoreApiKeyRef(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<StoreApiKeyRefInput>(&input.request.body)?;
    let arena = Bump::new();
    let mut txn = db
        .graph_env
        .write_txn()
        .map_err(|e| GraphError::New(format!("Failed to start write transaction: {:?}", e)))?;
    let user = G::new(&db, &txn, &arena)
        .n_from_id(&data.user_id)
        .collect_to_obj()?;
    let api_key = G::new_mut(&db, &arena, &mut txn)
        .add_n(
            "ApiKey",
            Some(ImmutablePropertiesMap::new(
                2,
                vec![
                    ("created_at", Value::from(chrono::Utc::now().to_rfc3339())),
                    ("unkey_key_id", Value::from(&data.unkey_key_id)),
                ]
                .into_iter(),
                &arena,
            )),
            None,
        )
        .collect_to_obj()?;
    G::new_mut(&db, &arena, &mut txn)
        .add_edge("CreatedApiKey", None, user.id(), api_key.id(), false, false)
        .collect_to_obj()?;
    txn.commit()
        .map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
    Ok(input.request.out_fmt.create_response(&()))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct DeleteApiTokenInput {
    pub token_id: ID,
}
#[handler(is_write)]
pub fn DeleteApiToken(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<DeleteApiTokenInput>(&input.request.body)?;
    let arena = Bump::new();
    let mut txn = db
        .graph_env
        .write_txn()
        .map_err(|e| GraphError::New(format!("Failed to start write transaction: {:?}", e)))?;
    Drop::drop_traversal(
        G::new(&db, &txn, &arena)
            .n_from_id(&data.token_id)
            .collect::<Vec<_>>()
            .into_iter(),
        &db,
        &mut txn,
    )?;
    txn.commit()
        .map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
    Ok(input.request.out_fmt.create_response(&()))
}
