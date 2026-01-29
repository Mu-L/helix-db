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
secondary_indices: Some(vec![SecondaryIndex::Index("company_number".to_string())]),
}),
db_max_size_gb: Some(10),
mcp: Some(true),
bm25: Some(true),
schema: Some(r#"{
  "schema": {
    "nodes": [
      {
        "name": "Company",
        "properties": {
          "total_filings": "I32",
          "ingested_filings": "I32",
          "company_number": "String",
          "label": "String",
          "id": "ID"
        }
      }
    ],
    "vectors": [
      {
        "name": "DocumentEmbedding",
        "properties": {
          "source_link": "String",
          "page_number": "U16",
          "score": "F64",
          "label": "String",
          "source_date": "String",
          "reference": "String",
          "chunk_id": "String",
          "text": "String",
          "id": "ID",
          "data": "Array(F64)"
        }
      }
    ],
    "edges": [
      {
        "name": "DocumentEdge",
        "from": "Company",
        "to": "DocumentEmbedding",
        "properties": {
          "description": "String",
          "category": "String",
          "filing_id": "String",
          "subcategory": "String",
          "date": "String"
        }
      }
    ]
  },
  "queries": [
    {
      "name": "AddEmbeddingsToCompany",
      "parameters": {
        "company_number": "String",
        "embeddings_data": "Array({subcategory: Stringdate2: Stringsource: Stringdate1: Stringdescription: Stringvector: Array(F64)page_number: I32reference: Stringfiling_id: Stringcategory: Stringtext: Stringchunk_id: String})"
      },
      "returns": []
    },
    {
      "name": "GetCompanies",
      "parameters": {},
      "returns": [
        "companies"
      ]
    },
    {
      "name": "UpdateCompany",
      "parameters": {
        "company_number": "String",
        "ingested_filings": "I32"
      },
      "returns": [
        "company"
      ]
    },
    {
      "name": "AddCompany",
      "parameters": {
        "company_number": "String",
        "total_filings": "I32"
      },
      "returns": [
        "company"
      ]
    },
    {
      "name": "GetDocumentEdges",
      "parameters": {
        "company_number": "String"
      },
      "returns": []
    },
    {
      "name": "DeleteCompany",
      "parameters": {
        "company_number": "String"
      },
      "returns": []
    },
    {
      "name": "CompanyEmbeddingSearch",
      "parameters": {
        "company_number": "String",
        "k": "I32",
        "query": "Array(F64)"
      },
      "returns": [
        "embedding_search"
      ]
    },
    {
      "name": "SearchVector",
      "parameters": {
        "query": "Array(F64)",
        "k": "I32"
      },
      "returns": [
        "embedding_search"
      ]
    },
    {
      "name": "GetCompany",
      "parameters": {
        "company_number": "String"
      },
      "returns": [
        "company"
      ]
    },
    {
      "name": "AddVector",
      "parameters": {
        "text": "String",
        "chunk_id": "String",
        "vector": "Array(F64)",
        "page_number": "I32",
        "reference": "String"
      },
      "returns": [
        "embedding"
      ]
    },
    {
      "name": "GetAllCompanyEmbeddings",
      "parameters": {
        "company_number": "String"
      },
      "returns": [
        "embeddings"
      ]
    }
  ]
}"#.to_string()),
embedding_model: Some("text-embedding-ada-002".to_string()),
graphvis_node_label: None,
});
}
pub struct Company {
    pub company_number: String,
    pub total_filings: i32,
    pub ingested_filings: i32,
}

pub struct DocumentEdge {
    pub from: Company,
    pub to: DocumentEmbedding,
    pub filing_id: String,
    pub category: String,
    pub subcategory: String,
    pub date: String,
    pub description: String,
}

