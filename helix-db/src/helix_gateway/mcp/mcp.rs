use crate::{
    helix_engine::{
        storage_core::HelixGraphStorage,
        traversal_core::{
            ops::util::{aggregate::AggregateAdapter, group_by::GroupByAdapter},
            traversal_value::TraversalValue,
        },
        types::GraphError,
    },
    helix_gateway::mcp::tools::{execute_query_chain, EdgeType, FilterTraversal, Order, ToolArgs},
    protocol::{Format, Request, Response},
    utils::id::v6_uuid,
};
use bumpalo::Bump;
use helix_macros::mcp_handler;
use serde::Deserialize;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

pub type QueryStep = ToolArgs;

pub struct McpConnections {
    pub connections: HashMap<String, MCPConnection>,
}

impl McpConnections {
    pub fn new() -> Self {
        Self {
            connections: HashMap::new(),
        }
    }

    pub fn new_with_max_connections(max_connections: usize) -> Self {
        Self {
            connections: HashMap::with_capacity(max_connections),
        }
    }

    pub fn add_connection(&mut self, connection: MCPConnection) {
        self.connections
            .insert(connection.connection_id.clone(), connection);
    }

    pub fn remove_connection(&mut self, connection_id: &str) -> Option<MCPConnection> {
        self.connections.remove(connection_id)
    }

    pub fn get_connection(&self, connection_id: &str) -> Option<&MCPConnection> {
        self.connections.get(connection_id)
    }

    pub fn get_connection_mut(&mut self, connection_id: &str) -> Option<&mut MCPConnection> {
        self.connections.get_mut(connection_id)
    }
}

impl Default for McpConnections {
    fn default() -> Self {
        Self::new()
    }
}

pub struct McpBackend {
    pub db: Arc<HelixGraphStorage>,
}

