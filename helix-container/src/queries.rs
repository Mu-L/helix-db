// DEFAULT CODE
// use helix_db::helix_engine::traversal_core::config::Config;

// pub fn config() -> Option<Config> {
//     None
// }

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
                    filter_ref::FilterRefAdapter, map::MapAdapter, order::OrderByAdapter,
                    paths::ShortestPathAdapter, props::PropsAdapter, range::RangeAdapter,
                    update::UpdateAdapter,
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
        items::{Edge, Node},
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
        "name": "Branch",
        "properties": {
          "name": "String",
          "id": "ID"
        }
      },
      {
        "name": "Page",
        "properties": {
          "id": "ID",
          "name": "String"
        }
      },
      {
        "name": "Element",
        "properties": {
          "element_id": "String",
          "id": "ID",
          "name": "String"
        }
      },
      {
        "name": "PageFolder",
        "properties": {
          "name": "String",
          "id": "ID"
        }
      },
      {
        "name": "User",
        "properties": {
          "email": "String",
          "id": "ID",
          "password": "String",
          "name": "String"
        }
      },
      {
        "name": "Frontend",
        "properties": {
          "id": "ID"
        }
      },
      {
        "name": "App",
        "properties": {
          "id": "ID",
          "created_at": "Date",
          "name": "String",
          "description": "String"
        }
      },
      {
        "name": "Backend",
        "properties": {
          "id": "ID"
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
          "assigned_at": "Date",
          "created_at": "Date"
        }
      },
      {
        "name": "Page_Has_Root_Element",
        "from": "Page",
        "to": "Element",
        "properties": {
          "assigned_at": "Date",
          "created_at": "Date"
        }
      },
      {
        "name": "User_Has_App",
        "from": "User",
        "to": "App",
        "properties": {
          "created_at": "Date"
        }
      },
      {
        "name": "Branch_Has_Frontend",
        "from": "Branch",
        "to": "Frontend",
        "properties": {
          "created_at": "Date"
        }
      },
      {
        "name": "Branch_Has_Backend",
        "from": "Branch",
        "to": "Backend",
        "properties": {
          "created_at": "Date"
        }
      },
      {
        "name": "Frontend_Contains_PageFolder",
        "from": "Frontend",
        "to": "PageFolder",
        "properties": {
          "assigned_at": "Date",
          "created_at": "Date"
        }
      },
      {
        "name": "App_Has_Branch",
        "from": "App",
        "to": "Branch",
        "properties": {
          "created_at": "Date"
        }
      },
      {
        "name": "Frontend_Has_Page",
        "from": "Frontend",
        "to": "Page",
        "properties": {
          "assigned_at": "Date",
          "created_at": "Date"
        }
      },
      {
        "name": "PageFolder_Contains_Page",
        "from": "PageFolder",
        "to": "Page",
        "properties": {
          "created_at": "Date",
          "assigned_at": "Date"
        }
      }
    ]
  },
  "queries": [
    {
      "name": "CreateFullAppWithPages",
      "parameters": {
        "user_id": "ID",
        "app_name": "String",
        "app_description": "String",
        "created_at": "Date"
      },
      "returns": []
    }
  ]
}"#
            .to_string(),
        ),
        embedding_model: Some("text-embedding-ada-002".to_string()),
        graphvis_node_label: Some("".to_string()),
    });
}

pub struct User {
    pub name: String,
    pub email: String,
    pub password: String,
}

pub struct App {
    pub name: String,
    pub description: String,
    pub created_at: DateTime<Utc>,
}

pub struct Branch {
    pub name: String,
}

pub struct Frontend {}

pub struct Backend {}

pub struct Element {
    pub element_id: String,
    pub name: String,
}

pub struct PageFolder {
    pub name: String,
}

pub struct Page {
    pub name: String,
}

pub struct User_Has_App {
    pub from: User,
    pub to: App,
    pub created_at: DateTime<Utc>,
}

pub struct App_Has_Branch {
    pub from: App,
    pub to: Branch,
    pub created_at: DateTime<Utc>,
}

pub struct Branch_Has_Frontend {
    pub from: Branch,
    pub to: Frontend,
    pub created_at: DateTime<Utc>,
}

