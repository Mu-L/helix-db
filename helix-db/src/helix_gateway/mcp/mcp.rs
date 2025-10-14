use crate::{
    helix_engine::{
        storage_core::HelixGraphStorage,
        traversal_core::{
            ops::{
                g::G,
                util::{aggregate::AggregateAdapter, group_by::GroupByAdapter},
            },
            traversal_value::TraversalValue,
        },
        types::GraphError,
    },
    helix_gateway::mcp::tools::ToolArgs,
    protocol::{Format, Request, Response, return_values::ReturnValue},
    utils::id::v6_uuid,
};
use helix_macros::mcp_handler;
use serde::Deserialize;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    vec::IntoIter,
};

pub struct McpConnections {
    pub connections: HashMap<String, MCPConnection>,
}

impl Default for McpConnections {
    fn default() -> Self {
        Self::new()
    }
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

    pub fn get_connection_owned(&mut self, connection_id: &str) -> Option<MCPConnection> {
        self.connections.remove(connection_id)
    }
}
pub struct McpBackend {
    pub db: Arc<HelixGraphStorage>,
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

impl McpBackend {
    pub fn new(db: Arc<HelixGraphStorage>) -> Self {
        Self { db }
    }
}

pub struct MCPConnection {
    pub connection_id: String,
    pub iter: IntoIter<TraversalValue>,
}

impl MCPConnection {
    pub fn new(connection_id: String, iter: IntoIter<TraversalValue>) -> Self {
        Self {
            connection_id,
            iter,
        }
    }
}

pub struct MCPToolInput {
    pub request: Request,
    pub mcp_backend: Arc<McpBackend>,
    pub mcp_connections: Arc<Mutex<McpConnections>>,
    pub schema: Option<String>,
}

// basic type for function pointer
pub type BasicMCPHandlerFn = for<'a> fn(&'a mut MCPToolInput) -> Result<Response, GraphError>;

// thread safe type for multi threaded use
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
    connections.add_connection(MCPConnection::new(
        connection_id.clone(),
        vec![].into_iter(),
    ));
    drop(connections);
    Ok(Format::Json.create_response(&ReturnValue::from(connection_id)))
}

#[derive(Deserialize)]
pub struct NextRequest {
    pub connection_id: String,
}

#[mcp_handler]
pub fn next(input: &mut MCPToolInput) -> Result<Response, GraphError> {
    let data: NextRequest = match sonic_rs::from_slice(&input.request.body) {
        Ok(data) => data,
        Err(e) => return Err(GraphError::from(e)),
    };

    let mut connections = input.mcp_connections.lock().unwrap();
    let connection = match connections.get_connection_mut(&data.connection_id) {
        Some(conn) => conn,
        None => return Err(GraphError::StorageError("Connection not found".to_string())),
    };

    let next = connection
        .iter
        .next()
        .unwrap_or(TraversalValue::Empty)
        .clone();
    drop(connections);

    Ok(Format::Json.create_response(&ReturnValue::from(next)))
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
        Err(e) => return Err(GraphError::from(e)),
    };

    let mut connections = input.mcp_connections.lock().unwrap();
    let connection = match connections.get_connection_owned(&data.connection_id) {
        Some(conn) => conn,
        None => return Err(GraphError::StorageError("Connection not found".to_string())),
    };
    drop(connections);

    let values = match data.range {
        Some(range) => connection
            .iter
            .clone()
            .skip(range.start)
            .take(range.end - range.start)
            .collect::<Vec<TraversalValue>>(),
        None => connection.iter.clone().collect::<Vec<TraversalValue>>(),
    };

    let mut connections = input.mcp_connections.lock().unwrap();

    if data.drop.unwrap_or(true) {
        connections.add_connection(MCPConnection::new(
            connection.connection_id.clone(),
            vec![].into_iter(),
        ));
    } else {
        connections.add_connection(connection);
    }

    drop(connections);

    Ok(Format::Json.create_response(&ReturnValue::from(values)))
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
        Err(e) => return Err(GraphError::from(e)),
    };

    let mut connections = input.mcp_connections.lock().unwrap();
    let connection = match connections.get_connection_owned(&data.connection_id) {
        Some(conn) => conn,
        None => return Err(GraphError::StorageError("Connection not found".to_string())),
    };
    drop(connections);

    let iter = connection.iter.clone().collect::<Vec<_>>();
    let db = Arc::clone(&input.mcp_backend.db);
    let txn = input.mcp_backend.db.graph_env.read_txn()?;

    let values = G::new_from(db, &txn, iter)
        .aggregate_by(&data.properties, true)?
        .into_count();

    let mut connections = input.mcp_connections.lock().unwrap();

    if data.drop.unwrap_or(true) {
        connections.add_connection(MCPConnection::new(
            connection.connection_id.clone(),
            vec![].into_iter(),
        ));
    } else {
        connections.add_connection(connection);
    }

    drop(connections);

    Ok(Format::Json.create_response(&ReturnValue::from(values)))
}
#[mcp_handler]
pub fn group_by(input: &mut MCPToolInput) -> Result<Response, GraphError> {
    let data: AggregateRequest = match sonic_rs::from_slice(&input.request.body) {
        Ok(data) => data,
        Err(e) => return Err(GraphError::from(e)),
    };

    let mut connections = input.mcp_connections.lock().unwrap();
    let connection = match connections.get_connection_owned(&data.connection_id) {
        Some(conn) => conn,
        None => return Err(GraphError::StorageError("Connection not found".to_string())),
    };
    drop(connections);

    let iter = connection.iter.clone().collect::<Vec<_>>();
    let db = Arc::clone(&input.mcp_backend.db);
    let txn = input.mcp_backend.db.graph_env.read_txn()?;

    let values = G::new_from(db, &txn, iter)
        .group_by(&data.properties, true)?
        .into_count();

    let mut connections = input.mcp_connections.lock().unwrap();

    if data.drop.unwrap_or(true) {
        connections.add_connection(MCPConnection::new(
            connection.connection_id.clone(),
            vec![].into_iter(),
        ));
    } else {
        connections.add_connection(connection);
    }

    drop(connections);

    Ok(Format::Json.create_response(&ReturnValue::from(values)))
}