pub struct DocumentEmbedding {
    pub text: String,
    pub chunk_id: String,
    pub page_number: u16,
    pub reference: String,
    pub source_link: String,
    pub source_date: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct AddEmbeddingsToCompanyInput {
    pub company_number: String,
    pub embeddings_data: Vec<AddEmbeddingsToCompanyEmbeddings_dataData>,
}
#[derive(Serialize, Deserialize, Clone)]
pub struct AddEmbeddingsToCompanyEmbeddings_dataData {
    pub subcategory: String,
    pub date2: String,
    pub source: String,
    pub date1: String,
    pub description: String,
    pub vector: Vec<f64>,
    pub page_number: i32,
    pub reference: String,
    pub filing_id: String,
    pub category: String,
    pub text: String,
    pub chunk_id: String,
}
#[handler(is_write)]
pub fn AddEmbeddingsToCompany(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<AddEmbeddingsToCompanyInput>(&input.request.body)?;
    let arena = Bump::new();
    let mut txn = db
        .graph_env
        .write_txn()
        .map_err(|e| GraphError::New(format!("Failed to start write transaction: {:?}", e)))?;
    let c = G::new(&db, &txn, &arena)
        .n_from_index("Company", "company_number", &data.company_number)
        .collect_to_obj()?;
    for AddEmbeddingsToCompanyEmbeddings_dataData {
        vector,
        text,
        chunk_id,
        page_number,
        reference,
        filing_id,
        category,
        subcategory,
        date1,
        date2,
        source,
        description,
    } in &data.embeddings_data
    {
        let embedding = G::new_mut(&db, &arena, &mut txn)
            .insert_v::<fn(&HVector, &RoTxn) -> bool>(
                &vector,
                "DocumentEmbedding",
                Some(ImmutablePropertiesMap::new(
                    6,
                    vec![
                        ("reference", Value::from(reference.clone())),
                        ("text", Value::from(text.clone())),
                        ("page_number", Value::from(page_number.clone())),
                        ("chunk_id", Value::from(chunk_id.clone())),
                        ("source_link", Value::from(source.clone())),
                        ("source_date", Value::from(date1.clone())),
                    ]
                    .into_iter(),
                    &arena,
                )),
            )
            .collect_to_obj()?;
        let edges = G::new_mut(&db, &arena, &mut txn)
            .add_edge(
                "DocumentEdge",
                Some(ImmutablePropertiesMap::new(
                    5,
                    vec![
                        ("description", Value::from(description.clone())),
                        ("date", Value::from(date2.clone())),
                        ("filing_id", Value::from(filing_id.clone())),
                        ("subcategory", Value::from(subcategory.clone())),
                        ("category", Value::from(category.clone())),
                    ]
                    .into_iter(),
                    &arena,
                )),
                c.id(),
                embedding.id(),
                false,
                false,
            )
            .collect_to_obj()?;
    }
    let response = json!({
        "data": "success"
    });
    txn.commit()
        .map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
    Ok(input.request.out_fmt.create_response(&response))
}

#[derive(Serialize, Default)]
pub struct GetCompaniesCompaniesReturnType<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub company_number: Option<&'a Value>,
    pub ingested_filings: Option<&'a Value>,
    pub total_filings: Option<&'a Value>,
}

#[handler]
pub fn GetCompanies(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let arena = Bump::new();
    let txn = db
        .graph_env
        .read_txn()
        .map_err(|e| GraphError::New(format!("Failed to start read transaction: {:?}", e)))?;
    let companies = G::new(&db, &txn, &arena)
        .n_from_type("Company")
        .collect::<Result<Vec<_>, _>>()?;
    let response = json!({
        "companies": companies.iter().map(|companie| GetCompaniesCompaniesReturnType {
            id: uuid_str(companie.id(), &arena),
            label: companie.label(),
            company_number: companie.get_property("company_number"),
            ingested_filings: companie.get_property("ingested_filings"),
            total_filings: companie.get_property("total_filings"),
        }).collect::<Vec<_>>()
    });
    txn.commit()
        .map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
    Ok(input.request.out_fmt.create_response(&response))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct UpdateCompanyInput {
    pub company_number: String,
    pub ingested_filings: i32,
}
#[derive(Serialize, Default)]
pub struct UpdateCompanyCompanyReturnType<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub company_number: Option<&'a Value>,
    pub ingested_filings: Option<&'a Value>,
    pub total_filings: Option<&'a Value>,
}

