// router

// takes in raw [u8] data
// parses to request type

// then locks graph and passes parsed data and graph to handler to execute query

// returns response

use crate::{
    helix_engine::{traversal_core::HelixGraphEngine, types::GraphError},
    helix_gateway::mcp::mcp::MCPHandlerFn,
    protocol::request::RetChan,
};
use core::fmt;
use std::{collections::HashMap, fmt::Debug, future::Future, pin::Pin, sync::Arc};

use crate::protocol::{Request, Response};

pub struct HandlerInput {
    pub request: Request,
    pub graph: Arc<HelixGraphEngine>,
}

pub type ContMsg = (
    RetChan,
    Box<dyn FnOnce() -> Result<Response, GraphError> + Send + Sync>,
);
pub type ContChan = flume::Sender<ContMsg>;

pub type ContFut = Pin<Box<dyn Future<Output = ()> + Send + Sync>>;

pub struct IoContFn(pub Box<dyn FnOnce(ContChan, RetChan) -> ContFut + Send + Sync>);

impl IoContFn {
    pub fn create_err<F>(func: F) -> GraphError
    where
        F: FnOnce(ContChan, RetChan) -> ContFut + Send + Sync + 'static,
    {
        GraphError::IoNeeded(Self(Box::new(func)))
    }
}

impl Debug for IoContFn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Asyncronous IO is needed to complete the DB operation")
    }
}



// basic type for function pointer
pub type BasicHandlerFn = fn(HandlerInput) -> Result<Response, GraphError>;

// thread safe type for multi threaded use
pub type HandlerFn = Arc<dyn Fn(HandlerInput) -> Result<Response, GraphError> + Send + Sync>;

#[derive(Clone, Debug)]
pub struct HandlerSubmission(pub Handler);

#[derive(Clone, Debug)]
pub struct Handler {
    pub name: &'static str,
    pub func: BasicHandlerFn,
}

impl Handler {
    pub const fn new(name: &'static str, func: BasicHandlerFn) -> Self {
        Self { name, func }
    }
}

inventory::collect!(HandlerSubmission);

/// Router for handling requests and MCP requests
///
/// Standard Routes and MCP Routes are stored in a HashMap with the method and path as the key
pub struct HelixRouter {
    /// Name => Function
    pub routes: HashMap<String, HandlerFn>,
    pub mcp_routes: HashMap<String, MCPHandlerFn>,
}

impl HelixRouter {
    /// Create a new router with a set of routes
    pub fn new(
        routes: Option<HashMap<String, HandlerFn>>,
        mcp_routes: Option<HashMap<String, MCPHandlerFn>>,
    ) -> Self {
        let rts = routes.unwrap_or_default();
        let mcp_rts = mcp_routes.unwrap_or_default();
        Self {
            routes: rts,
            mcp_routes: mcp_rts,
        }
    }

    /// Add a route to the router
    pub fn add_route(&mut self, name: &str, handler: BasicHandlerFn) {
        self.routes.insert(name.to_string(), Arc::new(handler));
    }
}

#[derive(Debug)]
pub enum RouterError {
    Io(std::io::Error),
    New(String),
}

impl fmt::Display for RouterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RouterError::Io(e) => write!(f, "IO error: {e}"),
            RouterError::New(msg) => write!(f, "Graph error: {msg}"),
        }
    }
}

impl From<String> for RouterError {
    fn from(error: String) -> Self {
        RouterError::New(error)
    }
}

impl From<std::io::Error> for RouterError {
    fn from(error: std::io::Error) -> Self {
        RouterError::Io(error)
    }
}

impl From<GraphError> for RouterError {
    fn from(error: GraphError) -> Self {
        RouterError::New(error.to_string())
    }
}

