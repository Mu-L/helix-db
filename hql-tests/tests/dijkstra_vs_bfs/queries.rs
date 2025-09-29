
// DEFAULT CODE
// use helix_db::helix_engine::traversal_core::config::Config;

// pub fn config() -> Option<Config> {
//     None
// }



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
                    filter_ref::FilterRefAdapter, map::MapAdapter, paths::{PathAlgorithm, ShortestPathAdapter},
                    props::PropsAdapter, range::RangeAdapter, update::UpdateAdapter, order::OrderByAdapter,
                    aggregate::AggregateAdapter, group_by::GroupByAdapter,
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
        router::router::{HandlerInput, IoContFn},
        mcp::mcp::{MCPHandlerSubmission, MCPToolInput, MCPHandler}
    },
    node_matches, props, embed, embed_async,
    field_remapping, identifier_remapping, 
    traversal_remapping, exclude_field, value_remapping, 
    field_addition_from_old_field, field_type_cast, field_addition_from_value,
    protocol::{
        remapping::{Remapping, RemappingMap, ResponseRemapping},
        response::Response,
        return_values::ReturnValue,
        value::{casting::{cast, CastType}, Value},
        format::Format,
    },
    utils::{
        count::Count,
        filterable::Filterable,
        id::ID,
        items::{Edge, Node},
    },
};
use sonic_rs::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;
use chrono::{DateTime, Utc};
    
pub fn config() -> Option<Config> {
return Some(Config {
vector_config: Some(VectorConfig {
m: Some(16),
ef_construction: Some(128),
ef_search: Some(768),
}),
graph_config: Some(GraphConfig {
secondary_indices: Some(vec!["name".to_string(), "username".to_string(), "code".to_string()]),
}),
db_max_size_gb: Some(20),
mcp: Some(false),
bm25: Some(false),
schema: Some(r#"{
  "schema": {
    "nodes": [
      {
        "name": "Location",
        "properties": {
          "description": "String",
          "name": "String",
          "id": "ID"
        }
      },
      {
        "name": "Person",
        "properties": {
          "age": "I32",
          "id": "ID",
          "username": "String",
          "name": "String"
        }
      },
      {
        "name": "Station",
        "properties": {
          "city": "String",
          "id": "ID",
          "name": "String",
          "code": "String"
        }
      }
    ],
    "vectors": [],
    "edges": [
      {
        "name": "TrainRoute",
        "from": "Station",
        "to": "Station",
        "properties": {
          "duration_minutes": "I32",
          "high_speed": "String",
          "price": "F64"
        }
      },
      {
        "name": "Follows",
        "from": "Person",
        "to": "Person",
        "properties": {
          "since_year": "I32",
          "interaction_score": "F64"
        }
      },
      {
        "name": "BusRoute",
        "from": "Station",
        "to": "Station",
        "properties": {
          "price": "F64",
          "stops": "I32",
          "duration_minutes": "I32"
        }
      },
      {
        "name": "FlightPath",
        "from": "Location",
        "to": "Location",
        "properties": {
          "airline": "String",
          "cost": "F64",
          "flight_time": "F64"
        }
      },
      {
        "name": "Route",
        "from": "Location",
        "to": "Location",
        "properties": {
          "toll_cost": "F64",
          "scenic_rating": "I32",
          "distance": "F64"
        }
      }
    ]
  },
  "queries": [
    {
      "name": "socialDijkstra",
      "parameters": {
        "person2": "ID",
        "person1": "ID"
      },
      "returns": [
        "path"
      ]
    },
    {
      "name": "routeWithConstraints",
      "parameters": {
        "end": "ID",
        "start": "ID"
      },
      "returns": [
        "locations"
      ]
    },
    {
      "name": "flightBFS",
      "parameters": {
        "origin": "ID",
        "destination": "ID"
      },
      "returns": [
        "path"
      ]
    },
    {
      "name": "flightDijkstra",
      "parameters": {
        "destination": "ID",
        "origin": "ID"
      },
      "returns": [
        "path"
      ]
    },
    {
      "name": "routeBFS",
      "parameters": {
        "end": "ID",
        "start": "ID"
      },
      "returns": [
        "path"
      ]
    },
    {
      "name": "routeDijkstra",
      "parameters": {
        "end": "ID",
        "start": "ID"
      },
      "returns": [
        "path"
      ]
    },
    {
      "name": "busDijkstra",
      "parameters": {
        "from": "ID",
        "to": "ID"
      },
      "returns": [
        "path"
      ]
    },
    {
      "name": "busBFS",
      "parameters": {
        "to": "ID",
        "from": "ID"
      },
      "returns": [
        "path"
      ]
    },
    {
      "name": "routeDijkstraFrom",
      "parameters": {
        "end": "ID",
        "start": "ID"
      },
      "returns": [
        "path"
      ]
    },
    {
      "name": "trainBFS",
      "parameters": {
        "to": "ID",
        "from": "ID"
      },
      "returns": [
        "path"
      ]
    },
    {
      "name": "bidirectionalPaths",
      "parameters": {
        "b": "ID",
        "a": "ID"
      },
      "returns": []
    },
    {
      "name": "selfPath",
      "parameters": {
        "node": "ID"
      },
      "returns": [
        "path"
      ]
    },
    {
      "name": "trainDijkstra",
      "parameters": {
        "from": "ID",
        "to": "ID"
      },
      "returns": [
        "path"
      ]
    },
    {
      "name": "routeDefault",
      "parameters": {
        "start": "ID",
        "end": "ID"
      },
      "returns": [
        "path"
      ]
    },
    {
      "name": "noPath",
      "parameters": {
        "start": "ID",
        "end": "ID"
      },
      "returns": [
        "path"
      ]
    },
    {
      "name": "socialBFS",
      "parameters": {
        "person1": "ID",
        "person2": "ID"
      },
      "returns": [
        "path"
      ]
    },
    {
      "name": "compareAlgorithms",
      "parameters": {
        "end": "ID",
        "start": "ID"
      },
      "returns": []
    }
  ]
}"#.to_string()),
embedding_model: None,
graphvis_node_label: None,
})
}

