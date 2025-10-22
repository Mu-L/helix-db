use crate::{
    helix_engine::{
        storage_core::HelixGraphStorage,
        traversal_core::{
            ops::util::{aggregate::AggregateAdapter, group_by::GroupByAdapter},
            traversal_value::TraversalValue,
        },
        types::GraphError,
    },
    helix_gateway::mcp::tools::{execute_query_chain, ToolArgs},
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

    let mut connections = input.mcp_connections.lock().unwrap();
    let mut connection = connections
        .remove_connection(&data.connection_id)
        .ok_or_else(|| GraphError::StorageError("Connection not found".to_string()))?;
    drop(connections);

    connection.add_query_step(data.tool);

    let arena = Bump::new();
    let storage = input.mcp_backend.db.as_ref();
    let txn = storage.graph_env.read_txn()?;
    let stream = execute_query_chain(&connection.query_chain, storage, &txn, &arena)?;
    let mut iter = stream.into_iter();

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
    let mut iter = stream.into_iter();

    let mut index = 0usize;
    let range = data.range;
    let start = range.as_ref().map(|r| r.start).unwrap_or(0);
    let end = range.as_ref().map(|r| r.end);

    let mut values = Vec::new();
    while let Some(item) = iter.next() {
        let item = item?;
        if index >= start {
            if let Some(end) = end {
                if index >= end {
                    break;
                }
            }
            values.push(item);
        }
        index += 1;
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