impl From<RouterError> for GraphError {
    fn from(error: RouterError) -> Self {
        GraphError::New(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        helix_engine::traversal_core::{config::Config, HelixGraphEngineOpts},
        protocol::{request::RequestType, Format},
    };
    use axum::body::Bytes;
    use tempfile::TempDir;

    fn create_test_graph() -> (Arc<HelixGraphEngine>, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let opts = HelixGraphEngineOpts {
            path: temp_dir.path().to_str().unwrap().to_string(),
            config: Config::default(),
            version_info: Default::default(),
        };
        let graph = Arc::new(HelixGraphEngine::new(opts).unwrap());
        (graph, temp_dir)
    }

    fn test_handler(_input: HandlerInput) -> Result<Response, GraphError> {
        Ok(Response {
            body: b"test response".to_vec(),
            fmt: Format::Json,
        })
    }

    fn error_handler(_input: HandlerInput) -> Result<Response, GraphError> {
        Err(GraphError::New("test error".to_string()))
    }

    fn echo_handler(input: HandlerInput) -> Result<Response, GraphError> {
        Ok(Response {
            body: input.request.name.as_bytes().to_vec(),
            fmt: Format::Json,
        })
    }

    // ============================================================================
    // Router Creation Tests
    // ============================================================================

    #[test]
    fn test_router_new_empty() {
        let router = HelixRouter::new(None, None);
        assert!(router.routes.is_empty());
        assert!(router.mcp_routes.is_empty());
    }

    #[test]
    fn test_router_new_with_routes() {
        let mut routes = HashMap::new();
        routes.insert("test".to_string(), Arc::new(test_handler) as HandlerFn);

        let router = HelixRouter::new(Some(routes), None);
        assert_eq!(router.routes.len(), 1);
        assert!(router.routes.contains_key("test"));
        assert!(router.mcp_routes.is_empty());
    }

    #[test]
    fn test_router_new_with_multiple_routes() {
        let mut routes = HashMap::new();
        routes.insert("route1".to_string(), Arc::new(test_handler) as HandlerFn);
        routes.insert("route2".to_string(), Arc::new(error_handler) as HandlerFn);
        routes.insert("route3".to_string(), Arc::new(echo_handler) as HandlerFn);

        let router = HelixRouter::new(Some(routes), None);
        assert_eq!(router.routes.len(), 3);
        assert!(router.routes.contains_key("route1"));
        assert!(router.routes.contains_key("route2"));
        assert!(router.routes.contains_key("route3"));
    }

    // ============================================================================
    // Route Addition Tests
    // ============================================================================

    #[test]
    fn test_add_route() {
        let mut router = HelixRouter::new(None, None);
        router.add_route("test", test_handler);

        assert_eq!(router.routes.len(), 1);
        assert!(router.routes.contains_key("test"));
    }

    #[test]
    fn test_add_multiple_routes() {
        let mut router = HelixRouter::new(None, None);
        router.add_route("route1", test_handler);
        router.add_route("route2", error_handler);
        router.add_route("route3", echo_handler);

        assert_eq!(router.routes.len(), 3);
        assert!(router.routes.contains_key("route1"));
        assert!(router.routes.contains_key("route2"));
        assert!(router.routes.contains_key("route3"));
    }

    #[test]
    fn test_add_route_overwrites_existing() {
        let mut router = HelixRouter::new(None, None);
        router.add_route("test", test_handler);
        router.add_route("test", error_handler);

        assert_eq!(router.routes.len(), 1);
        assert!(router.routes.contains_key("test"));
    }

    #[test]
    fn test_add_route_with_special_characters() {
        let mut router = HelixRouter::new(None, None);
        router.add_route("/api/v1/query", test_handler);
        router.add_route("user:detail", test_handler);
        router.add_route("test-route", test_handler);

        assert_eq!(router.routes.len(), 3);
        assert!(router.routes.contains_key("/api/v1/query"));
        assert!(router.routes.contains_key("user:detail"));
        assert!(router.routes.contains_key("test-route"));
    }

    // ============================================================================
    // Handler Invocation Tests
    // ============================================================================

    #[test]
    fn test_handler_invocation_success() {
        let (graph, _temp_dir) = create_test_graph();
        let mut router = HelixRouter::new(None, None);
        router.add_route("test", test_handler);

        let handler = router.routes.get("test").unwrap();
        let input = HandlerInput {
            request: Request {
                name: "test".to_string(),
                req_type: RequestType::Query,
                body: Bytes::new(),
                in_fmt: Format::Json,
                out_fmt: Format::Json,
            },
            graph: graph.clone(),
        };

        let result = handler(input);
        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.body, b"test response");
    }