pub struct Location {
    pub name: String,
    pub description: String,
}

pub struct Person {
    pub username: String,
    pub name: String,
    pub age: i32,
}

pub struct Station {
    pub code: String,
    pub name: String,
    pub city: String,
}

pub struct Route {
    pub from: Location,
    pub to: Location,
    pub distance: f64,
    pub toll_cost: f64,
    pub scenic_rating: i32,
}

pub struct FlightPath {
    pub from: Location,
    pub to: Location,
    pub flight_time: f64,
    pub cost: f64,
    pub airline: String,
}

pub struct Follows {
    pub from: Person,
    pub to: Person,
    pub since_year: i32,
    pub interaction_score: f64,
}

pub struct TrainRoute {
    pub from: Station,
    pub to: Station,
    pub duration_minutes: i32,
    pub price: f64,
    pub high_speed: String,
}

pub struct BusRoute {
    pub from: Station,
    pub to: Station,
    pub duration_minutes: i32,
    pub price: f64,
    pub stops: i32,
}


#[derive(Serialize, Deserialize, Clone)]
pub struct socialDijkstraInput {

pub person1: ID,
pub person2: ID
}
#[handler]
pub fn socialDijkstra (input: HandlerInput) -> Result<Response, GraphError> {
let db = Arc::clone(&input.graph.storage);
let data = input.request.in_fmt.deserialize::<socialDijkstraInput>(&input.request.body)?;
let mut remapping_vals = RemappingMap::new();
let txn = db.graph_env.read_txn().unwrap();
    let path = G::new(Arc::clone(&db), &txn)
.n_from_id(&data.person1)

.shortest_path(Some("Follows"), None, Some(&data.person2)).collect_to::<Vec<_>>();
let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
        return_vals.insert("path".to_string(), ReturnValue::from_traversal_value_array_with_mixin(path.clone(), remapping_vals.borrow_mut()));

txn.commit().unwrap();
Ok(input.request.out_fmt.create_response(&return_vals))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct routeWithConstraintsInput {

pub start: ID,
pub end: ID
}
#[handler]
pub fn routeWithConstraints (input: HandlerInput) -> Result<Response, GraphError> {
let db = Arc::clone(&input.graph.storage);
let data = input.request.in_fmt.deserialize::<routeWithConstraintsInput>(&input.request.body)?;
let mut remapping_vals = RemappingMap::new();
let txn = db.graph_env.read_txn().unwrap();
    let locations = G::new(Arc::clone(&db), &txn)
.n_from_id(&data.start)

.shortest_path(Some("Route"), None, Some(&data.end)).collect_to::<Vec<_>>();
let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
        return_vals.insert("locations".to_string(), ReturnValue::from_traversal_value_array_with_mixin(locations.clone(), remapping_vals.borrow_mut()));

txn.commit().unwrap();
Ok(input.request.out_fmt.create_response(&return_vals))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct flightBFSInput {

pub origin: ID,
pub destination: ID
}
#[handler]
pub fn flightBFS (input: HandlerInput) -> Result<Response, GraphError> {
let db = Arc::clone(&input.graph.storage);
let data = input.request.in_fmt.deserialize::<flightBFSInput>(&input.request.body)?;
let mut remapping_vals = RemappingMap::new();
let txn = db.graph_env.read_txn().unwrap();
    let path = G::new(Arc::clone(&db), &txn)
.n_from_id(&data.origin)

.shortest_path(Some("FlightPath"), None, Some(&data.destination)).collect_to::<Vec<_>>();
let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
        return_vals.insert("path".to_string(), ReturnValue::from_traversal_value_array_with_mixin(path.clone(), remapping_vals.borrow_mut()));

txn.commit().unwrap();
Ok(input.request.out_fmt.create_response(&return_vals))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct flightDijkstraInput {

pub origin: ID,
pub destination: ID
}
#[handler]
pub fn flightDijkstra (input: HandlerInput) -> Result<Response, GraphError> {
let db = Arc::clone(&input.graph.storage);
let data = input.request.in_fmt.deserialize::<flightDijkstraInput>(&input.request.body)?;
let mut remapping_vals = RemappingMap::new();
let txn = db.graph_env.read_txn().unwrap();
    let path = G::new(Arc::clone(&db), &txn)
.n_from_id(&data.origin)

.shortest_path(Some("FlightPath"), None, Some(&data.destination)).collect_to::<Vec<_>>();
let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
        return_vals.insert("path".to_string(), ReturnValue::from_traversal_value_array_with_mixin(path.clone(), remapping_vals.borrow_mut()));

txn.commit().unwrap();
Ok(input.request.out_fmt.create_response(&return_vals))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct routeBFSInput {

pub start: ID,
pub end: ID
}
#[handler]
pub fn routeBFS (input: HandlerInput) -> Result<Response, GraphError> {
let db = Arc::clone(&input.graph.storage);
let data = input.request.in_fmt.deserialize::<routeBFSInput>(&input.request.body)?;
let mut remapping_vals = RemappingMap::new();
let txn = db.graph_env.read_txn().unwrap();
    let path = G::new(Arc::clone(&db), &txn)
.n_from_id(&data.start)

.shortest_path(Some("Route"), None, Some(&data.end)).collect_to::<Vec<_>>();
let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
        return_vals.insert("path".to_string(), ReturnValue::from_traversal_value_array_with_mixin(path.clone(), remapping_vals.borrow_mut()));

txn.commit().unwrap();
Ok(input.request.out_fmt.create_response(&return_vals))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct routeDijkstraInput {

pub start: ID,
pub end: ID
}
#[handler]
pub fn routeDijkstra (input: HandlerInput) -> Result<Response, GraphError> {
let db = Arc::clone(&input.graph.storage);
let data = input.request.in_fmt.deserialize::<routeDijkstraInput>(&input.request.body)?;
let mut remapping_vals = RemappingMap::new();
let txn = db.graph_env.read_txn().unwrap();
    let path = G::new(Arc::clone(&db), &txn)
.n_from_id(&data.start)

.shortest_path(Some("Route"), None, Some(&data.end)).collect_to::<Vec<_>>();
let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
        return_vals.insert("path".to_string(), ReturnValue::from_traversal_value_array_with_mixin(path.clone(), remapping_vals.borrow_mut()));

txn.commit().unwrap();
Ok(input.request.out_fmt.create_response(&return_vals))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct busDijkstraInput {

pub from: ID,
pub to: ID
}
#[handler]
pub fn busDijkstra (input: HandlerInput) -> Result<Response, GraphError> {
let db = Arc::clone(&input.graph.storage);
let data = input.request.in_fmt.deserialize::<busDijkstraInput>(&input.request.body)?;
let mut remapping_vals = RemappingMap::new();
let txn = db.graph_env.read_txn().unwrap();
    let path = G::new(Arc::clone(&db), &txn)
.n_from_id(&data.from)

.shortest_path(Some("BusRoute"), None, Some(&data.to)).collect_to::<Vec<_>>();
let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
        return_vals.insert("path".to_string(), ReturnValue::from_traversal_value_array_with_mixin(path.clone(), remapping_vals.borrow_mut()));

txn.commit().unwrap();
Ok(input.request.out_fmt.create_response(&return_vals))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct busBFSInput {

pub from: ID,
pub to: ID
}
#[handler]
pub fn busBFS (input: HandlerInput) -> Result<Response, GraphError> {
let db = Arc::clone(&input.graph.storage);
let data = input.request.in_fmt.deserialize::<busBFSInput>(&input.request.body)?;
let mut remapping_vals = RemappingMap::new();
let txn = db.graph_env.read_txn().unwrap();
    let path = G::new(Arc::clone(&db), &txn)
.n_from_id(&data.from)

.shortest_path(Some("BusRoute"), None, Some(&data.to)).collect_to::<Vec<_>>();
let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
        return_vals.insert("path".to_string(), ReturnValue::from_traversal_value_array_with_mixin(path.clone(), remapping_vals.borrow_mut()));

txn.commit().unwrap();
Ok(input.request.out_fmt.create_response(&return_vals))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct routeDijkstraFromInput {

pub start: ID,
pub end: ID
}
#[handler]
pub fn routeDijkstraFrom (input: HandlerInput) -> Result<Response, GraphError> {
let db = Arc::clone(&input.graph.storage);
let data = input.request.in_fmt.deserialize::<routeDijkstraFromInput>(&input.request.body)?;
let mut remapping_vals = RemappingMap::new();
let txn = db.graph_env.read_txn().unwrap();
    let path = G::new(Arc::clone(&db), &txn)
.n_from_id(&data.end)

.shortest_path(Some("Route"), Some(&data.start), None).collect_to::<Vec<_>>();
let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
        return_vals.insert("path".to_string(), ReturnValue::from_traversal_value_array_with_mixin(path.clone(), remapping_vals.borrow_mut()));

txn.commit().unwrap();
Ok(input.request.out_fmt.create_response(&return_vals))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct trainBFSInput {

pub from: ID,
pub to: ID
}
#[handler]
pub fn trainBFS (input: HandlerInput) -> Result<Response, GraphError> {
let db = Arc::clone(&input.graph.storage);
let data = input.request.in_fmt.deserialize::<trainBFSInput>(&input.request.body)?;
let mut remapping_vals = RemappingMap::new();
let txn = db.graph_env.read_txn().unwrap();
    let path = G::new(Arc::clone(&db), &txn)
.n_from_id(&data.from)

.shortest_path(Some("TrainRoute"), None, Some(&data.to)).collect_to::<Vec<_>>();
let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
        return_vals.insert("path".to_string(), ReturnValue::from_traversal_value_array_with_mixin(path.clone(), remapping_vals.borrow_mut()));

txn.commit().unwrap();
Ok(input.request.out_fmt.create_response(&return_vals))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct bidirectionalPathsInput {

pub a: ID,
pub b: ID
}
#[handler]
pub fn bidirectionalPaths (input: HandlerInput) -> Result<Response, GraphError> {
let db = Arc::clone(&input.graph.storage);
let data = input.request.in_fmt.deserialize::<bidirectionalPathsInput>(&input.request.body)?;
let mut remapping_vals = RemappingMap::new();
let txn = db.graph_env.read_txn().unwrap();
    let forward_bfs = G::new(Arc::clone(&db), &txn)
.n_from_id(&data.a)

.shortest_path(Some("Route"), None, Some(&data.b)).collect_to::<Vec<_>>();
    let backward_bfs = G::new(Arc::clone(&db), &txn)
.n_from_id(&data.b)

.shortest_path(Some("Route"), Some(&data.a), None).collect_to::<Vec<_>>();
    let forward_dijkstra = G::new(Arc::clone(&db), &txn)
.n_from_id(&data.a)

.shortest_path(Some("Route"), None, Some(&data.b)).collect_to::<Vec<_>>();
    let backward_dijkstra = G::new(Arc::clone(&db), &txn)
.n_from_id(&data.b)

.shortest_path(Some("Route"), Some(&data.a), None).collect_to::<Vec<_>>();
let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
        return_vals.insert("data".to_string(), ReturnValue::from(HashMap::from([(String::from("backward_dijkstra"), ReturnValue::from(backward_dijkstra.clone())),(String::from("forward_dijkstra"), ReturnValue::from(forward_dijkstra.clone())),(String::from("forward_bfs"), ReturnValue::from(forward_bfs.clone())),(String::from("backward_bfs"), ReturnValue::from(backward_bfs.clone())),])));

txn.commit().unwrap();
Ok(input.request.out_fmt.create_response(&return_vals))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct selfPathInput {

pub node: ID
}
#[handler]
pub fn selfPath (input: HandlerInput) -> Result<Response, GraphError> {
let db = Arc::clone(&input.graph.storage);
let data = input.request.in_fmt.deserialize::<selfPathInput>(&input.request.body)?;
let mut remapping_vals = RemappingMap::new();
let txn = db.graph_env.read_txn().unwrap();
    let path = G::new(Arc::clone(&db), &txn)
.n_from_id(&data.node)

.shortest_path(Some("Route"), None, Some(&data.node)).collect_to::<Vec<_>>();
let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
        return_vals.insert("path".to_string(), ReturnValue::from_traversal_value_array_with_mixin(path.clone(), remapping_vals.borrow_mut()));

txn.commit().unwrap();
Ok(input.request.out_fmt.create_response(&return_vals))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct trainDijkstraInput {

pub from: ID,
pub to: ID
}
#[handler]
pub fn trainDijkstra (input: HandlerInput) -> Result<Response, GraphError> {
let db = Arc::clone(&input.graph.storage);
let data = input.request.in_fmt.deserialize::<trainDijkstraInput>(&input.request.body)?;
let mut remapping_vals = RemappingMap::new();
let txn = db.graph_env.read_txn().unwrap();
    let path = G::new(Arc::clone(&db), &txn)
.n_from_id(&data.from)

.shortest_path(Some("TrainRoute"), None, Some(&data.to)).collect_to::<Vec<_>>();
let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
        return_vals.insert("path".to_string(), ReturnValue::from_traversal_value_array_with_mixin(path.clone(), remapping_vals.borrow_mut()));

txn.commit().unwrap();
Ok(input.request.out_fmt.create_response(&return_vals))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct routeDefaultInput {

pub start: ID,
pub end: ID
}
#[handler]
pub fn routeDefault (input: HandlerInput) -> Result<Response, GraphError> {
let db = Arc::clone(&input.graph.storage);
let data = input.request.in_fmt.deserialize::<routeDefaultInput>(&input.request.body)?;
let mut remapping_vals = RemappingMap::new();
let txn = db.graph_env.read_txn().unwrap();
    let path = G::new(Arc::clone(&db), &txn)
.n_from_id(&data.start)

.shortest_path(Some("Route"), None, Some(&data.end)).collect_to::<Vec<_>>();
let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
        return_vals.insert("path".to_string(), ReturnValue::from_traversal_value_array_with_mixin(path.clone(), remapping_vals.borrow_mut()));

txn.commit().unwrap();
Ok(input.request.out_fmt.create_response(&return_vals))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct noPathInput {

pub start: ID,
pub end: ID
}
#[handler]
pub fn noPath (input: HandlerInput) -> Result<Response, GraphError> {
let db = Arc::clone(&input.graph.storage);
let data = input.request.in_fmt.deserialize::<noPathInput>(&input.request.body)?;
let mut remapping_vals = RemappingMap::new();
let txn = db.graph_env.read_txn().unwrap();
    let path = G::new(Arc::clone(&db), &txn)
.n_from_id(&data.start)

.shortest_path(Some("Route"), None, Some(&data.end)).collect_to::<Vec<_>>();
let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
        return_vals.insert("path".to_string(), ReturnValue::from_traversal_value_array_with_mixin(path.clone(), remapping_vals.borrow_mut()));

txn.commit().unwrap();
Ok(input.request.out_fmt.create_response(&return_vals))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct socialBFSInput {

pub person1: ID,
pub person2: ID
}
#[handler]
pub fn socialBFS (input: HandlerInput) -> Result<Response, GraphError> {
let db = Arc::clone(&input.graph.storage);
let data = input.request.in_fmt.deserialize::<socialBFSInput>(&input.request.body)?;
let mut remapping_vals = RemappingMap::new();
let txn = db.graph_env.read_txn().unwrap();
    let path = G::new(Arc::clone(&db), &txn)
.n_from_id(&data.person1)

.shortest_path(Some("Follows"), None, Some(&data.person2)).collect_to::<Vec<_>>();
let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
        return_vals.insert("path".to_string(), ReturnValue::from_traversal_value_array_with_mixin(path.clone(), remapping_vals.borrow_mut()));

txn.commit().unwrap();
Ok(input.request.out_fmt.create_response(&return_vals))
}

#[derive(Serialize, Deserialize, Clone)]
pub struct compareAlgorithmsInput {

pub start: ID,
pub end: ID
}
#[handler]
pub fn compareAlgorithms (input: HandlerInput) -> Result<Response, GraphError> {
let db = Arc::clone(&input.graph.storage);
let data = input.request.in_fmt.deserialize::<compareAlgorithmsInput>(&input.request.body)?;
let mut remapping_vals = RemappingMap::new();
let txn = db.graph_env.read_txn().unwrap();
    let bfs_path = G::new(Arc::clone(&db), &txn)
.n_from_id(&data.start)

.shortest_path(Some("Route"), None, Some(&data.end)).collect_to::<Vec<_>>();
    let dijkstra_path = G::new(Arc::clone(&db), &txn)
.n_from_id(&data.start)

.shortest_path(Some("Route"), None, Some(&data.end)).collect_to::<Vec<_>>();
let mut return_vals: HashMap<String, ReturnValue> = HashMap::new();
        return_vals.insert("data".to_string(), ReturnValue::from(HashMap::from([(String::from("dijkstra"), ReturnValue::from(dijkstra_path.clone())),(String::from("bfs"), ReturnValue::from(bfs_path.clone())),])));

txn.commit().unwrap();
Ok(input.request.out_fmt.create_response(&return_vals))
}