pub struct Branch_Has_Backend {
    pub from: Branch,
    pub to: Backend,
    pub created_at: DateTime<Utc>,
}

pub struct Frontend_Contains_PageFolder {
    pub from: Frontend,
    pub to: PageFolder,
    pub created_at: DateTime<Utc>,
    pub assigned_at: DateTime<Utc>,
}

pub struct Page_Has_Root_Element {
    pub from: Page,
    pub to: Element,
    pub created_at: DateTime<Utc>,
    pub assigned_at: DateTime<Utc>,
}

pub struct Frontend_Has_Page {
    pub from: Frontend,
    pub to: Page,
    pub created_at: DateTime<Utc>,
    pub assigned_at: DateTime<Utc>,
}

pub struct PageFolder_Contains_Page {
    pub from: PageFolder,
    pub to: Page,
    pub created_at: DateTime<Utc>,
    pub assigned_at: DateTime<Utc>,
}

pub struct User_Has_Access_To {
    pub from: User,
    pub to: App,
    pub created_at: DateTime<Utc>,
    pub assigned_at: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct CreateFullAppWithPagesInput {
    pub user_id: ID,
    pub app_name: String,
    pub app_description: String,
    pub created_at: DateTime<Utc>,
}
#[handler]
pub fn CreateFullAppWithPages(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<CreateFullAppWithPagesInput>(&input.request.body)?;
    let mut remapping_vals = RemappingMap::new();
    let mut txn = db.graph_env.write_txn().unwrap();
    let user = G::new(Arc::clone(&db), &txn)
        .n_from_id(&data.user_id)
        .collect_to_obj();
    let app = G::new_mut(Arc::clone(&db), &mut txn)
.add_n("App", Some(props! { "name" => &data.app_name, "description" => &data.app_description, "created_at" => &data.created_at }), None).collect_to_obj();
    let dev_branch = G::new_mut(Arc::clone(&db), &mut txn)
        .add_n("Branch", Some(props! { "name" => "Development" }), None)
        .collect_to_obj();
    let staging_branch = G::new_mut(Arc::clone(&db), &mut txn)
        .add_n("Branch", Some(props! { "name" => "Staging" }), None)
        .collect_to_obj();
    let frontend_dev = G::new_mut(Arc::clone(&db), &mut txn)
        .add_n("Frontend", Some(props! {}), None)
        .collect_to_obj();
    let backend_dev = G::new_mut(Arc::clone(&db), &mut txn)
        .add_n("Backend", Some(props! {}), None)
        .collect_to_obj();
    let frontend_staging = G::new_mut(Arc::clone(&db), &mut txn)
        .add_n("Frontend", Some(props! {}), None)
        .collect_to_obj();
    let backend_staging = G::new_mut(Arc::clone(&db), &mut txn)
        .add_n("Backend", Some(props! {}), None)
        .collect_to_obj();
    let root_element = G::new_mut(Arc::clone(&db), &mut txn)
        .add_n(
            "Element",
            Some(props! { "element_id" => "root_element", "name" => "root_element" }),
            None,
        )
        .collect_to_obj();
    let root_element_404 = G::new_mut(Arc::clone(&db), &mut txn)
        .add_n(
            "Element",
            Some(props! { "name" => "root_element", "element_id" => "root_element" }),
            None,
        )
        .collect_to_obj();
    let root_element_reset = G::new_mut(Arc::clone(&db), &mut txn)
        .add_n(
            "Element",
            Some(props! { "element_id" => "root_element", "name" => "root_element" }),
            None,
        )
        .collect_to_obj();
    let index_page = G::new_mut(Arc::clone(&db), &mut txn)
        .add_n("Page", Some(props! { "name" => "index" }), None)
        .collect_to_obj();
    let not_found_page = G::new_mut(Arc::clone(&db), &mut txn)
        .add_n("Page", Some(props! { "name" => "Page not found" }), None)
        .collect_to_obj();
    let reset_password_page = G::new_mut(Arc::clone(&db), &mut txn)
        .add_n("Page", Some(props! { "name" => "Reset Password" }), None)
        .collect_to_obj();
    let main_folder = G::new_mut(Arc::clone(&db), &mut txn)
        .add_n("PageFolder", Some(props! { "name" => "Unsorted" }), None)
        .collect_to_obj();
    let user_app_edge = G::new_mut(Arc::clone(&db), &mut txn)
        .add_e(
            "User_Has_Access_To",
            Some(props! { "assigned_at" => data.created_at.clone() }),
            user.id(),
            app.id(),
            true,
            EdgeType::Node,
        )
        .collect_to_obj();
    let app_dev_branch_edge = G::new_mut(Arc::clone(&db), &mut txn)
        .add_e(
            "App_Has_Branch",
            Some(props! { "created_at" => data.created_at.clone() }),
            app.id(),
            dev_branch.id(),
            true,
            EdgeType::Node,
        )
        .collect_to_obj();
    let app_staging_branch_edge = G::new_mut(Arc::clone(&db), &mut txn)
        .add_e(
            "App_Has_Branch",
            Some(props! { "created_at" => data.created_at.clone() }),
            app.id(),
            staging_branch.id(),
            true,
            EdgeType::Node,
        )
        .collect_to_obj();
    let dev_branch_frontend_edge = G::new_mut(Arc::clone(&db), &mut txn)
        .add_e(
            "Branch_Has_Frontend",
            Some(props! { "created_at" => data.created_at.clone() }),
            dev_branch.id(),
            frontend_dev.id(),
            true,
            EdgeType::Node,
        )
        .collect_to_obj();
    let dev_branch_backend_edge = G::new_mut(Arc::clone(&db), &mut txn)
        .add_e(
            "Branch_Has_Backend",
            Some(props! { "created_at" => data.created_at.clone() }),
            dev_branch.id(),
            backend_dev.id(),
            true,
            EdgeType::Node,
        )
        .collect_to_obj();
    let staging_branch_frontend_edge = G::new_mut(Arc::clone(&db), &mut txn)
        .add_e(
            "Branch_Has_Frontend",
            Some(props! { "created_at" => data.created_at.clone() }),
            staging_branch.id(),
            frontend_staging.id(),
            true,
            EdgeType::Node,
        )
        .collect_to_obj();
    let staging_branch_backend_edge = G::new_mut(Arc::clone(&db), &mut txn)
        .add_e(
            "Branch_Has_Backend",
            Some(props! { "created_at" => data.created_at.clone() }),
            staging_branch.id(),
            backend_staging.id(),
            true,
            EdgeType::Node,
        )
        .collect_to_obj();
    let index_page_element_edge = G::new_mut(Arc::clone(&db), &mut txn)
        .add_e(
            "Page_Has_Root_Element",
            Some(props! { "assigned_at" => data.created_at.clone() }),
            index_page.id(),
            root_element.id(),
            true,
            EdgeType::Node,
        )
        .collect_to_obj();
    let not_found_page_element_edge = G::new_mut(Arc::clone(&db), &mut txn)
        .add_e(
            "Page_Has_Root_Element",
            Some(props! { "assigned_at" => data.created_at.clone() }),
            not_found_page.id(),
            root_element_404.id(),
            true,
            EdgeType::Node,
        )
        .collect_to_obj();
    let reset_page_element_edge = G::new_mut(Arc::clone(&db), &mut txn)
        .add_e(
            "Page_Has_Root_Element",
            Some(props! { "assigned_at" => data.created_at.clone() }),
            reset_password_page.id(),
            root_element_reset.id(),
            true,
            EdgeType::Node,
        )
        .collect_to_obj();
    let folder_index_edge = G::new_mut(Arc::clone(&db), &mut txn)
        .add_e(
            "PageFolder_Contains_Page",
            Some(props! { "assigned_at" => data.created_at.clone() }),
            main_folder.id(),
            index_page.id(),
            true,
            EdgeType::Node,
        )
        .collect_to_obj();
    let folder_404_edge = G::new_mut(Arc::clone(&db), &mut txn)
        .add_e(
            "PageFolder_Contains_Page",
            Some(props! { "assigned_at" => data.created_at.clone() }),
            main_folder.id(),
            not_found_page.id(),
            true,
            EdgeType::Node,
        )
        .collect_to_obj();
    let folder_reset_edge = G::new_mut(Arc::clone(&db), &mut txn)
        .add_e(
            "PageFolder_Contains_Page",
            Some(props! { "assigned_at" => data.created_at.clone() }),
            main_folder.id(),
            reset_password_page.id(),
            true,
            EdgeType::Node,
        )
        .collect_to_obj();
    let frontend_index_edge = G::new_mut(Arc::clone(&db), &mut txn)
        .add_e(
            "Frontend_Has_Page",
            Some(props! { "assigned_at" => data.created_at.clone() }),
            frontend_dev.id(),
            index_page.id(),
            true,
            EdgeType::Node,
        )
        .collect_to_obj();
    let frontend_404_edge = G::new_mut(Arc::clone(&db), &mut txn)
        .add_e(
            "Frontend_Has_Page",
            Some(props! { "assigned_at" => data.created_at.clone() }),
            frontend_dev.id(),
            not_found_page.id(),
            true,
            EdgeType::Node,
        )
        .collect_to_obj();
    let frontend_reset_edge = G::new_mut(Arc::clone(&db), &mut txn)
        .add_e(
            "Frontend_Has_Page",
            Some(props! { "assigned_at" => data.created_at.clone() }),
            frontend_dev.id(),
            reset_password_page.id(),
            true,
            EdgeType::Node,
        )
        .collect_to_obj();
    let frontend_folder_edge = G::new_mut(Arc::clone(&db), &mut txn)
        .add_e(
            "Frontend_Contains_PageFolder",
            Some(props! { "assigned_at" => data.created_at.clone() }),
            frontend_dev.id(),
            main_folder.id(),
            true,
            EdgeType::Node,
        )
        .collect_to_obj();
    let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
    return_vals.insert(
        "data".to_string(),
        ReturnValue::from(HashMap::from([(
            String::from("app"),
            ReturnValue::from(HashMap::from([
                (
                    String::from("branches"),
                    ReturnValue::from(vec![
                        ReturnValue::from(HashMap::from([
                            (
                                String::from("frontend"),
                                ReturnValue::from(HashMap::from([(
                                    String::from("page_folders"),
                                    ReturnValue::from(vec![ReturnValue::from(HashMap::from([
                                        (
                                            String::from("pages"),
                                            ReturnValue::from(vec![
                                                ReturnValue::from(index_page.clone()),
                                                ReturnValue::from(not_found_page.clone()),
                                                ReturnValue::from(reset_password_page.clone()),
                                            ]),
                                        ),
                                        (
                                            String::from("name"),
                                            ReturnValue::from(
                                                G::new_from(
                                                    Arc::clone(&db),
                                                    &txn,
                                                    main_folder.clone(),
                                                )
                                                .check_property("name")
                                                .collect_to_obj(),
                                            ),
                                        ),
                                    ]))]),
                                )])),
                            ),
                            (
                                String::from("name"),
                                ReturnValue::from(
                                    G::new_from(Arc::clone(&db), &txn, dev_branch.clone())
                                        .check_property("name")
                                        .collect_to_obj(),
                                ),
                            ),
                            (
                                String::from("backend"),
                                ReturnValue::from(backend_dev.clone()),
                            ),
                        ])),
                        ReturnValue::from(HashMap::from([
                            (
                                String::from("backend"),
                                ReturnValue::from(backend_staging.clone()),
                            ),
                            (
                                String::from("frontend"),
                                ReturnValue::from(frontend_staging.clone()),
                            ),
                            (
                                String::from("name"),
                                ReturnValue::from(
                                    G::new_from(Arc::clone(&db), &txn, staging_branch.clone())
                                        .check_property("name")
                                        .collect_to_obj(),
                                ),
                            ),
                        ])),
                    ]),
                ),
                (
                    String::from("id"),
                    ReturnValue::from(
                        G::new_from(Arc::clone(&db), &txn, app.clone())
                            .check_property("id")
                            .collect_to_obj(),
                    ),
                ),
                (
                    String::from("description"),
                    ReturnValue::from(
                        G::new_from(Arc::clone(&db), &txn, app.clone())
                            .check_property("description")
                            .collect_to_obj(),
                    ),
                ),
                (
                    String::from("name"),
                    ReturnValue::from(
                        G::new_from(Arc::clone(&db), &txn, app.clone())
                            .check_property("name")
                            .collect_to_obj(),
                    ),
                ),
            ])),
        )])),
    );

    txn.commit().unwrap();
    Ok(input.request.out_fmt.create_response(&return_vals))
}
