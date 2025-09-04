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
        "name": "City",
        "properties": {
          "description": "String",
          "zip_codes": "Array(String)",
          "name": "String",
          "id": "ID"
        }
      },
      {
        "name": "Continent",
        "properties": {
          "id": "ID",
          "name": "String"
        }
      },
      {
        "name": "Country",
        "properties": {
          "name": "String",
          "id": "ID",
          "currency": "String",
          "gdp": "F64",
          "population": "I64"
        }
      }
    ],
    "vectors": [
      {
        "name": "CityDescription",
        "properties": {
          "vector": "Array(F64)",
          "id": "ID"
        }
      }
    ],
    "edges": [
      {
        "name": "Country_to_Capital",
        "from": "Country",
        "to": "City",
        "properties": {}
      },
      {
        "name": "Continent_to_Country",
        "from": "Continent",
        "to": "Country",
        "properties": {}
      },
      {
        "name": "Country_to_City",
        "from": "Country",
        "to": "City",
        "properties": {}
      },
      {
        "name": "City_to_Embedding",
        "from": "City",
        "to": "CityDescription",
        "properties": {}
      }
    ]
  },
  "queries": [
    {
      "name": "getCountriesByPopGdp",
      "parameters": {
        "min_population": "I64",
        "max_gdp": "F64"
      },
      "returns": [
        "countries"
      ]
    },
    {
      "name": "updateCurrency",
      "parameters": {
        "country_id": "ID",
        "currency": "String"
      },
      "returns": [
        "country"
      ]
    },
    {
      "name": "getCapital",
      "parameters": {
        "country_id": "ID"
      },
      "returns": [
        "capital"
      ]
    },
    {
      "name": "getCitiesInCountry",
      "parameters": {
        "country_id": "ID"
      },
      "returns": [
        "cities"
      ]
    },
    {
      "name": "getCountriesWithCapitals",
      "parameters": {},
      "returns": [
        "countries"
      ]
    },
    {
      "name": "getCountriesByGdp",
      "parameters": {
        "min_gdp": "F64"
      },
      "returns": [
        "countries"
      ]
    },
    {
      "name": "getAllContinents",
      "parameters": {},
      "returns": [
        "continents"
      ]
    },
    {
      "name": "getCountryByCityCnt",
      "parameters": {
        "num_cities": "I64"
      },
      "returns": [
        "countries"
      ]
    },
    {
      "name": "deleteCapital",
      "parameters": {
        "country_id": "ID"
      },
      "returns": []
    },
    {
      "name": "getCityByName",
      "parameters": {
        "city_name": "String"
      },
      "returns": [
        "city"
      ]
    },
    {
      "name": "countCapitals",
      "parameters": {},
      "returns": [
        "num_capital"
      ]
    },
    {
      "name": "updatePopGdp",
      "parameters": {
        "gdp": "F64",
        "country_id": "ID",
        "population": "I64"
      },
      "returns": [
        "country"
      ]
    },
    {
      "name": "getContinentCities",
      "parameters": {
        "k": "I64",
        "continent_name": "String"
      },
      "returns": [
        "cities"
      ]
    },
    {
      "name": "getCountry",
      "parameters": {
        "country_id": "ID"
      },
      "returns": [
        "country"
      ]
    },
    {
      "name": "getContinent",
      "parameters": {
        "continent_id": "ID"
      },
      "returns": [
        "continent"
      ]
    },
    {
      "name": "getCity",
      "parameters": {
        "city_id": "ID"
      },
      "returns": [
        "city"
      ]
    },
    {
      "name": "searchDescriptions",
      "parameters": {
        "vector": "Array(F64)",
        "k": "I64"
      },
      "returns": [
        "cities"
      ]
    },
    {
      "name": "getCountriesByPopulation",
      "parameters": {
        "max_population": "I64"
      },
      "returns": [
        "countries"
      ]
    },
    {
      "name": "createCity",
      "parameters": {
        "description": "String",
        "country_id": "ID",
        "name": "String"
      },
      "returns": [
        "city"
      ]
    },
    {
      "name": "updateDescription",
      "parameters": {
        "description": "String",
        "city_id": "ID",
        "vector": "Array(F64)"
      },
      "returns": [
        "city"
      ]
    },
    {
      "name": "createCountry",
      "parameters": {
        "population": "I64",
        "currency": "String",
        "continent_id": "ID",
        "gdp": "F64",
        "name": "String"
      },
      "returns": [
        "country"
      ]
    },
    {
      "name": "getCountryNames",
      "parameters": {},
      "returns": [
        "countries"
      ]
    },
    {
      "name": "deleteCountry",
      "parameters": {
        "country_id": "ID"
      },
      "returns": []
    },
    {
      "name": "embedDescription",
      "parameters": {
        "vector": "Array(F64)",
        "city_id": "ID"
      },
      "returns": [
        "embedding"
      ]
    },
    {
      "name": "getAllCountries",
      "parameters": {},
      "returns": [
        "countries"
      ]
    },
    {
      "name": "getAllCities",
      "parameters": {},
      "returns": [
        "cities"
      ]
    },
    {
      "name": "updateCapital",
      "parameters": {
        "city_id": "ID",
        "country_id": "ID"
      },
      "returns": [
        "city"
      ]
    },
    {
      "name": "getCountriesByCurrency",
      "parameters": {
        "currency": "String"
      },
      "returns": [
        "countries"
      ]
    },
    {
      "name": "getContinentByName",
      "parameters": {
        "continent_name": "String"
      },
      "returns": [
        "continent"
      ]
    },
    {
      "name": "createContinent",
      "parameters": {
        "name": "String"
      },
      "returns": [
        "continent"
      ]
    },
    {
      "name": "getCountriesInContinent",
      "parameters": {
        "continent_id": "ID"
      },
      "returns": [
        "countries"
      ]
    },
    {
      "name": "deleteCity",
      "parameters": {
        "city_id": "ID"
      },
      "returns": []
    },
    {
      "name": "getCountriesByCurrPop",
      "parameters": {
        "max_population": "I64",
        "currency": "String"
      },
      "returns": [
        "countries"
      ]
    },
    {
      "name": "getCountryByName",
      "parameters": {
        "country_name": "String"
      },
      "returns": [
        "country"
      ]
    },
    {
      "name": "setCapital",
      "parameters": {
        "country_id": "ID",
        "city_id": "ID"
      },
      "returns": [
        "country_capital"
      ]
    }
  ]
}"#
            .to_string(),
        ),
        embedding_model: Some("text-embedding-ada-002".to_string()),
        graphvis_node_label: Some("".to_string()),
    });
}