impl McpBackend {
    pub fn new(db: Arc<HelixGraphStorage>) -> Self {
        Self { db }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ToolCallRequest {
    pub connection_id: String,
    pub tool: ToolArgs,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResourceCallRequest {
    pub connection_id: String,
}

pub struct MCPConnection {
    pub connection_id: String,
    pub query_chain: Vec<QueryStep>,
    pub current_position: usize,
}

impl MCPConnection {
    pub fn new(connection_id: String) -> Self {
        Self {
            connection_id,
            query_chain: Vec::new(),
            current_position: 0,
        }
    }

    pub fn add_query_step(&mut self, step: QueryStep) {
        self.query_chain.push(step);
        self.current_position = 0;
    }

    pub fn reset_position(&mut self) {
        self.current_position = 0;
    }

    pub fn clear_chain(&mut self) {
        self.query_chain.clear();
        self.reset_position();
    }

    pub fn next_item<'db, 'arena>(
        &mut self,
        db: &'db HelixGraphStorage,
        arena: &'arena Bump,
    ) -> Result<TraversalValue<'arena>, GraphError>
    where
        'db: 'arena,
    {
        let txn = db.graph_env.read_txn()?;
        let stream = execute_query_chain(&self.query_chain, db, &txn, arena)?;
        match stream.nth(self.current_position)? {
            Some(value) => {
                self.current_position += 1;
                Ok(value)
            }
            None => Ok(TraversalValue::Empty),
        }
    }
}

pub struct MCPToolInput {
    pub request: Request,
    pub mcp_backend: Arc<McpBackend>,
    pub mcp_connections: Arc<Mutex<McpConnections>>,
    pub schema: Option<String>,
}

pub type BasicMCPHandlerFn = for<'a> fn(&'a mut MCPToolInput) -> Result<Response, GraphError>;

pub type MCPHandlerFn =
    Arc<dyn for<'a> Fn(&'a mut MCPToolInput) -> Result<Response, GraphError> + Send + Sync>;

#[derive(Clone, Debug)]
pub struct MCPHandlerSubmission(pub MCPHandler);

#[derive(Clone, Debug)]
pub struct MCPHandler {
    pub name: &'static str,
    pub func: BasicMCPHandlerFn,
}

impl MCPHandler {
    pub const fn new(name: &'static str, func: BasicMCPHandlerFn) -> Self {
        Self { name, func }
    }
}

inventory::collect!(MCPHandlerSubmission);

/// Helper function to execute a tool step on a connection
fn execute_tool_step(
    input: &mut MCPToolInput,
    connection_id: &str,
    tool: ToolArgs,
) -> Result<Response, GraphError> {
    let mut connections = input.mcp_connections.lock().unwrap();
    let mut connection = connections
        .remove_connection(connection_id)
        .ok_or_else(|| GraphError::StorageError("Connection not found".to_string()))?;
    drop(connections);

    connection.add_query_step(tool);

    let arena = Bump::new();
    let storage = input.mcp_backend.db.as_ref();
    let txn = storage.graph_env.read_txn()?;
    let stream = execute_query_chain(&connection.query_chain, storage, &txn, &arena)?;
    let mut iter = stream.into_inner_iter();

    let (first, consumed_one) = match iter.next() {
        Some(value) => (value?, true),
        None => (TraversalValue::Empty, false),
    };

    connection.current_position = if consumed_one { 1 } else { 0 };

    let mut connections = input.mcp_connections.lock().unwrap();
    connections.add_connection(connection);
    drop(connections);

    Ok(Format::Json.create_response(&first))
}

#[derive(Deserialize)]
pub struct InitRequest {
    pub connection_addr: String,
    pub connection_port: u16,
}

#[mcp_handler]
pub fn init(input: &mut MCPToolInput) -> Result<Response, GraphError> {
    let connection_id = uuid::Uuid::from_u128(v6_uuid()).to_string();
    let mut connections = input.mcp_connections.lock().unwrap();
    connections.add_connection(MCPConnection::new(connection_id.clone()));
    drop(connections);
    Ok(Format::Json.create_response(&connection_id))
}

#[mcp_handler]
pub fn tool_call(input: &mut MCPToolInput) -> Result<Response, GraphError> {
    let data: ToolCallRequest = match sonic_rs::from_slice(&input.request.body) {
        Ok(data) => data,
        Err(err) => return Err(GraphError::from(err)),
    };

    execute_tool_step(input, &data.connection_id, data.tool)
}

#[derive(Deserialize)]
pub struct NextRequest {
    pub connection_id: String,
}

#[mcp_handler]
pub fn next(input: &mut MCPToolInput) -> Result<Response, GraphError> {
    let data: NextRequest = match sonic_rs::from_slice(&input.request.body) {
        Ok(data) => data,
        Err(err) => return Err(GraphError::from(err)),
    };

    let mut connections = input.mcp_connections.lock().unwrap();
    let mut connection = connections
        .remove_connection(&data.connection_id)
        .ok_or_else(|| GraphError::StorageError("Connection not found".to_string()))?;
    drop(connections);

    let arena = Bump::new();
    let storage = input.mcp_backend.db.as_ref();
    let next = connection.next_item(storage, &arena)?;

    let mut connections = input.mcp_connections.lock().unwrap();
    connections.add_connection(connection);
    drop(connections);

    Ok(Format::Json.create_response(&next))
}

#[derive(Deserialize)]
pub struct Range {
    pub start: usize,
    pub end: usize,
}

#[derive(Deserialize)]
pub struct CollectRequest {
    pub connection_id: String,
    pub range: Option<Range>,
    pub drop: Option<bool>,
}

#[mcp_handler]
pub fn collect(input: &mut MCPToolInput) -> Result<Response, GraphError> {
    let data: CollectRequest = match sonic_rs::from_slice(&input.request.body) {
        Ok(data) => data,
        Err(err) => return Err(GraphError::from(err)),
    };

    let mut connections = input.mcp_connections.lock().unwrap();
    let connection = connections
        .remove_connection(&data.connection_id)
        .ok_or_else(|| GraphError::StorageError("Connection not found".to_string()))?;
    drop(connections);

    let arena = Bump::new();
    let storage = input.mcp_backend.db.as_ref();
    let txn = storage.graph_env.read_txn()?;
    let stream = execute_query_chain(&connection.query_chain, storage, &txn, &arena)?;
    let iter = stream.into_inner_iter();

    let range = data.range;
    let start = range.as_ref().map(|r| r.start).unwrap_or(0);
    let end = range.as_ref().map(|r| r.end);

    let mut values = Vec::new();
    for (index, item) in iter.enumerate() {
        let item = item?;
        if index >= start {
            if let Some(end) = end
                && index >= end {
                    break;
                }
            values.push(item);
        }
    }

    let mut connections = input.mcp_connections.lock().unwrap();

    if data.drop.unwrap_or(true) {
        connections.add_connection(MCPConnection::new(connection.connection_id.clone()));
    } else {
        connections.add_connection(connection);
    }

    drop(connections);

    Ok(Format::Json.create_response(&values))
}

#[derive(Deserialize)]
pub struct AggregateRequest {
    pub connection_id: String,
    properties: Vec<String>,
    pub drop: Option<bool>,
}

#[mcp_handler]
pub fn aggregate_by(input: &mut MCPToolInput) -> Result<Response, GraphError> {
    let data: AggregateRequest = match sonic_rs::from_slice(&input.request.body) {
        Ok(data) => data,
        Err(err) => return Err(GraphError::from(err)),
    };

    let mut connections = input.mcp_connections.lock().unwrap();
    let connection = connections
        .remove_connection(&data.connection_id)
        .ok_or_else(|| GraphError::StorageError("Connection not found".to_string()))?;
    drop(connections);

    let arena = Bump::new();
    let storage = input.mcp_backend.db.as_ref();
    let txn = storage.graph_env.read_txn()?;
    let stream = execute_query_chain(&connection.query_chain, storage, &txn, &arena)?;

    let aggregation = stream
        .into_ro()
        .aggregate_by(&data.properties, true)?
        .into_count();

    let mut connections = input.mcp_connections.lock().unwrap();
    if data.drop.unwrap_or(true) {
        connections.add_connection(MCPConnection::new(connection.connection_id.clone()));
    } else {
        connections.add_connection(connection);
    }
    drop(connections);

    Ok(Format::Json.create_response(&aggregation))
}

#[mcp_handler]
pub fn group_by(input: &mut MCPToolInput) -> Result<Response, GraphError> {
    let data: AggregateRequest = match sonic_rs::from_slice(&input.request.body) {
        Ok(data) => data,
        Err(err) => return Err(GraphError::from(err)),
    };

    let mut connections = input.mcp_connections.lock().unwrap();
    let connection = connections
        .remove_connection(&data.connection_id)
        .ok_or_else(|| GraphError::StorageError("Connection not found".to_string()))?;
    drop(connections);

    let arena = Bump::new();
    let storage = input.mcp_backend.db.as_ref();
    let txn = storage.graph_env.read_txn()?;
    let stream = execute_query_chain(&connection.query_chain, storage, &txn, &arena)?;

    let aggregation = stream
        .into_ro()
        .group_by(&data.properties, true)?
        .into_count();

    let mut connections = input.mcp_connections.lock().unwrap();
    if data.drop.unwrap_or(true) {
        connections.add_connection(MCPConnection::new(connection.connection_id.clone()));
    } else {
        connections.add_connection(connection);
    }
    drop(connections);

    Ok(Format::Json.create_response(&aggregation))
}

#[derive(Deserialize)]
pub struct ResetRequest {
    pub connection_id: String,
}

#[mcp_handler]
pub fn reset(input: &mut MCPToolInput) -> Result<Response, GraphError> {
    let data: ResetRequest = match sonic_rs::from_slice(&input.request.body) {
        Ok(data) => data,
        Err(err) => return Err(GraphError::from(err)),
    };

    let mut connections = input.mcp_connections.lock().unwrap();
    let connection = connections
        .remove_connection(&data.connection_id)
        .ok_or_else(|| GraphError::StorageError("Connection not found".to_string()))?;
    let connection_id = connection.connection_id.clone();

    connections.add_connection(MCPConnection::new(connection_id.clone()));
    drop(connections);

    Ok(Format::Json.create_response(&connection_id))
}

#[mcp_handler]
pub fn schema_resource(input: &mut MCPToolInput) -> Result<Response, GraphError> {
    let data: ResourceCallRequest = match sonic_rs::from_slice(&input.request.body) {
        Ok(data) => data,
        Err(err) => return Err(GraphError::from(err)),
    };

    let connections = input.mcp_connections.lock().unwrap();
    if !connections.connections.contains_key(&data.connection_id) {
        return Err(GraphError::StorageError("Connection not found".to_string()));
    }
    drop(connections);

    if let Some(schema) = &input.schema {
        Ok(Format::Json.create_response(&schema.clone()))
    } else {
        Ok(Format::Json.create_response(&"no schema".to_string()))
    }
}

// Individual tool endpoint handlers

#[derive(Debug, Deserialize)]
pub struct OutStepInput {
    pub connection_id: String,
    pub edge_label: String,
    pub edge_type: EdgeType,
    pub filter: Option<FilterTraversal>,
}

#[mcp_handler]
pub fn out_step(input: &mut MCPToolInput) -> Result<Response, GraphError> {
    let data: OutStepInput = match sonic_rs::from_slice(&input.request.body) {
        Ok(data) => data,
        Err(err) => return Err(GraphError::from(err)),
    };

    let tool = ToolArgs::OutStep {
        edge_label: data.edge_label,
        edge_type: data.edge_type,
        filter: data.filter,
    };

    execute_tool_step(input, &data.connection_id, tool)
}

#[derive(Debug, Deserialize)]
pub struct InStepInput {
    pub connection_id: String,
    pub edge_label: String,
    pub edge_type: EdgeType,
    pub filter: Option<FilterTraversal>,
}

#[mcp_handler]
pub fn in_step(input: &mut MCPToolInput) -> Result<Response, GraphError> {
    let data: InStepInput = match sonic_rs::from_slice(&input.request.body) {
        Ok(data) => data,
        Err(err) => return Err(GraphError::from(err)),
    };

    let tool = ToolArgs::InStep {
        edge_label: data.edge_label,
        edge_type: data.edge_type,
        filter: data.filter,
    };

    execute_tool_step(input, &data.connection_id, tool)
}

#[derive(Debug, Deserialize)]
pub struct OutEStepInput {
    pub connection_id: String,
    pub edge_label: String,
    pub filter: Option<FilterTraversal>,
}

#[mcp_handler]
pub fn out_e_step(input: &mut MCPToolInput) -> Result<Response, GraphError> {
    let data: OutEStepInput = match sonic_rs::from_slice(&input.request.body) {
        Ok(data) => data,
        Err(err) => return Err(GraphError::from(err)),
    };

    let tool = ToolArgs::OutEStep {
        edge_label: data.edge_label,
        filter: data.filter,
    };

    execute_tool_step(input, &data.connection_id, tool)
}

#[derive(Debug, Deserialize)]
pub struct InEStepInput {
    pub connection_id: String,
    pub edge_label: String,
    pub filter: Option<FilterTraversal>,
}

#[mcp_handler]
pub fn in_e_step(input: &mut MCPToolInput) -> Result<Response, GraphError> {
    let data: InEStepInput = match sonic_rs::from_slice(&input.request.body) {
        Ok(data) => data,
        Err(err) => return Err(GraphError::from(err)),
    };

    let tool = ToolArgs::InEStep {
        edge_label: data.edge_label,
        filter: data.filter,
    };

    execute_tool_step(input, &data.connection_id, tool)
}

#[derive(Debug, Deserialize)]
pub struct NFromTypeInput {
    pub connection_id: String,
    pub node_type: String,
}

#[mcp_handler]
pub fn n_from_type(input: &mut MCPToolInput) -> Result<Response, GraphError> {
    let data: NFromTypeInput = match sonic_rs::from_slice(&input.request.body) {
        Ok(data) => data,
        Err(err) => return Err(GraphError::from(err)),
    };

    let tool = ToolArgs::NFromType {
        node_type: data.node_type,
    };

    execute_tool_step(input, &data.connection_id, tool)
}

#[derive(Debug, Deserialize)]
pub struct EFromTypeInput {
    pub connection_id: String,
    pub edge_type: String,
}

#[mcp_handler]
pub fn e_from_type(input: &mut MCPToolInput) -> Result<Response, GraphError> {
    let data: EFromTypeInput = match sonic_rs::from_slice(&input.request.body) {
        Ok(data) => data,
        Err(err) => return Err(GraphError::from(err)),
    };

    let tool = ToolArgs::EFromType {
        edge_type: data.edge_type,
    };

    execute_tool_step(input, &data.connection_id, tool)
}

#[derive(Debug, Deserialize)]
pub struct FilterItemsInput {
    pub connection_id: String,
    #[serde(default)]
    pub filter: FilterTraversal,
}

#[mcp_handler]
pub fn filter_items(input: &mut MCPToolInput) -> Result<Response, GraphError> {
    let data: FilterItemsInput = match sonic_rs::from_slice(&input.request.body) {
        Ok(data) => data,
        Err(err) => return Err(GraphError::from(err)),
    };

    let tool = ToolArgs::FilterItems {
        filter: data.filter,
    };

    execute_tool_step(input, &data.connection_id, tool)
}

#[derive(Debug, Deserialize)]
pub struct OrderByInput {
    pub connection_id: String,
    pub properties: String,
    pub order: Order,
}

#[mcp_handler]
pub fn order_by(input: &mut MCPToolInput) -> Result<Response, GraphError> {
    let data: OrderByInput = match sonic_rs::from_slice(&input.request.body) {
        Ok(data) => data,
        Err(err) => return Err(GraphError::from(err)),
    };

    let tool = ToolArgs::OrderBy {
        properties: data.properties,
        order: data.order,
    };

    execute_tool_step(input, &data.connection_id, tool)
}

#[derive(Debug, Deserialize)]
pub struct SearchKeywordInput {
    pub connection_id: String,
    pub query: String,
    pub limit: usize,
    pub label: String,
}

#[mcp_handler]
pub fn search_keyword(input: &mut MCPToolInput) -> Result<Response, GraphError> {
    use crate::helix_engine::traversal_core::ops::{
        bm25::search_bm25::SearchBM25Adapter,
        g::G,
    };

    let data: SearchKeywordInput = match sonic_rs::from_slice(&input.request.body) {
        Ok(data) => data,
        Err(err) => return Err(GraphError::from(err)),
    };

    let mut connections = input.mcp_connections.lock().unwrap();
    let mut connection = connections
        .remove_connection(&data.connection_id)
        .ok_or_else(|| GraphError::StorageError("Connection not found".to_string()))?;
    drop(connections);

    let arena = Bump::new();
    let storage = input.mcp_backend.db.as_ref();
    let txn = storage.graph_env.read_txn()?;

    // Perform BM25 search using the existing index
    let results = G::new(storage, &txn, &arena)
        .search_bm25(&data.label, &data.query, data.limit)?
        .collect_to::<Vec<_>>();

    let (first, consumed_one) = match results.first() {
        Some(value) => (value.clone(), true),
        None => (TraversalValue::Empty, false),
    };

    // Store remaining results for pagination
    connection.current_position = if consumed_one { 1 } else { 0 };
    // Note: For search_keyword, we don't update the query_chain since it's a starting operation

    let mut connections = input.mcp_connections.lock().unwrap();
    connections.add_connection(connection);
    drop(connections);

    Ok(Format::Json.create_response(&first))
}

#[derive(Debug, Deserialize)]
pub struct SearchVecTextInput {
    pub connection_id: String,
    pub query: String,
    pub label: String,
    pub k: Option<usize>,
}

#[mcp_handler]
pub fn search_vec_text(input: &mut MCPToolInput) -> Result<Response, GraphError> {
    use crate::helix_engine::traversal_core::ops::{
        g::G,
        vectors::search::SearchVAdapter,
    };
    use crate::helix_gateway::embedding_providers::{get_embedding_model, EmbeddingModel};

    let data: SearchVecTextInput = match sonic_rs::from_slice(&input.request.body) {
        Ok(data) => data,
        Err(err) => return Err(GraphError::from(err)),
    };

    let mut connections = input.mcp_connections.lock().unwrap();
    let mut connection = connections
        .remove_connection(&data.connection_id)
        .ok_or_else(|| GraphError::StorageError("Connection not found".to_string()))?;
    drop(connections);

    let arena = Bump::new();
    let storage = input.mcp_backend.db.as_ref();
    let txn = storage.graph_env.read_txn()?;

    // Get embedding model and convert query text to vector
    let embedding_model = get_embedding_model(None, None, None)?;
    let query_embedding = embedding_model.fetch_embedding(&data.query)?;
    let query_vec_arena = arena.alloc_slice_copy(&query_embedding);

    // Perform vector search
    let k_value = data.k.unwrap_or(10);
    let label_arena = arena.alloc_str(&data.label);
    let results = G::new(storage, &txn, &arena)
        .search_v::<fn(&crate::helix_engine::vector_core::vector::HVector, &heed3::RoTxn) -> bool, _>(
            query_vec_arena,
            k_value,
            label_arena,
            None
        )
        .collect_to::<Vec<_>>();

    let (first, consumed_one) = match results.first() {
        Some(value) => (value.clone(), true),
        None => (TraversalValue::Empty, false),
    };

    connection.current_position = if consumed_one { 1 } else { 0 };

    let mut connections = input.mcp_connections.lock().unwrap();
    connections.add_connection(connection);
    drop(connections);

    Ok(Format::Json.create_response(&first))
}

#[derive(Debug, Deserialize)]
pub struct SearchVecInput {
    pub connection_id: String,
    pub vector: Vec<f64>,
    pub k: usize,
    pub min_score: Option<f64>,
}

#[mcp_handler]
pub fn search_vec(input: &mut MCPToolInput) -> Result<Response, GraphError> {
    let data: SearchVecInput = match sonic_rs::from_slice(&input.request.body) {
        Ok(data) => data,
        Err(err) => return Err(GraphError::from(err)),
    };

    let tool = ToolArgs::SearchVec {
        vector: data.vector,
        k: data.k,
        min_score: data.min_score,
    };

    execute_tool_step(input, &data.connection_id, tool)
}