#[handler(is_write)]
pub fn UpdateCompany(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<UpdateCompanyInput>(&input.request.body)?;
    let arena = Bump::new();
    let mut txn = db
        .graph_env
        .write_txn()
        .map_err(|e| GraphError::New(format!("Failed to start write transaction: {:?}", e)))?;
    let company = {
        let update_tr = G::new(&db, &txn, &arena)
            .n_from_index("Company", "company_number", &data.company_number)
            .collect::<Result<Vec<_>, _>>()?;
        G::new_mut_from_iter(&db, &mut txn, update_tr.iter().cloned(), &arena)
            .update(&[("ingested_filings", Value::from(&data.ingested_filings))])
            .collect_to_obj()?
    };
    let response = json!({
        "company": UpdateCompanyCompanyReturnType {
            id: uuid_str(company.id(), &arena),
            label: company.label(),
            company_number: company.get_property("company_number"),
            ingested_filings: company.get_property("ingested_filings"),
            total_filings: company.get_property("total_filings"),
        }
    });
    txn.commit()
        .map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
    Ok(input.request.out_fmt.create_response(&response))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct AddCompanyInput {
    pub company_number: String,
    pub total_filings: i32,
}
#[derive(Serialize, Default)]
pub struct AddCompanyCompanyReturnType<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub company_number: Option<&'a Value>,
    pub ingested_filings: Option<&'a Value>,
    pub total_filings: Option<&'a Value>,
}

#[handler(is_write)]
pub fn AddCompany(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<AddCompanyInput>(&input.request.body)?;
    let arena = Bump::new();
    let mut txn = db
        .graph_env
        .write_txn()
        .map_err(|e| GraphError::New(format!("Failed to start write transaction: {:?}", e)))?;
    let company = G::new_mut(&db, &arena, &mut txn)
        .add_n(
            "Company",
            Some(ImmutablePropertiesMap::new(
                3,
                vec![
                    ("company_number", Value::from(&data.company_number)),
                    ("ingested_filings", Value::from(0)),
                    ("total_filings", Value::from(&data.total_filings)),
                ]
                .into_iter(),
                &arena,
            )),
            Some(&["company_number"]),
        )
        .collect_to_obj()?;
    let response = json!({
        "company": AddCompanyCompanyReturnType {
            id: uuid_str(company.id(), &arena),
            label: company.label(),
            company_number: company.get_property("company_number"),
            ingested_filings: company.get_property("ingested_filings"),
            total_filings: company.get_property("total_filings"),
        }
    });
    txn.commit()
        .map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
    Ok(input.request.out_fmt.create_response(&response))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct GetDocumentEdgesInput {
    pub company_number: String,
}
#[derive(Serialize, Default)]
pub struct GetDocumentEdgesEdgesEdgesReturnType<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub from_node: &'a str,
    pub to_node: &'a str,
    pub category: Option<&'a Value>,
    pub subcategory: Option<&'a Value>,
    pub description: Option<&'a Value>,
    pub date: Option<&'a Value>,
    pub filing_id: Option<&'a Value>,
}

#[derive(Serialize, Default)]
pub struct GetDocumentEdgesEdgesReturnType<'a> {
    pub count: Value,
    pub edges: GetDocumentEdgesEdgesEdgesReturnType<'a>,
}