pub struct Continent {
    pub name: String,
}

pub struct Country {
    pub name: String,
    pub population: i64,
    pub currency: String,
    pub gdp: f64,
}

pub struct City {
    pub name: String,
    pub zip_codes: Vec<String>,
    pub description: String,
}

pub struct Continent_to_Country {
    pub from: Continent,
    pub to: Country,
}

pub struct Country_to_City {
    pub from: Country,
    pub to: City,
}

pub struct Country_to_Capital {
    pub from: Country,
    pub to: City,
}

pub struct City_to_Embedding {
    pub from: City,
    pub to: CityDescription,
}

pub struct CityDescription {
    pub vector: Vec<f64>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct getCountriesByPopGdpInput {
    pub min_population: i64,
    pub max_gdp: f64,
}
#[handler]
pub fn getCountriesByPopGdp(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<getCountriesByPopGdpInput>(&input.request.body)?;
    let mut remapping_vals = RemappingMap::new();
    let txn = db.graph_env.read_txn().unwrap();
    let countries = G::new(Arc::clone(&db), &txn)
        .n_from_type("Country")
        .filter_ref(|val, txn| {
            if let Ok(val) = val {
                Ok(G::new_from(Arc::clone(&db), &txn, val.clone())
                    .check_property("population")
                    .map_value_or(false, |v| *v > data.min_population.clone())?
                    && G::new_from(Arc::clone(&db), &txn, val.clone())
                        .check_property("gdp")
                        .map_value_or(false, |v| *v <= data.max_gdp.clone())?)
            } else {
                Ok(false)
            }
        })
        .collect_to::<Vec<_>>();
    let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
    return_vals.insert(
        "countries".to_string(),
        ReturnValue::from_traversal_value_array_with_mixin(
            countries.clone().clone(),
            remapping_vals.borrow_mut(),
        ),
    );

    txn.commit().unwrap();
    Ok(input.request.out_fmt.create_response(&return_vals))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct updateCurrencyInput {
    pub country_id: ID,
    pub currency: String,
}
#[handler]
pub fn updateCurrency(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<updateCurrencyInput>(&input.request.body)?;
    let mut remapping_vals = RemappingMap::new();
    let mut txn = db.graph_env.write_txn().unwrap();
    let country = {
        let update_tr = G::new(Arc::clone(&db), &txn)
            .n_from_id(&data.country_id)
            .collect_to::<Vec<_>>();
        G::new_mut_from(Arc::clone(&db), &mut txn, update_tr)
            .update(Some(props! { "currency" => &data.currency }))
            .collect_to_obj()
    };
    let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
    return_vals.insert(
        "country".to_string(),
        ReturnValue::from_traversal_value_with_mixin(
            country.clone().clone(),
            remapping_vals.borrow_mut(),
        ),
    );

    txn.commit().unwrap();
    Ok(input.request.out_fmt.create_response(&return_vals))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct getCapitalInput {
    pub country_id: ID,
}
#[handler]
pub fn getCapital(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<getCapitalInput>(&input.request.body)?;
    let mut remapping_vals = RemappingMap::new();
    let txn = db.graph_env.read_txn().unwrap();
    let country = G::new(Arc::clone(&db), &txn)
        .n_from_id(&data.country_id)
        .collect_to_obj();
    let capital = G::new_from(Arc::clone(&db), &txn, country.clone())
        .out("Country_to_Capital", &EdgeType::Node)
        .collect_to::<Vec<_>>();
    let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
    return_vals.insert(
        "capital".to_string(),
        ReturnValue::from_traversal_value_array_with_mixin(
            capital.clone().clone(),
            remapping_vals.borrow_mut(),
        ),
    );

    txn.commit().unwrap();
    Ok(input.request.out_fmt.create_response(&return_vals))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct getCitiesInCountryInput {
    pub country_id: ID,
}
#[handler]
pub fn getCitiesInCountry(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<getCitiesInCountryInput>(&input.request.body)?;
    let mut remapping_vals = RemappingMap::new();
    let txn = db.graph_env.read_txn().unwrap();
    let country = G::new(Arc::clone(&db), &txn)
        .n_from_id(&data.country_id)
        .collect_to_obj();
    let cities = G::new_from(Arc::clone(&db), &txn, country.clone())
        .out("Country_to_City", &EdgeType::Node)
        .collect_to::<Vec<_>>();
    let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
    return_vals.insert(
        "cities".to_string(),
        ReturnValue::from_traversal_value_array_with_mixin(
            cities.clone().clone(),
            remapping_vals.borrow_mut(),
        ),
    );

    txn.commit().unwrap();
    Ok(input.request.out_fmt.create_response(&return_vals))
}

#[handler]
pub fn getCountriesWithCapitals(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let mut remapping_vals = RemappingMap::new();
    let txn = db.graph_env.read_txn().unwrap();
    let countries = G::new(Arc::clone(&db), &txn)
        .n_from_type("Country")
        .filter_ref(|val, txn| {
            if let Ok(val) = val {
                Ok(Exist::exists(
                    &mut G::new_from(Arc::clone(&db), &txn, vec![val.clone()])
                        .out("Country_to_Capital", &EdgeType::Node),
                ))
            } else {
                Ok(false)
            }
        })
        .collect_to::<Vec<_>>();
    let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
    return_vals.insert(
        "countries".to_string(),
        ReturnValue::from_traversal_value_array_with_mixin(
            countries.clone().clone(),
            remapping_vals.borrow_mut(),
        ),
    );

    txn.commit().unwrap();
    Ok(input.request.out_fmt.create_response(&return_vals))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct getCountriesByGdpInput {
    pub min_gdp: f64,
}
#[handler]
pub fn getCountriesByGdp(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<getCountriesByGdpInput>(&input.request.body)?;
    let mut remapping_vals = RemappingMap::new();
    let txn = db.graph_env.read_txn().unwrap();
    let countries = G::new(Arc::clone(&db), &txn)
        .n_from_type("Country")
        .filter_ref(|val, txn| {
            if let Ok(val) = val {
                Ok(G::new_from(Arc::clone(&db), &txn, val.clone())
                    .check_property("gdp")
                    .map_value_or(false, |v| *v >= data.min_gdp.clone())?)
            } else {
                Ok(false)
            }
        })
        .collect_to::<Vec<_>>();
    let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
    return_vals.insert(
        "countries".to_string(),
        ReturnValue::from_traversal_value_array_with_mixin(
            countries.clone().clone(),
            remapping_vals.borrow_mut(),
        ),
    );

    txn.commit().unwrap();
    Ok(input.request.out_fmt.create_response(&return_vals))
}

#[handler]
pub fn getAllContinents(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let mut remapping_vals = RemappingMap::new();
    let txn = db.graph_env.read_txn().unwrap();
    let continents = G::new(Arc::clone(&db), &txn)
        .n_from_type("Continent")
        .collect_to::<Vec<_>>();
    let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
    return_vals.insert(
        "continents".to_string(),
        ReturnValue::from_traversal_value_array_with_mixin(
            continents.clone().clone(),
            remapping_vals.borrow_mut(),
        ),
    );

    txn.commit().unwrap();
    Ok(input.request.out_fmt.create_response(&return_vals))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct getCountryByCityCntInput {
    pub num_cities: i64,
}
#[handler]
pub fn getCountryByCityCnt(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<getCountryByCityCntInput>(&input.request.body)?;
    let mut remapping_vals = RemappingMap::new();
    let txn = db.graph_env.read_txn().unwrap();
    let countries = G::new(Arc::clone(&db), &txn)
        .n_from_type("Country")
        .filter_ref(|val, txn| {
            if let Ok(val) = val {
                Ok(G::new_from(Arc::clone(&db), &txn, val.clone())
                    .out("Country_to_City", &EdgeType::Node)
                    .count_to_val()
                    .map_value_or(false, |v| *v > data.num_cities.clone())?)
            } else {
                Ok(false)
            }
        })
        .collect_to::<Vec<_>>();
    let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
    return_vals.insert(
        "countries".to_string(),
        ReturnValue::from_traversal_value_array_with_mixin(
            countries.clone().clone(),
            remapping_vals.borrow_mut(),
        ),
    );

    txn.commit().unwrap();
    Ok(input.request.out_fmt.create_response(&return_vals))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct deleteCapitalInput {
    pub country_id: ID,
}
#[handler]
pub fn deleteCapital(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<deleteCapitalInput>(&input.request.body)?;
    let mut remapping_vals = RemappingMap::new();
    let mut txn = db.graph_env.write_txn().unwrap();
    Drop::<Vec<_>>::drop_traversal(
        G::new(Arc::clone(&db), &txn)
            .n_from_id(&data.country_id)
            .out("Country_to_Capital", &EdgeType::Node)
            .collect_to::<Vec<_>>(),
        Arc::clone(&db),
        &mut txn,
    )?;
    let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
    return_vals.insert(
        "success".to_string(),
        ReturnValue::from(Value::from("success")),
    );

    txn.commit().unwrap();
    Ok(input.request.out_fmt.create_response(&return_vals))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct getCityByNameInput {
    pub city_name: String,
}
#[handler]
pub fn getCityByName(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<getCityByNameInput>(&input.request.body)?;
    let mut remapping_vals = RemappingMap::new();
    let txn = db.graph_env.read_txn().unwrap();
    let city = G::new(Arc::clone(&db), &txn)
        .n_from_type("City")
        .filter_ref(|val, txn| {
            if let Ok(val) = val {
                Ok(G::new_from(Arc::clone(&db), &txn, val.clone())
                    .check_property("name")
                    .map_value_or(false, |v| *v == data.city_name.clone())?)
            } else {
                Ok(false)
            }
        })
        .collect_to::<Vec<_>>();
    let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
    return_vals.insert(
        "city".to_string(),
        ReturnValue::from_traversal_value_array_with_mixin(
            city.clone().clone(),
            remapping_vals.borrow_mut(),
        ),
    );

    txn.commit().unwrap();
    Ok(input.request.out_fmt.create_response(&return_vals))
}

#[handler]
pub fn countCapitals(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let mut remapping_vals = RemappingMap::new();
    let txn = db.graph_env.read_txn().unwrap();
    let num_capital = G::new(Arc::clone(&db), &txn)
        .n_from_type("City")
        .filter_ref(|val, txn| {
            if let Ok(val) = val {
                Ok(Exist::exists(
                    &mut G::new_from(Arc::clone(&db), &txn, vec![val.clone()])
                        .in_("Country_to_Capital", &EdgeType::Node),
                ))
            } else {
                Ok(false)
            }
        })
        .count_to_val();
    let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
    return_vals.insert(
        "num_capital".to_string(),
        ReturnValue::from(Value::from(num_capital.clone())),
    );

    txn.commit().unwrap();
    Ok(input.request.out_fmt.create_response(&return_vals))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct updatePopGdpInput {
    pub country_id: ID,
    pub population: i64,
    pub gdp: f64,
}
#[handler]
pub fn updatePopGdp(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<updatePopGdpInput>(&input.request.body)?;
    let mut remapping_vals = RemappingMap::new();
    let mut txn = db.graph_env.write_txn().unwrap();
    let country = {
        let update_tr = G::new(Arc::clone(&db), &txn)
            .n_from_id(&data.country_id)
            .collect_to::<Vec<_>>();
        G::new_mut_from(Arc::clone(&db), &mut txn, update_tr)
            .update(Some(
                props! { "population" => &data.population, "gdp" => &data.gdp },
            ))
            .collect_to_obj()
    };
    let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
    return_vals.insert(
        "country".to_string(),
        ReturnValue::from_traversal_value_with_mixin(
            country.clone().clone(),
            remapping_vals.borrow_mut(),
        ),
    );

    txn.commit().unwrap();
    Ok(input.request.out_fmt.create_response(&return_vals))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct getContinentCitiesInput {
    pub continent_name: String,
    pub k: i64,
}
#[handler]
pub fn getContinentCities(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<getContinentCitiesInput>(&input.request.body)?;
    let mut remapping_vals = RemappingMap::new();
    let txn = db.graph_env.read_txn().unwrap();
    let continent = G::new(Arc::clone(&db), &txn)
        .n_from_type("Continent")
        .filter_ref(|val, txn| {
            if let Ok(val) = val {
                Ok(G::new_from(Arc::clone(&db), &txn, val.clone())
                    .check_property("name")
                    .map_value_or(false, |v| *v == data.continent_name.clone())?)
            } else {
                Ok(false)
            }
        })
        .collect_to::<Vec<_>>();
    let countries = G::new_from(Arc::clone(&db), &txn, continent.clone())
        .out("Continent_to_Country", &EdgeType::Node)
        .collect_to::<Vec<_>>();
    let cities = G::new_from(Arc::clone(&db), &txn, countries.clone())
        .out("Country_to_City", &EdgeType::Node)
        .range(0, data.k.clone())
        .collect_to::<Vec<_>>();
    let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
    return_vals.insert(
        "cities".to_string(),
        ReturnValue::from_traversal_value_array_with_mixin(
            cities.clone().clone(),
            remapping_vals.borrow_mut(),
        ),
    );

    txn.commit().unwrap();
    Ok(input.request.out_fmt.create_response(&return_vals))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct getCountryInput {
    pub country_id: ID,
}
#[handler]
pub fn getCountry(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<getCountryInput>(&input.request.body)?;
    let mut remapping_vals = RemappingMap::new();
    let txn = db.graph_env.read_txn().unwrap();
    let country = G::new(Arc::clone(&db), &txn)
        .n_from_id(&data.country_id)
        .collect_to_obj();
    let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
    return_vals.insert(
        "country".to_string(),
        ReturnValue::from_traversal_value_with_mixin(
            country.clone().clone(),
            remapping_vals.borrow_mut(),
        ),
    );

    txn.commit().unwrap();
    Ok(input.request.out_fmt.create_response(&return_vals))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct getContinentInput {
    pub continent_id: ID,
}
#[handler]
pub fn getContinent(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<getContinentInput>(&input.request.body)?;
    let mut remapping_vals = RemappingMap::new();
    let txn = db.graph_env.read_txn().unwrap();
    let continent = G::new(Arc::clone(&db), &txn)
        .n_from_id(&data.continent_id)
        .collect_to_obj();
    let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
    return_vals.insert(
        "continent".to_string(),
        ReturnValue::from_traversal_value_with_mixin(
            continent.clone().clone(),
            remapping_vals.borrow_mut(),
        ),
    );

    txn.commit().unwrap();
    Ok(input.request.out_fmt.create_response(&return_vals))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct getCityInput {
    pub city_id: ID,
}
#[handler]
pub fn getCity(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<getCityInput>(&input.request.body)?;
    let mut remapping_vals = RemappingMap::new();
    let txn = db.graph_env.read_txn().unwrap();
    let city = G::new(Arc::clone(&db), &txn)
        .n_from_id(&data.city_id)
        .collect_to_obj();
    let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
    return_vals.insert(
        "city".to_string(),
        ReturnValue::from_traversal_value_with_mixin(
            city.clone().clone(),
            remapping_vals.borrow_mut(),
        ),
    );

    txn.commit().unwrap();
    Ok(input.request.out_fmt.create_response(&return_vals))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct searchDescriptionsInput {
    pub vector: Vec<f64>,
    pub k: i64,
}
#[handler]
pub fn searchDescriptions(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<searchDescriptionsInput>(&input.request.body)?;
    let mut remapping_vals = RemappingMap::new();
    let txn = db.graph_env.read_txn().unwrap();
    let descriptions = G::new(Arc::clone(&db), &txn)
        .search_v::<fn(&HVector, &RoTxn) -> bool, _>(
            &data.vector,
            data.k.clone(),
            "CityDescription",
            None,
        )
        .collect_to::<Vec<_>>();
    let cities = G::new_from(Arc::clone(&db), &txn, descriptions.clone())
        .in_("City_to_Embedding", &EdgeType::Node)
        .collect_to::<Vec<_>>();
    let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
    return_vals.insert(
        "cities".to_string(),
        ReturnValue::from_traversal_value_array_with_mixin(
            cities.clone().clone(),
            remapping_vals.borrow_mut(),
        ),
    );

    txn.commit().unwrap();
    Ok(input.request.out_fmt.create_response(&return_vals))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct getCountriesByPopulationInput {
    pub max_population: i64,
}
#[handler]
pub fn getCountriesByPopulation(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<getCountriesByPopulationInput>(&input.request.body)?;
    let mut remapping_vals = RemappingMap::new();
    let txn = db.graph_env.read_txn().unwrap();
    let countries = G::new(Arc::clone(&db), &txn)
        .n_from_type("Country")
        .filter_ref(|val, txn| {
            if let Ok(val) = val {
                Ok(G::new_from(Arc::clone(&db), &txn, val.clone())
                    .check_property("population")
                    .map_value_or(false, |v| *v < data.max_population.clone())?)
            } else {
                Ok(false)
            }
        })
        .collect_to::<Vec<_>>();
    let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
    return_vals.insert(
        "countries".to_string(),
        ReturnValue::from_traversal_value_array_with_mixin(
            countries.clone().clone(),
            remapping_vals.borrow_mut(),
        ),
    );

    txn.commit().unwrap();
    Ok(input.request.out_fmt.create_response(&return_vals))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct createCityInput {
    pub country_id: ID,
    pub name: String,
    pub description: String,
}
#[handler]
pub fn createCity(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<createCityInput>(&input.request.body)?;
    let mut remapping_vals = RemappingMap::new();
    let mut txn = db.graph_env.write_txn().unwrap();
    let city = G::new_mut(Arc::clone(&db), &mut txn)
        .add_n(
            "City",
            Some(props! { "name" => &data.name, "description" => &data.description }),
            None,
        )
        .collect_to_obj();
    let country = G::new(Arc::clone(&db), &txn)
        .n_from_id(&data.country_id)
        .collect_to_obj();
    let country_city = G::new_mut(Arc::clone(&db), &mut txn)
        .add_e(
            "Country_to_City",
            None,
            country.id(),
            city.id(),
            true,
            EdgeType::Node,
        )
        .collect_to_obj();
    let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
    return_vals.insert(
        "city".to_string(),
        ReturnValue::from_traversal_value_with_mixin(
            city.clone().clone(),
            remapping_vals.borrow_mut(),
        ),
    );

    txn.commit().unwrap();
    Ok(input.request.out_fmt.create_response(&return_vals))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct updateDescriptionInput {
    pub city_id: ID,
    pub description: String,
    pub vector: Vec<f64>,
}
#[handler]
pub fn updateDescription(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<updateDescriptionInput>(&input.request.body)?;
    let mut remapping_vals = RemappingMap::new();
    let mut txn = db.graph_env.write_txn().unwrap();
    Drop::<Vec<_>>::drop_traversal(
        G::new(Arc::clone(&db), &txn)
            .n_from_id(&data.city_id)
            .out_e("City_to_Embedding")
            .collect_to::<Vec<_>>(),
        Arc::clone(&db),
        &mut txn,
    )?;
    let city = {
        let update_tr = G::new(Arc::clone(&db), &txn)
            .n_from_id(&data.city_id)
            .collect_to::<Vec<_>>();
        G::new_mut_from(Arc::clone(&db), &mut txn, update_tr)
            .update(Some(props! { "description" => &data.description }))
            .collect_to_obj()
    };
    let description_embedding = G::new_mut(Arc::clone(&db), &mut txn)
        .insert_v::<fn(&HVector, &RoTxn) -> bool>(&data.vector, "CityDescription", None)
        .collect_to_obj();
    let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
    return_vals.insert(
        "city".to_string(),
        ReturnValue::from_traversal_value_with_mixin(
            city.clone().clone(),
            remapping_vals.borrow_mut(),
        ),
    );

    txn.commit().unwrap();
    Ok(input.request.out_fmt.create_response(&return_vals))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct createCountryInput {
    pub continent_id: ID,
    pub name: String,
    pub currency: String,
    pub population: i64,
    pub gdp: f64,
}
#[handler]
pub fn createCountry(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<createCountryInput>(&input.request.body)?;
    let mut remapping_vals = RemappingMap::new();
    let mut txn = db.graph_env.write_txn().unwrap();
    let country = G::new_mut(Arc::clone(&db), &mut txn)
.add_n("Country", Some(props! { "currency" => &data.currency, "gdp" => &data.gdp, "name" => &data.name, "population" => &data.population }), None).collect_to_obj();
    let continent = G::new(Arc::clone(&db), &txn)
        .n_from_id(&data.continent_id)
        .collect_to_obj();
    let continent_country = G::new_mut(Arc::clone(&db), &mut txn)
        .add_e(
            "Continent_to_Country",
            None,
            continent.id(),
            country.id(),
            true,
            EdgeType::Node,
        )
        .collect_to_obj();
    let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
    return_vals.insert(
        "country".to_string(),
        ReturnValue::from_traversal_value_with_mixin(
            country.clone().clone(),
            remapping_vals.borrow_mut(),
        ),
    );

    txn.commit().unwrap();
    Ok(input.request.out_fmt.create_response(&return_vals))
}

#[handler]
pub fn getCountryNames(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let mut remapping_vals = RemappingMap::new();
    let txn = db.graph_env.read_txn().unwrap();
    let countries = G::new(Arc::clone(&db), &txn)
        .n_from_type("Country")
        .check_property("name")
        .collect_to::<Vec<_>>();
    let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
    return_vals.insert(
        "countries".to_string(),
        ReturnValue::from_traversal_value_array_with_mixin(
            countries.clone().clone(),
            remapping_vals.borrow_mut(),
        ),
    );

    txn.commit().unwrap();
    Ok(input.request.out_fmt.create_response(&return_vals))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct deleteCountryInput {
    pub country_id: ID,
}
#[handler]
pub fn deleteCountry(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<deleteCountryInput>(&input.request.body)?;
    let mut remapping_vals = RemappingMap::new();
    let mut txn = db.graph_env.write_txn().unwrap();
    Drop::<Vec<_>>::drop_traversal(
        G::new(Arc::clone(&db), &txn)
            .n_from_id(&data.country_id)
            .out_e("Country_to_City")
            .collect_to::<Vec<_>>(),
        Arc::clone(&db),
        &mut txn,
    )?;
    Drop::<Vec<_>>::drop_traversal(
        G::new(Arc::clone(&db), &txn)
            .n_from_id(&data.country_id)
            .collect_to_obj(),
        Arc::clone(&db),
        &mut txn,
    )?;
    let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
    return_vals.insert(
        "success".to_string(),
        ReturnValue::from(Value::from("success")),
    );

    txn.commit().unwrap();
    Ok(input.request.out_fmt.create_response(&return_vals))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct embedDescriptionInput {
    pub city_id: ID,
    pub vector: Vec<f64>,
}
#[handler]
pub fn embedDescription(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<embedDescriptionInput>(&input.request.body)?;
    let mut remapping_vals = RemappingMap::new();
    let mut txn = db.graph_env.write_txn().unwrap();
    let embedding = G::new_mut(Arc::clone(&db), &mut txn)
        .insert_v::<fn(&HVector, &RoTxn) -> bool>(&data.vector, "CityDescription", None)
        .collect_to_obj();
    let city = G::new(Arc::clone(&db), &txn)
        .n_from_id(&data.city_id)
        .collect_to_obj();
    let city_embedding = G::new_mut(Arc::clone(&db), &mut txn)
        .add_e(
            "City_to_Embedding",
            None,
            city.id(),
            embedding.id(),
            true,
            EdgeType::Node,
        )
        .collect_to_obj();
    let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
    return_vals.insert(
        "embedding".to_string(),
        ReturnValue::from_traversal_value_with_mixin(
            embedding.clone().clone(),
            remapping_vals.borrow_mut(),
        ),
    );

    txn.commit().unwrap();
    Ok(input.request.out_fmt.create_response(&return_vals))
}

#[handler]
pub fn getAllCountries(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let mut remapping_vals = RemappingMap::new();
    let txn = db.graph_env.read_txn().unwrap();
    let countries = G::new(Arc::clone(&db), &txn)
        .n_from_type("Country")
        .collect_to::<Vec<_>>();
    let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
    return_vals.insert(
        "countries".to_string(),
        ReturnValue::from_traversal_value_array_with_mixin(
            countries.clone().clone(),
            remapping_vals.borrow_mut(),
        ),
    );

    txn.commit().unwrap();
    Ok(input.request.out_fmt.create_response(&return_vals))
}

#[handler]
pub fn getAllCities(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let mut remapping_vals = RemappingMap::new();
    let txn = db.graph_env.read_txn().unwrap();
    let cities = G::new(Arc::clone(&db), &txn)
        .n_from_type("City")
        .collect_to::<Vec<_>>();
    let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
    return_vals.insert(
        "cities".to_string(),
        ReturnValue::from_traversal_value_array_with_mixin(
            cities.clone().clone(),
            remapping_vals.borrow_mut(),
        ),
    );

    txn.commit().unwrap();
    Ok(input.request.out_fmt.create_response(&return_vals))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct updateCapitalInput {
    pub country_id: ID,
    pub city_id: ID,
}
#[handler]
pub fn updateCapital(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<updateCapitalInput>(&input.request.body)?;
    let mut remapping_vals = RemappingMap::new();
    let mut txn = db.graph_env.write_txn().unwrap();
    Drop::<Vec<_>>::drop_traversal(
        G::new(Arc::clone(&db), &txn)
            .n_from_id(&data.country_id)
            .out_e("Country_to_Capital")
            .collect_to::<Vec<_>>(),
        Arc::clone(&db),
        &mut txn,
    )?;
    let country = G::new(Arc::clone(&db), &txn)
        .n_from_id(&data.country_id)
        .collect_to_obj();
    let city = G::new(Arc::clone(&db), &txn)
        .n_from_id(&data.city_id)
        .collect_to_obj();
    let capital = G::new_mut(Arc::clone(&db), &mut txn)
        .add_e(
            "Country_to_Capital",
            None,
            country.id(),
            city.id(),
            true,
            EdgeType::Node,
        )
        .collect_to_obj();
    let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
    return_vals.insert(
        "city".to_string(),
        ReturnValue::from_traversal_value_with_mixin(
            city.clone().clone(),
            remapping_vals.borrow_mut(),
        ),
    );

    txn.commit().unwrap();
    Ok(input.request.out_fmt.create_response(&return_vals))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct getCountriesByCurrencyInput {
    pub currency: String,
}
#[handler]
pub fn getCountriesByCurrency(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<getCountriesByCurrencyInput>(&input.request.body)?;
    let mut remapping_vals = RemappingMap::new();
    let txn = db.graph_env.read_txn().unwrap();
    let countries = G::new(Arc::clone(&db), &txn)
        .n_from_type("Country")
        .filter_ref(|val, txn| {
            if let Ok(val) = val {
                Ok(G::new_from(Arc::clone(&db), &txn, val.clone())
                    .check_property("currency")
                    .map_value_or(false, |v| *v == data.currency.clone())?)
            } else {
                Ok(false)
            }
        })
        .collect_to::<Vec<_>>();
    let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
    return_vals.insert(
        "countries".to_string(),
        ReturnValue::from_traversal_value_array_with_mixin(
            countries.clone().clone(),
            remapping_vals.borrow_mut(),
        ),
    );

    txn.commit().unwrap();
    Ok(input.request.out_fmt.create_response(&return_vals))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct getContinentByNameInput {
    pub continent_name: String,
}
#[handler]
pub fn getContinentByName(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<getContinentByNameInput>(&input.request.body)?;
    let mut remapping_vals = RemappingMap::new();
    let txn = db.graph_env.read_txn().unwrap();
    let continent = G::new(Arc::clone(&db), &txn)
        .n_from_type("Continent")
        .filter_ref(|val, txn| {
            if let Ok(val) = val {
                Ok(G::new_from(Arc::clone(&db), &txn, val.clone())
                    .check_property("name")
                    .map_value_or(false, |v| *v == data.continent_name.clone())?)
            } else {
                Ok(false)
            }
        })
        .collect_to::<Vec<_>>();
    let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
    return_vals.insert(
        "continent".to_string(),
        ReturnValue::from_traversal_value_array_with_mixin(
            continent.clone().clone(),
            remapping_vals.borrow_mut(),
        ),
    );

    txn.commit().unwrap();
    Ok(input.request.out_fmt.create_response(&return_vals))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct createContinentInput {
    pub name: String,
}
#[handler]
pub fn createContinent(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<createContinentInput>(&input.request.body)?;
    let mut remapping_vals = RemappingMap::new();
    let mut txn = db.graph_env.write_txn().unwrap();
    let continent = G::new_mut(Arc::clone(&db), &mut txn)
        .add_n("Continent", Some(props! { "name" => &data.name }), None)
        .collect_to_obj();
    let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
    return_vals.insert(
        "continent".to_string(),
        ReturnValue::from_traversal_value_with_mixin(
            continent.clone().clone(),
            remapping_vals.borrow_mut(),
        ),
    );

    txn.commit().unwrap();
    Ok(input.request.out_fmt.create_response(&return_vals))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct getCountriesInContinentInput {
    pub continent_id: ID,
}
#[handler]
pub fn getCountriesInContinent(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<getCountriesInContinentInput>(&input.request.body)?;
    let mut remapping_vals = RemappingMap::new();
    let txn = db.graph_env.read_txn().unwrap();
    let continent = G::new(Arc::clone(&db), &txn)
        .n_from_id(&data.continent_id)
        .collect_to_obj();
    let countries = G::new_from(Arc::clone(&db), &txn, continent.clone())
        .out("Continent_to_Country", &EdgeType::Node)
        .collect_to::<Vec<_>>();
    let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
    return_vals.insert(
        "countries".to_string(),
        ReturnValue::from_traversal_value_array_with_mixin(
            countries.clone().clone(),
            remapping_vals.borrow_mut(),
        ),
    );

    txn.commit().unwrap();
    Ok(input.request.out_fmt.create_response(&return_vals))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct deleteCityInput {
    pub city_id: ID,
}
#[handler]
pub fn deleteCity(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<deleteCityInput>(&input.request.body)?;
    let mut remapping_vals = RemappingMap::new();
    let mut txn = db.graph_env.write_txn().unwrap();
    Drop::<Vec<_>>::drop_traversal(
        G::new(Arc::clone(&db), &txn)
            .n_from_id(&data.city_id)
            .collect_to_obj(),
        Arc::clone(&db),
        &mut txn,
    )?;
    let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
    return_vals.insert(
        "success".to_string(),
        ReturnValue::from(Value::from("success")),
    );

    txn.commit().unwrap();
    Ok(input.request.out_fmt.create_response(&return_vals))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct getCountriesByCurrPopInput {
    pub currency: String,
    pub max_population: i64,
}
#[handler]
pub fn getCountriesByCurrPop(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<getCountriesByCurrPopInput>(&input.request.body)?;
    let mut remapping_vals = RemappingMap::new();
    let txn = db.graph_env.read_txn().unwrap();
    let countries = G::new(Arc::clone(&db), &txn)
        .n_from_type("Country")
        .filter_ref(|val, txn| {
            if let Ok(val) = val {
                Ok(G::new_from(Arc::clone(&db), &txn, val.clone())
                    .check_property("currency")
                    .map_value_or(false, |v| *v == data.currency.clone())?
                    || G::new_from(Arc::clone(&db), &txn, val.clone())
                        .check_property("population")
                        .map_value_or(false, |v| *v <= data.max_population.clone())?)
            } else {
                Ok(false)
            }
        })
        .collect_to::<Vec<_>>();
    let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
    return_vals.insert(
        "countries".to_string(),
        ReturnValue::from_traversal_value_array_with_mixin(
            countries.clone().clone(),
            remapping_vals.borrow_mut(),
        ),
    );

    txn.commit().unwrap();
    Ok(input.request.out_fmt.create_response(&return_vals))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct getCountryByNameInput {
    pub country_name: String,
}
#[handler]
pub fn getCountryByName(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<getCountryByNameInput>(&input.request.body)?;
    let mut remapping_vals = RemappingMap::new();
    let txn = db.graph_env.read_txn().unwrap();
    let country = G::new(Arc::clone(&db), &txn)
        .n_from_type("Country")
        .filter_ref(|val, txn| {
            if let Ok(val) = val {
                Ok(G::new_from(Arc::clone(&db), &txn, val.clone())
                    .check_property("name")
                    .map_value_or(false, |v| *v == data.country_name.clone())?)
            } else {
                Ok(false)
            }
        })
        .collect_to::<Vec<_>>();
    let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
    return_vals.insert(
        "country".to_string(),
        ReturnValue::from_traversal_value_array_with_mixin(
            country.clone().clone(),
            remapping_vals.borrow_mut(),
        ),
    );

    txn.commit().unwrap();
    Ok(input.request.out_fmt.create_response(&return_vals))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct setCapitalInput {
    pub country_id: ID,
    pub city_id: ID,
}
#[handler]
pub fn setCapital(input: HandlerInput) -> Result<Response, GraphError> {
    let db = Arc::clone(&input.graph.storage);
    let data = input
        .request
        .in_fmt
        .deserialize::<setCapitalInput>(&input.request.body)?;
    let mut remapping_vals = RemappingMap::new();
    let mut txn = db.graph_env.write_txn().unwrap();
    let country = G::new(Arc::clone(&db), &txn)
        .n_from_id(&data.country_id)
        .collect_to_obj();
    let city = G::new(Arc::clone(&db), &txn)
        .n_from_id(&data.city_id)
        .collect_to_obj();
    let country_capital = G::new_mut(Arc::clone(&db), &mut txn)
        .add_e(
            "Country_to_Capital",
            None,
            country.id(),
            city.id(),
            true,
            EdgeType::Node,
        )
        .collect_to_obj();
    let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
    return_vals.insert(
        "country_capital".to_string(),
        ReturnValue::from_traversal_value_with_mixin(
            country_capital.clone().clone(),
            remapping_vals.borrow_mut(),
        ),
    );

    txn.commit().unwrap();
    Ok(input.request.out_fmt.create_response(&return_vals))
}