    #[test]
    fn test_handler_invocation_error() {
        let (graph, _temp_dir) = create_test_graph();
        let mut router = HelixRouter::new(None, None);
        router.add_route("error", error_handler);

        let handler = router.routes.get("error").unwrap();
        let input = HandlerInput {
            request: Request {
                name: "error".to_string(),
                req_type: RequestType::Query,
                body: Bytes::new(),
                in_fmt: Format::Json,
                out_fmt: Format::Json,
            },
            graph: graph.clone(),
        };

        let result = handler(input);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("test error"));
    }

    #[test]
    fn test_handler_invocation_echo() {
        let (graph, _temp_dir) = create_test_graph();
        let mut router = HelixRouter::new(None, None);
        router.add_route("echo", echo_handler);

        let handler = router.routes.get("echo").unwrap();
        let input = HandlerInput {
            request: Request {
                name: "test_path".to_string(),
                req_type: RequestType::Query,
                body: Bytes::new(),
                in_fmt: Format::Json,
                out_fmt: Format::Json,
            },
            graph: graph.clone(),
        };

        let result = handler(input);
        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.body, b"test_path");
    }

    #[test]
    fn test_route_not_found() {
        let router = HelixRouter::new(None, None);
        assert!(router.routes.get("nonexistent").is_none());
    }

    // ============================================================================
    // Handler Input Tests
    // ============================================================================

    #[test]
    fn test_handler_input_creation() {
        let (graph, _temp_dir) = create_test_graph();
        let input = HandlerInput {
            request: Request {
                name: "test".to_string(),
                req_type: RequestType::Query,
                body: Bytes::new(),
                in_fmt: Format::Json,
                out_fmt: Format::Json,
            },
            graph: graph.clone(),
        };

        assert_eq!(input.request.name, "test");
        assert!(input.request.body.is_empty());
    }

    #[test]
    fn test_handler_input_with_body() {
        let (graph, _temp_dir) = create_test_graph();
        let body_data = vec![1, 2, 3, 4];
        let input = HandlerInput {
            request: Request {
                name: "query".to_string(),
                req_type: RequestType::Query,
                body: Bytes::from(body_data.clone()),
                in_fmt: Format::Json,
                out_fmt: Format::Json,
            },
            graph: graph.clone(),
        };

        assert_eq!(input.request.name, "query");
        assert_eq!(input.request.body, Bytes::from(body_data));
    }

    // ============================================================================
    // Router Error Tests
    // ============================================================================

    #[test]
    fn test_router_error_display() {
        let error = RouterError::New("test error message".to_string());
        assert_eq!(error.to_string(), "Graph error: test error message");
    }

    #[test]
    fn test_router_error_from_string() {
        let error: RouterError = "test error".to_string().into();
        assert!(matches!(error, RouterError::New(_)));
    }

    #[test]
    fn test_router_error_to_graph_error() {
        let router_error = RouterError::New("router error".to_string());
        let graph_error: GraphError = router_error.into();
        assert!(graph_error.to_string().contains("router error"));
    }

    #[test]
    fn test_graph_error_to_router_error() {
        let graph_error = GraphError::New("graph error".to_string());
        let router_error: RouterError = graph_error.into();
        assert!(router_error.to_string().contains("graph error"));
    }

    // ============================================================================
    // Handler Struct Tests
    // ============================================================================

    #[test]
    fn test_handler_creation() {
        let handler = Handler::new("test_handler", test_handler);
        assert_eq!(handler.name, "test_handler");
    }

    #[test]
    fn test_handler_submission_creation() {
        let handler = Handler::new("test", test_handler);
        let submission = HandlerSubmission(handler);
        assert_eq!(submission.0.name, "test");
    }

    #[test]
    fn test_router_new_with_mcp_routes() {
        let routes = HashMap::new();
        let router = HelixRouter::new(Some(routes), None);
        assert!(router.routes.is_empty());
        assert!(router.mcp_routes.is_empty());
    }
}