#[handler]
pub fn GetDocumentEdges(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<GetDocumentEdgesInput>(&input.request.body)?;
    let arena = Bump::new();
    let txn = db
        .graph_env
        .read_txn()
        .map_err(|e| GraphError::New(format!("Failed to start read transaction: {:?}", e)))?;
    let c = G::new(&db, &txn, &arena)
        .n_from_index("Company", "company_number", &data.company_number)
        .collect_to_obj()?;
    let edges = G::from_iter(&db, &txn, std::iter::once(c.clone()), &arena)
        .out_e("DocumentEdge")
        .collect::<Result<Vec<_>, _>>()?;
    let count = G::from_iter(&db, &txn, std::iter::once(c.clone()), &arena)
        .out_vec("DocumentEdge", false)
        .count_to_val();
    let response = json!({
        "edges": edges.iter().map(|edge| Ok::<_, GraphError>(GetDocumentEdgesEdgesReturnType {
            count: count.clone(),
            edges: GetDocumentEdgesEdgesEdgesReturnType {
                            id: uuid_str(edge.id(), &arena),
                            label: edge.label(),
                            from_node: uuid_str(edge.from_node(), &arena),
                            to_node: uuid_str(edge.to_node(), &arena),
                            category: edge.get_property("category"),
                            subcategory: edge.get_property("subcategory"),
                            description: edge.get_property("description"),
                            date: edge.get_property("date"),
                            filing_id: edge.get_property("filing_id"),
                        },
        })).collect::<Result<Vec<_>, GraphError>>()?
    });
    txn.commit()
        .map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
    Ok(input.request.out_fmt.create_response(&response))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct DeleteCompanyInput {
    pub company_number: String,
}
#[handler(is_write)]
pub fn DeleteCompany(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<DeleteCompanyInput>(&input.request.body)?;
    let arena = Bump::new();
    let mut txn = db
        .graph_env
        .write_txn()
        .map_err(|e| GraphError::New(format!("Failed to start write transaction: {:?}", e)))?;
    Drop::drop_traversal(
        G::new(&db, &txn, &arena)
            .n_from_index("Company", "company_number", &data.company_number)
            .out_vec("DocumentEdge", false)
            .collect::<Vec<_>>()
            .into_iter(),
        &db,
        &mut txn,
    )?;
    Drop::drop_traversal(
        G::new(&db, &txn, &arena)
            .n_from_index("Company", "company_number", &data.company_number)
            .collect::<Vec<_>>()
            .into_iter(),
        &db,
        &mut txn,
    )?;
    let response = json!({
        "data": "success"
    });
    txn.commit()
        .map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
    Ok(input.request.out_fmt.create_response(&response))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct CompanyEmbeddingSearchInput {
    pub company_number: String,
    pub query: Vec<f64>,
    pub k: i32,
}
#[derive(Serialize, Default)]
pub struct CompanyEmbeddingSearchEmbedding_searchReturnType<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub data: &'a [f64],
    pub score: f64,
    pub chunk_id: Option<&'a Value>,
    pub page_number: Option<&'a Value>,
    pub reference: Option<&'a Value>,
    pub source_link: Option<&'a Value>,
    pub source_date: Option<&'a Value>,
    pub text: Option<&'a Value>,
}

#[handler]
pub fn CompanyEmbeddingSearch(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<CompanyEmbeddingSearchInput>(&input.request.body)?;
    let arena = Bump::new();
    let txn = db
        .graph_env
        .read_txn()
        .map_err(|e| GraphError::New(format!("Failed to start read transaction: {:?}", e)))?;
    let c = G::new(&db, &txn, &arena)
        .n_from_index("Company", "company_number", &data.company_number)
        .out_e("DocumentEdge")
        .to_v(false)
        .collect::<Result<Vec<_>, _>>()?;
    let embedding_search = G::from_iter(&db, &txn, c.iter().cloned(), &arena)
        .brute_force_search_v(&data.query, data.k.clone())
        .collect::<Result<Vec<_>, _>>()?;
    let response = json!({
        "embedding_search": embedding_search.iter().map(|embedding_search| CompanyEmbeddingSearchEmbedding_searchReturnType {
            id: uuid_str(embedding_search.id(), &arena),
            label: embedding_search.label(),
            data: embedding_search.data(),
            score: embedding_search.score(),
            chunk_id: embedding_search.get_property("chunk_id"),
            page_number: embedding_search.get_property("page_number"),
            reference: embedding_search.get_property("reference"),
            source_link: embedding_search.get_property("source_link"),
            source_date: embedding_search.get_property("source_date"),
            text: embedding_search.get_property("text"),
        }).collect::<Vec<_>>()
    });
    txn.commit()
        .map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
    Ok(input.request.out_fmt.create_response(&response))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SearchVectorInput {
    pub query: Vec<f64>,
    pub k: i32,
}
#[derive(Serialize, Default)]
pub struct SearchVectorEmbedding_searchReturnType<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub data: &'a [f64],
    pub score: f64,
    pub chunk_id: Option<&'a Value>,
    pub page_number: Option<&'a Value>,
    pub reference: Option<&'a Value>,
    pub source_link: Option<&'a Value>,
    pub source_date: Option<&'a Value>,
    pub text: Option<&'a Value>,
}

#[handler]
pub fn SearchVector(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<SearchVectorInput>(&input.request.body)?;
    let arena = Bump::new();
    let txn = db
        .graph_env
        .read_txn()
        .map_err(|e| GraphError::New(format!("Failed to start read transaction: {:?}", e)))?;
    let embedding_search = G::new(&db, &txn, &arena)
        .search_v::<fn(&HVector, &RoTxn) -> bool, _>(
            &data.query,
            data.k.clone(),
            "DocumentEmbedding",
            None,
        )
        .collect::<Result<Vec<_>, _>>()?;
    let response = json!({
        "embedding_search": embedding_search.iter().map(|embedding_search| SearchVectorEmbedding_searchReturnType {
            id: uuid_str(embedding_search.id(), &arena),
            label: embedding_search.label(),
            data: embedding_search.data(),
            score: embedding_search.score(),
            chunk_id: embedding_search.get_property("chunk_id"),
            page_number: embedding_search.get_property("page_number"),
            reference: embedding_search.get_property("reference"),
            source_link: embedding_search.get_property("source_link"),
            source_date: embedding_search.get_property("source_date"),
            text: embedding_search.get_property("text"),
        }).collect::<Vec<_>>()
    });
    txn.commit()
        .map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
    Ok(input.request.out_fmt.create_response(&response))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct GetCompanyInput {
    pub company_number: String,
}
#[derive(Serialize, Default)]
pub struct GetCompanyCompanyReturnType<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub company_number: Option<&'a Value>,
    pub ingested_filings: Option<&'a Value>,
    pub total_filings: Option<&'a Value>,
}

#[handler]
pub fn GetCompany(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<GetCompanyInput>(&input.request.body)?;
    let arena = Bump::new();
    let txn = db
        .graph_env
        .read_txn()
        .map_err(|e| GraphError::New(format!("Failed to start read transaction: {:?}", e)))?;
    let company = G::new(&db, &txn, &arena)
        .n_from_index("Company", "company_number", &data.company_number)
        .collect_to_obj()?;
    let response = json!({
        "company": GetCompanyCompanyReturnType {
            id: uuid_str(company.id(), &arena),
            label: company.label(),
            company_number: company.get_property("company_number"),
            ingested_filings: company.get_property("ingested_filings"),
            total_filings: company.get_property("total_filings"),
        }
    });
    txn.commit()
        .map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
    Ok(input.request.out_fmt.create_response(&response))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct AddVectorInput {
    pub vector: Vec<f64>,
    pub text: String,
    pub chunk_id: String,
    pub page_number: i32,
    pub reference: String,
}
#[derive(Serialize, Default)]
pub struct AddVectorEmbeddingReturnType<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub data: &'a [f64],
    pub score: f64,
    pub chunk_id: Option<&'a Value>,
    pub page_number: Option<&'a Value>,
    pub reference: Option<&'a Value>,
    pub source_link: Option<&'a Value>,
    pub source_date: Option<&'a Value>,
    pub text: Option<&'a Value>,
}

#[handler(is_write)]
pub fn AddVector(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<AddVectorInput>(&input.request.body)?;
    let arena = Bump::new();
    let mut txn = db
        .graph_env
        .write_txn()
        .map_err(|e| GraphError::New(format!("Failed to start write transaction: {:?}", e)))?;
    let embedding = G::new_mut(&db, &arena, &mut txn)
        .insert_v::<fn(&HVector, &RoTxn) -> bool>(
            &data.vector,
            "DocumentEmbedding",
            Some(ImmutablePropertiesMap::new(
                4,
                vec![
                    ("text", Value::from(data.text.clone())),
                    ("chunk_id", Value::from(data.chunk_id.clone())),
                    ("page_number", Value::from(data.page_number.clone())),
                    ("reference", Value::from(data.reference.clone())),
                ]
                .into_iter(),
                &arena,
            )),
        )
        .collect_to_obj()?;
    let response = json!({
        "embedding": AddVectorEmbeddingReturnType {
            id: uuid_str(embedding.id(), &arena),
            label: embedding.label(),
            data: embedding.data(),
            score: embedding.score(),
            chunk_id: embedding.get_property("chunk_id"),
            page_number: embedding.get_property("page_number"),
            reference: embedding.get_property("reference"),
            source_link: embedding.get_property("source_link"),
            source_date: embedding.get_property("source_date"),
            text: embedding.get_property("text"),
        }
    });
    txn.commit()
        .map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
    Ok(input.request.out_fmt.create_response(&response))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct GetAllCompanyEmbeddingsInput {
    pub company_number: String,
}
#[derive(Serialize, Default)]
pub struct GetAllCompanyEmbeddingsEmbeddingsReturnType<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub data: &'a [f64],
    pub score: f64,
    pub chunk_id: Option<&'a Value>,
    pub page_number: Option<&'a Value>,
    pub reference: Option<&'a Value>,
    pub source_link: Option<&'a Value>,
    pub source_date: Option<&'a Value>,
    pub text: Option<&'a Value>,
}

#[handler]
pub fn GetAllCompanyEmbeddings(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<GetAllCompanyEmbeddingsInput>(&input.request.body)?;
    let arena = Bump::new();
    let txn = db
        .graph_env
        .read_txn()
        .map_err(|e| GraphError::New(format!("Failed to start read transaction: {:?}", e)))?;
    let c = G::new(&db, &txn, &arena)
        .n_from_index("Company", "company_number", &data.company_number)
        .collect_to_obj()?;
    let embeddings = G::from_iter(&db, &txn, std::iter::once(c.clone()), &arena)
        .out_vec("DocumentEdge", false)
        .collect::<Result<Vec<_>, _>>()?;
    let response = json!({
        "embeddings": embeddings.iter().map(|embedding| GetAllCompanyEmbeddingsEmbeddingsReturnType {
            id: uuid_str(embedding.id(), &arena),
            label: embedding.label(),
            data: embedding.data(),
            score: embedding.score(),
            chunk_id: embedding.get_property("chunk_id"),
            page_number: embedding.get_property("page_number"),
            reference: embedding.get_property("reference"),
            source_link: embedding.get_property("source_link"),
            source_date: embedding.get_property("source_date"),
            text: embedding.get_property("text"),
        }).collect::<Vec<_>>()
    });
    txn.commit()
        .map_err(|e| GraphError::New(format!("Failed to commit transaction: {:?}", e)))?;
    Ok(input.request.out_fmt.create_response(&response))
}