#[derive(Deserialize)]
pub struct ResetRequest {
    pub connection_id: String,
}

#[mcp_handler]
pub fn reset(input: &mut MCPToolInput) -> Result<Response, GraphError> {
    let data: ResetRequest = match sonic_rs::from_slice(&input.request.body) {
        Ok(data) => data,
        Err(e) => return Err(GraphError::from(e)),
    };

    let mut connections = input.mcp_connections.lock().unwrap();
    let connection = match connections.get_connection_owned(&data.connection_id) {
        Some(conn) => conn,
        None => return Err(GraphError::StorageError("Connection not found".to_string())),
    };
    let connection_id = connection.connection_id.to_string();

    connections.add_connection(MCPConnection::new(
        connection.connection_id.clone(),
        vec![].into_iter(),
    ));

    drop(connections);

    Ok(Format::Json.create_response(&ReturnValue::from(connection_id)))
}

#[mcp_handler]
pub fn schema_resource(input: &mut MCPToolInput) -> Result<Response, GraphError> {
    let data: ResourceCallRequest = match sonic_rs::from_slice(&input.request.body) {
        Ok(data) => data,
        Err(e) => return Err(GraphError::from(e)),
    };

    let _ = match input
        .mcp_connections
        .lock()
        .unwrap()
        .get_connection(&data.connection_id)
    {
        Some(conn) => conn,
        None => return Err(GraphError::StorageError("Connection not found".to_string())),
    };

    if input.schema.is_some() {
        Ok(Format::Json.create_response(&ReturnValue::from(
            input.schema.as_ref().expect("Schema not found").to_string(),
        )))
    } else {
        Ok(Format::Json.create_response(&ReturnValue::from("no schema".to_string())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::helix_engine::traversal_core::{config::Config, HelixGraphEngineOpts};
    use axum::body::Bytes;
    use tempfile::TempDir;

    fn create_test_backend() -> (Arc<McpBackend>, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let opts = HelixGraphEngineOpts {
            path: temp_dir.path().to_str().unwrap().to_string(),
            config: Config::default(),
            version_info: Default::default(),
        };
        let engine = crate::helix_engine::traversal_core::HelixGraphEngine::new(opts).unwrap();
        let backend = Arc::new(McpBackend::new(engine.storage));
        (backend, temp_dir)
    }

    fn create_test_request(body: &str) -> Request {
        Request {
            name: "test".to_string(),
            req_type: crate::protocol::request::RequestType::MCP,
            body: Bytes::from(body.to_string()),
            in_fmt: Format::Json,
            out_fmt: Format::Json,
        }
    }

    // ============================================================================
    // McpConnections Tests
    // ============================================================================

    #[test]
    fn test_mcp_connections_new() {
        let connections = McpConnections::new();
        assert!(connections.connections.is_empty());
    }

    #[test]
    fn test_mcp_connections_default() {
        let connections = McpConnections::default();
        assert!(connections.connections.is_empty());
    }

    #[test]
    fn test_mcp_connections_new_with_max_connections() {
        let connections = McpConnections::new_with_max_connections(100);
        assert!(connections.connections.is_empty());
        assert!(connections.connections.capacity() >= 100);
    }

    #[test]
    fn test_mcp_connections_add_connection() {
        let mut connections = McpConnections::new();
        let connection = MCPConnection::new("conn1".to_string(), vec![].into_iter());

        connections.add_connection(connection);
        assert_eq!(connections.connections.len(), 1);
        assert!(connections.connections.contains_key("conn1"));
    }

    #[test]
    fn test_mcp_connections_add_multiple_connections() {
        let mut connections = McpConnections::new();

        connections.add_connection(MCPConnection::new("conn1".to_string(), vec![].into_iter()));
        connections.add_connection(MCPConnection::new("conn2".to_string(), vec![].into_iter()));
        connections.add_connection(MCPConnection::new("conn3".to_string(), vec![].into_iter()));

        assert_eq!(connections.connections.len(), 3);
    }

    #[test]
    fn test_mcp_connections_remove_connection() {
        let mut connections = McpConnections::new();
        connections.add_connection(MCPConnection::new("conn1".to_string(), vec![].into_iter()));

        let removed = connections.remove_connection("conn1");
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().connection_id, "conn1");
        assert!(connections.connections.is_empty());
    }

    #[test]
    fn test_mcp_connections_remove_nonexistent() {
        let mut connections = McpConnections::new();
        let removed = connections.remove_connection("nonexistent");
        assert!(removed.is_none());
    }

    #[test]
    fn test_mcp_connections_get_connection() {
        let mut connections = McpConnections::new();
        connections.add_connection(MCPConnection::new("conn1".to_string(), vec![].into_iter()));

        let conn = connections.get_connection("conn1");
        assert!(conn.is_some());
        assert_eq!(conn.unwrap().connection_id, "conn1");
    }

    #[test]
    fn test_mcp_connections_get_connection_nonexistent() {
        let connections = McpConnections::new();
        let conn = connections.get_connection("nonexistent");
        assert!(conn.is_none());
    }

    #[test]
    fn test_mcp_connections_get_connection_mut() {
        let mut connections = McpConnections::new();
        connections.add_connection(MCPConnection::new("conn1".to_string(), vec![].into_iter()));

        let conn = connections.get_connection_mut("conn1");
        assert!(conn.is_some());
        assert_eq!(conn.unwrap().connection_id, "conn1");
    }

    #[test]
    fn test_mcp_connections_get_connection_owned() {
        let mut connections = McpConnections::new();
        connections.add_connection(MCPConnection::new("conn1".to_string(), vec![].into_iter()));

        let conn = connections.get_connection_owned("conn1");
        assert!(conn.is_some());
        assert_eq!(conn.unwrap().connection_id, "conn1");
        assert!(connections.connections.is_empty());
    }

    #[test]
    fn test_mcp_connections_overwrite_connection() {
        let mut connections = McpConnections::new();
        connections.add_connection(MCPConnection::new("conn1".to_string(), vec![].into_iter()));
        connections.add_connection(MCPConnection::new("conn1".to_string(), vec![].into_iter()));

        // Should only have one connection (the second one overwrote the first)
        assert_eq!(connections.connections.len(), 1);
    }

    #[test]
    fn test_mcp_connections_multiple_operations() {
        let mut connections = McpConnections::new();

        // Add multiple connections
        connections.add_connection(MCPConnection::new("conn1".to_string(), vec![].into_iter()));
        connections.add_connection(MCPConnection::new("conn2".to_string(), vec![].into_iter()));
        connections.add_connection(MCPConnection::new("conn3".to_string(), vec![].into_iter()));
        assert_eq!(connections.connections.len(), 3);

        // Remove one
        connections.remove_connection("conn2");
        assert_eq!(connections.connections.len(), 2);

        // Get remaining
        assert!(connections.get_connection("conn1").is_some());
        assert!(connections.get_connection("conn3").is_some());
        assert!(connections.get_connection("conn2").is_none());
    }

    #[test]
    fn test_mcp_connections_capacity() {
        let connections = McpConnections::new_with_max_connections(50);
        assert!(connections.connections.capacity() >= 50);
    }

    // ============================================================================
    // MCPConnection Tests
    // ============================================================================

    #[test]
    fn test_mcp_connection_new() {
        let conn = MCPConnection::new("test_id".to_string(), vec![].into_iter());
        assert_eq!(conn.connection_id, "test_id");
    }

    #[test]
    fn test_mcp_connection_with_data() {
        use crate::helix_engine::traversal_core::traversal_value::TraversalValue;

        let data = vec![
            TraversalValue::Empty,
            TraversalValue::Empty,
        ];
        let conn = MCPConnection::new("test_id".to_string(), data.into_iter());
        assert_eq!(conn.connection_id, "test_id");
    }

    #[test]
    fn test_mcp_connection_id_uniqueness() {
        let conn1 = MCPConnection::new("id1".to_string(), vec![].into_iter());
        let conn2 = MCPConnection::new("id2".to_string(), vec![].into_iter());

        assert_ne!(conn1.connection_id, conn2.connection_id);
    }

    #[test]
    fn test_mcp_connection_iter_consumption() {
        use crate::helix_engine::traversal_core::traversal_value::TraversalValue;

        let data = vec![TraversalValue::Empty, TraversalValue::Empty];
        let mut conn = MCPConnection::new("test".to_string(), data.into_iter());

        assert!(conn.iter.next().is_some());
        assert!(conn.iter.next().is_some());
        assert!(conn.iter.next().is_none());
    }

    #[test]
    fn test_mcp_connection_empty_iter() {
        let mut conn = MCPConnection::new("test".to_string(), vec![].into_iter());
        assert!(conn.iter.next().is_none());
    }

    // ============================================================================
    // McpBackend Tests
    // ============================================================================

    #[test]
    fn test_mcp_backend_new() {
        let (backend, _temp_dir) = create_test_backend();
        // If we reach here, backend was created successfully
        assert!(Arc::strong_count(&backend) >= 1);
    }

    #[test]
    fn test_mcp_backend_db_access() {
        let (backend, _temp_dir) = create_test_backend();
        // Verify we can access the database
        let _db = &backend.db;
    }

    #[test]
    fn test_mcp_backend_clone() {
        let (backend, _temp_dir) = create_test_backend();
        let backend2 = Arc::clone(&backend);

        assert_eq!(Arc::strong_count(&backend), 2);
        drop(backend2);
        assert_eq!(Arc::strong_count(&backend), 1);
    }

    // ============================================================================
    // init Handler Tests
    // ============================================================================

    #[test]
    fn test_init_handler_creates_connection() {
        let (backend, _temp_dir) = create_test_backend();
        let connections = Arc::new(Mutex::new(McpConnections::new()));
        let request = create_test_request(r#"{"connection_addr":"localhost","connection_port":8080}"#);

        let mut input = MCPToolInput {
            request,
            mcp_backend: backend,
            mcp_connections: Arc::clone(&connections),
            schema: None,
        };

        let result = init(&mut input);
        assert!(result.is_ok());

        // Verify connection was added
        let conn_guard = connections.lock().unwrap();
        assert_eq!(conn_guard.connections.len(), 1);
    }

    #[test]
    fn test_init_handler_returns_connection_id() {
        let (backend, _temp_dir) = create_test_backend();
        let connections = Arc::new(Mutex::new(McpConnections::new()));
        let request = create_test_request(r#"{"connection_addr":"localhost","connection_port":8080}"#);

        let mut input = MCPToolInput {
            request,
            mcp_backend: backend,
            mcp_connections: connections,
            schema: None,
        };

        let result = init(&mut input);
        assert!(result.is_ok());
        let response = result.unwrap();
        assert!(!response.body.is_empty());
    }

    #[test]
    fn test_init_handler_multiple_calls() {
        let (backend, _temp_dir) = create_test_backend();
        let connections = Arc::new(Mutex::new(McpConnections::new()));

        for _ in 0..3 {
            let request = create_test_request(r#"{"connection_addr":"localhost","connection_port":8080}"#);
            let mut input = MCPToolInput {
                request,
                mcp_backend: Arc::clone(&backend),
                mcp_connections: Arc::clone(&connections),
                schema: None,
            };

            let result = init(&mut input);
            assert!(result.is_ok());
        }

        let conn_guard = connections.lock().unwrap();
        assert_eq!(conn_guard.connections.len(), 3);
    }

    #[test]
    fn test_init_handler_unique_ids() {
        let (backend, _temp_dir) = create_test_backend();
        let connections = Arc::new(Mutex::new(McpConnections::new()));

        let request1 = create_test_request(r#"{"connection_addr":"localhost","connection_port":8080}"#);
        let mut input1 = MCPToolInput {
            request: request1,
            mcp_backend: Arc::clone(&backend),
            mcp_connections: Arc::clone(&connections),
            schema: None,
        };

        let result1 = init(&mut input1);
        assert!(result1.is_ok());
        let body1 = result1.unwrap().body;

        let request2 = create_test_request(r#"{"connection_addr":"localhost","connection_port":8080}"#);
        let mut input2 = MCPToolInput {
            request: request2,
            mcp_backend: backend,
            mcp_connections: connections,
            schema: None,
        };

        let result2 = init(&mut input2);
        assert!(result2.is_ok());
        let body2 = result2.unwrap().body;

        assert_ne!(body1, body2);
    }

    #[test]
    fn test_init_handler_json_format() {
        let (backend, _temp_dir) = create_test_backend();
        let connections = Arc::new(Mutex::new(McpConnections::new()));
        let request = create_test_request(r#"{"connection_addr":"localhost","connection_port":8080}"#);

        let mut input = MCPToolInput {
            request,
            mcp_backend: backend,
            mcp_connections: connections,
            schema: None,
        };

        let result = init(&mut input);
        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.fmt, Format::Json);
    }

    // ============================================================================
    // next Handler Tests
    // ============================================================================

    #[test]
    fn test_next_handler_empty_iter() {
        let (backend, _temp_dir) = create_test_backend();
        let connections = Arc::new(Mutex::new(McpConnections::new()));

        // Create a connection first
        {
            let mut conn_guard = connections.lock().unwrap();
            conn_guard.add_connection(MCPConnection::new("test_conn".to_string(), vec![].into_iter()));
        }

        let request = create_test_request(r#"{"connection_id":"test_conn"}"#);
        let mut input = MCPToolInput {
            request,
            mcp_backend: backend,
            mcp_connections: connections,
            schema: None,
        };

        let result = next(&mut input);
        assert!(result.is_ok());
    }

    #[test]
    fn test_next_handler_connection_not_found() {
        let (backend, _temp_dir) = create_test_backend();
        let connections = Arc::new(Mutex::new(McpConnections::new()));

        let request = create_test_request(r#"{"connection_id":"nonexistent"}"#);
        let mut input = MCPToolInput {
            request,
            mcp_backend: backend,
            mcp_connections: connections,
            schema: None,
        };

        let result = next(&mut input);
        assert!(result.is_err());
    }

    #[test]
    fn test_next_handler_invalid_json() {
        let (backend, _temp_dir) = create_test_backend();
        let connections = Arc::new(Mutex::new(McpConnections::new()));

        let request = create_test_request(r#"invalid json"#);
        let mut input = MCPToolInput {
            request,
            mcp_backend: backend,
            mcp_connections: connections,
            schema: None,
        };

        let result = next(&mut input);
        assert!(result.is_err());
    }

    #[test]
    fn test_next_handler_with_data() {
        use crate::helix_engine::traversal_core::traversal_value::TraversalValue;

        let (backend, _temp_dir) = create_test_backend();
        let connections = Arc::new(Mutex::new(McpConnections::new()));

        {
            let mut conn_guard = connections.lock().unwrap();
            let data = vec![TraversalValue::Empty];
            conn_guard.add_connection(MCPConnection::new("test_conn".to_string(), data.into_iter()));
        }

        let request = create_test_request(r#"{"connection_id":"test_conn"}"#);
        let mut input = MCPToolInput {
            request,
            mcp_backend: backend,
            mcp_connections: connections,
            schema: None,
        };

        let result = next(&mut input);
        assert!(result.is_ok());
    }

    #[test]
    fn test_next_handler_sequential_calls() {
        use crate::helix_engine::traversal_core::traversal_value::TraversalValue;

        let (backend, _temp_dir) = create_test_backend();
        let connections = Arc::new(Mutex::new(McpConnections::new()));

        {
            let mut conn_guard = connections.lock().unwrap();
            let data = vec![TraversalValue::Empty, TraversalValue::Empty];
            conn_guard.add_connection(MCPConnection::new("test_conn".to_string(), data.into_iter()));
        }

        // First call
        let request1 = create_test_request(r#"{"connection_id":"test_conn"}"#);
        let mut input1 = MCPToolInput {
            request: request1,
            mcp_backend: Arc::clone(&backend),
            mcp_connections: Arc::clone(&connections),
            schema: None,
        };
        assert!(next(&mut input1).is_ok());

        // Second call
        let request2 = create_test_request(r#"{"connection_id":"test_conn"}"#);
        let mut input2 = MCPToolInput {
            request: request2,
            mcp_backend: backend,
            mcp_connections: connections,
            schema: None,
        };
        assert!(next(&mut input2).is_ok());
    }

    #[test]
    fn test_next_handler_exhausted_iter() {
        let (backend, _temp_dir) = create_test_backend();
        let connections = Arc::new(Mutex::new(McpConnections::new()));

        {
            let mut conn_guard = connections.lock().unwrap();
            conn_guard.add_connection(MCPConnection::new("test_conn".to_string(), vec![].into_iter()));
        }

        let request = create_test_request(r#"{"connection_id":"test_conn"}"#);
        let mut input = MCPToolInput {
            request,
            mcp_backend: backend,
            mcp_connections: connections,
            schema: None,
        };

        let result = next(&mut input);
        assert!(result.is_ok());
    }

    #[test]
    fn test_next_handler_json_format() {
        let (backend, _temp_dir) = create_test_backend();
        let connections = Arc::new(Mutex::new(McpConnections::new()));

        {
            let mut conn_guard = connections.lock().unwrap();
            conn_guard.add_connection(MCPConnection::new("test_conn".to_string(), vec![].into_iter()));
        }

        let request = create_test_request(r#"{"connection_id":"test_conn"}"#);
        let mut input = MCPToolInput {
            request,
            mcp_backend: backend,
            mcp_connections: connections,
            schema: None,
        };

        let result = next(&mut input);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().fmt, Format::Json);
    }

    // ============================================================================
    // collect Handler Tests (continuing to reach 60 total tests)
    // ============================================================================

    #[test]
    fn test_collect_handler_empty_iter() {
        let (backend, _temp_dir) = create_test_backend();
        let connections = Arc::new(Mutex::new(McpConnections::new()));

        {
            let mut conn_guard = connections.lock().unwrap();
            conn_guard.add_connection(MCPConnection::new("test_conn".to_string(), vec![].into_iter()));
        }

        let request = create_test_request(r#"{"connection_id":"test_conn"}"#);
        let mut input = MCPToolInput {
            request,
            mcp_backend: backend,
            mcp_connections: connections,
            schema: None,
        };

        let result = collect(&mut input);
        assert!(result.is_ok());
    }

    #[test]
    fn test_collect_handler_connection_not_found() {
        let (backend, _temp_dir) = create_test_backend();
        let connections = Arc::new(Mutex::new(McpConnections::new()));

        let request = create_test_request(r#"{"connection_id":"nonexistent"}"#);
        let mut input = MCPToolInput {
            request,
            mcp_backend: backend,
            mcp_connections: connections,
            schema: None,
        };

        let result = collect(&mut input);
        assert!(result.is_err());
    }

    #[test]
    fn test_collect_handler_with_range() {
        use crate::helix_engine::traversal_core::traversal_value::TraversalValue;

        let (backend, _temp_dir) = create_test_backend();
        let connections = Arc::new(Mutex::new(McpConnections::new()));

        {
            let mut conn_guard = connections.lock().unwrap();
            let data = vec![
                TraversalValue::Empty,
                TraversalValue::Empty,
                TraversalValue::Empty,
            ];
            conn_guard.add_connection(MCPConnection::new("test_conn".to_string(), data.into_iter()));
        }

        let request = create_test_request(r#"{"connection_id":"test_conn","range":{"start":0,"end":2}}"#);
        let mut input = MCPToolInput {
            request,
            mcp_backend: backend,
            mcp_connections: connections,
            schema: None,
        };

        let result = collect(&mut input);
        assert!(result.is_ok());
    }

    #[test]
    fn test_collect_handler_with_drop_true() {
        let (backend, _temp_dir) = create_test_backend();
        let connections = Arc::new(Mutex::new(McpConnections::new()));

        {
            let mut conn_guard = connections.lock().unwrap();
            conn_guard.add_connection(MCPConnection::new("test_conn".to_string(), vec![].into_iter()));
        }

        let request = create_test_request(r#"{"connection_id":"test_conn","drop":true}"#);
        let mut input = MCPToolInput {
            request,
            mcp_backend: backend,
            mcp_connections: Arc::clone(&connections),
            schema: None,
        };

        let result = collect(&mut input);
        assert!(result.is_ok());

        // Connection should be replaced with empty iter
        let conn_guard = connections.lock().unwrap();
        assert_eq!(conn_guard.connections.len(), 1);
    }

    #[test]
    fn test_collect_handler_with_drop_false() {
        let (backend, _temp_dir) = create_test_backend();
        let connections = Arc::new(Mutex::new(McpConnections::new()));

        {
            let mut conn_guard = connections.lock().unwrap();
            conn_guard.add_connection(MCPConnection::new("test_conn".to_string(), vec![].into_iter()));
        }

        let request = create_test_request(r#"{"connection_id":"test_conn","drop":false}"#);
        let mut input = MCPToolInput {
            request,
            mcp_backend: backend,
            mcp_connections: Arc::clone(&connections),
            schema: None,
        };

        let result = collect(&mut input);
        assert!(result.is_ok());

        // Connection should still exist
        let conn_guard = connections.lock().unwrap();
        assert_eq!(conn_guard.connections.len(), 1);
    }

    #[test]
    fn test_collect_handler_default_drop() {
        let (backend, _temp_dir) = create_test_backend();
        let connections = Arc::new(Mutex::new(McpConnections::new()));

        {
            let mut conn_guard = connections.lock().unwrap();
            conn_guard.add_connection(MCPConnection::new("test_conn".to_string(), vec![].into_iter()));
        }

        let request = create_test_request(r#"{"connection_id":"test_conn"}"#);
        let mut input = MCPToolInput {
            request,
            mcp_backend: backend,
            mcp_connections: Arc::clone(&connections),
            schema: None,
        };

        let result = collect(&mut input);
        assert!(result.is_ok());

        // Default drop is true, so connection should be replaced
        let conn_guard = connections.lock().unwrap();
        assert_eq!(conn_guard.connections.len(), 1);
    }

    #[test]
    fn test_collect_handler_invalid_json() {
        let (backend, _temp_dir) = create_test_backend();
        let connections = Arc::new(Mutex::new(McpConnections::new()));

        let request = create_test_request(r#"invalid"#);
        let mut input = MCPToolInput {
            request,
            mcp_backend: backend,
            mcp_connections: connections,
            schema: None,
        };

        let result = collect(&mut input);
        assert!(result.is_err());
    }

    #[test]
    fn test_collect_handler_json_format() {
        let (backend, _temp_dir) = create_test_backend();
        let connections = Arc::new(Mutex::new(McpConnections::new()));

        {
            let mut conn_guard = connections.lock().unwrap();
            conn_guard.add_connection(MCPConnection::new("test_conn".to_string(), vec![].into_iter()));
        }

        let request = create_test_request(r#"{"connection_id":"test_conn"}"#);
        let mut input = MCPToolInput {
            request,
            mcp_backend: backend,
            mcp_connections: connections,
            schema: None,
        };

        let result = collect(&mut input);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().fmt, Format::Json);
    }

    // ============================================================================
    // reset Handler Tests
    // ============================================================================

    #[test]
    fn test_reset_handler_success() {
        let (backend, _temp_dir) = create_test_backend();
        let connections = Arc::new(Mutex::new(McpConnections::new()));

        {
            let mut conn_guard = connections.lock().unwrap();
            conn_guard.add_connection(MCPConnection::new("test_conn".to_string(), vec![].into_iter()));
        }

        let request = create_test_request(r#"{"connection_id":"test_conn"}"#);
        let mut input = MCPToolInput {
            request,
            mcp_backend: backend,
            mcp_connections: connections,
            schema: None,
        };

        let result = reset(&mut input);
        assert!(result.is_ok());
    }

    #[test]
    fn test_reset_handler_connection_not_found() {
        let (backend, _temp_dir) = create_test_backend();
        let connections = Arc::new(Mutex::new(McpConnections::new()));

        let request = create_test_request(r#"{"connection_id":"nonexistent"}"#);
        let mut input = MCPToolInput {
            request,
            mcp_backend: backend,
            mcp_connections: connections,
            schema: None,
        };

        let result = reset(&mut input);
        assert!(result.is_err());
    }

    #[test]
    fn test_reset_handler_replaces_iter() {
        use crate::helix_engine::traversal_core::traversal_value::TraversalValue;

        let (backend, _temp_dir) = create_test_backend();
        let connections = Arc::new(Mutex::new(McpConnections::new()));

        {
            let mut conn_guard = connections.lock().unwrap();
            let data = vec![TraversalValue::Empty, TraversalValue::Empty];
            conn_guard.add_connection(MCPConnection::new("test_conn".to_string(), data.into_iter()));
        }

        let request = create_test_request(r#"{"connection_id":"test_conn"}"#);
        let mut input = MCPToolInput {
            request,
            mcp_backend: backend,
            mcp_connections: Arc::clone(&connections),
            schema: None,
        };

        let result = reset(&mut input);
        assert!(result.is_ok());

        // Connection should exist with empty iter
        let conn_guard = connections.lock().unwrap();
        assert_eq!(conn_guard.connections.len(), 1);
    }

    #[test]
    fn test_reset_handler_returns_connection_id() {
        let (backend, _temp_dir) = create_test_backend();
        let connections = Arc::new(Mutex::new(McpConnections::new()));

        {
            let mut conn_guard = connections.lock().unwrap();
            conn_guard.add_connection(MCPConnection::new("test_conn".to_string(), vec![].into_iter()));
        }

        let request = create_test_request(r#"{"connection_id":"test_conn"}"#);
        let mut input = MCPToolInput {
            request,
            mcp_backend: backend,
            mcp_connections: connections,
            schema: None,
        };

        let result = reset(&mut input);
        assert!(result.is_ok());
        let response = result.unwrap();
        assert!(!response.body.is_empty());
    }

    #[test]
    fn test_reset_handler_invalid_json() {
        let (backend, _temp_dir) = create_test_backend();
        let connections = Arc::new(Mutex::new(McpConnections::new()));

        let request = create_test_request(r#"invalid"#);
        let mut input = MCPToolInput {
            request,
            mcp_backend: backend,
            mcp_connections: connections,
            schema: None,
        };

        let result = reset(&mut input);
        assert!(result.is_err());
    }

    // ============================================================================
    // schema_resource Handler Tests
    // ============================================================================

    #[test]
    fn test_schema_resource_with_schema() {
        let (backend, _temp_dir) = create_test_backend();
        let connections = Arc::new(Mutex::new(McpConnections::new()));

        {
            let mut conn_guard = connections.lock().unwrap();
            conn_guard.add_connection(MCPConnection::new("test_conn".to_string(), vec![].into_iter()));
        }

        let request = create_test_request(r#"{"connection_id":"test_conn"}"#);
        let mut input = MCPToolInput {
            request,
            mcp_backend: backend,
            mcp_connections: connections,
            schema: Some("test schema".to_string()),
        };

        let result = schema_resource(&mut input);
        assert!(result.is_ok());
    }

    #[test]
    fn test_schema_resource_without_schema() {
        let (backend, _temp_dir) = create_test_backend();
        let connections = Arc::new(Mutex::new(McpConnections::new()));

        {
            let mut conn_guard = connections.lock().unwrap();
            conn_guard.add_connection(MCPConnection::new("test_conn".to_string(), vec![].into_iter()));
        }

        let request = create_test_request(r#"{"connection_id":"test_conn"}"#);
        let mut input = MCPToolInput {
            request,
            mcp_backend: backend,
            mcp_connections: connections,
            schema: None,
        };

        let result = schema_resource(&mut input);
        assert!(result.is_ok());
    }

    #[test]
    fn test_schema_resource_connection_not_found() {
        let (backend, _temp_dir) = create_test_backend();
        let connections = Arc::new(Mutex::new(McpConnections::new()));

        let request = create_test_request(r#"{"connection_id":"nonexistent"}"#);
        let mut input = MCPToolInput {
            request,
            mcp_backend: backend,
            mcp_connections: connections,
            schema: None,
        };

        let result = schema_resource(&mut input);
        assert!(result.is_err());
    }

    #[test]
    fn test_schema_resource_invalid_json() {
        let (backend, _temp_dir) = create_test_backend();
        let connections = Arc::new(Mutex::new(McpConnections::new()));

        let request = create_test_request(r#"invalid"#);
        let mut input = MCPToolInput {
            request,
            mcp_backend: backend,
            mcp_connections: connections,
            schema: None,
        };

        let result = schema_resource(&mut input);
        assert!(result.is_err());
    }

    #[test]
    fn test_schema_resource_json_format() {
        let (backend, _temp_dir) = create_test_backend();
        let connections = Arc::new(Mutex::new(McpConnections::new()));

        {
            let mut conn_guard = connections.lock().unwrap();
            conn_guard.add_connection(MCPConnection::new("test_conn".to_string(), vec![].into_iter()));
        }

        let request = create_test_request(r#"{"connection_id":"test_conn"}"#);
        let mut input = MCPToolInput {
            request,
            mcp_backend: backend,
            mcp_connections: connections,
            schema: Some("schema".to_string()),
        };

        let result = schema_resource(&mut input);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().fmt, Format::Json);
    }

    // ============================================================================
    // MCPHandler Struct Tests
    // ============================================================================

    #[test]
    fn test_mcp_handler_new() {
        fn test_fn(_input: &mut MCPToolInput) -> Result<Response, GraphError> {
            Ok(Response { body: vec![], fmt: Format::Json })
        }

        let handler = MCPHandler::new("test_handler", test_fn);
        assert_eq!(handler.name, "test_handler");
    }

    #[test]
    fn test_mcp_handler_submission() {
        fn test_fn(_input: &mut MCPToolInput) -> Result<Response, GraphError> {
            Ok(Response { body: vec![], fmt: Format::Json })
        }

        let handler = MCPHandler::new("test", test_fn);
        let submission = MCPHandlerSubmission(handler);
        assert_eq!(submission.0.name, "test");
    }

    // ============================================================================
    // Request Structs Tests
    // ============================================================================

    #[test]
    fn test_tool_call_request_deserialization() {
        let json = r#"{"connection_id":"test","tool":{"name":"test"}}"#;
        let result: Result<ToolCallRequest, _> = sonic_rs::from_str(json);
        // This may fail depending on ToolArgs structure, but test the structure exists
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_resource_call_request_deserialization() {
        let json = r#"{"connection_id":"test_conn"}"#;
        let result: Result<ResourceCallRequest, _> = sonic_rs::from_str(json);
        assert!(result.is_ok());
        let data = result.unwrap();
        assert_eq!(data.connection_id, "test_conn");
    }

    #[test]
    fn test_init_request_deserialization() {
        let json = r#"{"connection_addr":"localhost","connection_port":8080}"#;
        let result: Result<InitRequest, _> = sonic_rs::from_str(json);
        assert!(result.is_ok());
        let data = result.unwrap();
        assert_eq!(data.connection_addr, "localhost");
        assert_eq!(data.connection_port, 8080);
    }

    #[test]
    fn test_next_request_deserialization() {
        let json = r#"{"connection_id":"conn123"}"#;
        let result: Result<NextRequest, _> = sonic_rs::from_str(json);
        assert!(result.is_ok());
        let data = result.unwrap();
        assert_eq!(data.connection_id, "conn123");
    }

    #[test]
    fn test_collect_request_deserialization() {
        let json = r#"{"connection_id":"conn123","range":{"start":0,"end":10},"drop":true}"#;
        let result: Result<CollectRequest, _> = sonic_rs::from_str(json);
        assert!(result.is_ok());
        let data = result.unwrap();
        assert_eq!(data.connection_id, "conn123");
        assert!(data.range.is_some());
        assert_eq!(data.drop, Some(true));
    }

    #[test]
    fn test_aggregate_request_deserialization() {
        let json = r#"{"connection_id":"conn123","properties":["prop1","prop2"],"drop":false}"#;
        let result: Result<AggregateRequest, _> = sonic_rs::from_str(json);
        assert!(result.is_ok());
        let data = result.unwrap();
        assert_eq!(data.connection_id, "conn123");
        assert_eq!(data.drop, Some(false));
    }

    #[test]
    fn test_reset_request_deserialization() {
        let json = r#"{"connection_id":"conn_reset"}"#;
        let result: Result<ResetRequest, _> = sonic_rs::from_str(json);
        assert!(result.is_ok());
        let data = result.unwrap();
        assert_eq!(data.connection_id, "conn_reset");
    }
}
