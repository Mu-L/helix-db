use std::sync::LazyLock;
use std::sync::atomic::{self, AtomicUsize};
use std::time::Instant;
use std::{collections::HashMap, sync::Arc};

use axum::body::Body;
use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use core_affinity::CoreId;
use helix_metrics::events::{EventType, QueryErrorEvent, QuerySuccessEvent};
use sonic_rs::json;
use tracing::{info, trace, warn};

use super::router::router::{HandlerFn, HelixRouter};
#[cfg(feature = "dev-instance")]
use crate::helix_gateway::builtin::all_nodes_and_edges::nodes_edges_handler;
#[cfg(feature = "dev-instance")]
use crate::helix_gateway::builtin::node_by_id::node_details_handler;
#[cfg(feature = "dev-instance")]
use crate::helix_gateway::builtin::node_connections::node_connections_handler;
#[cfg(feature = "dev-instance")]
use crate::helix_gateway::builtin::nodes_by_label::nodes_by_label_handler;
use crate::helix_gateway::introspect_schema::introspect_schema_handler;
use crate::helix_gateway::worker_pool::WorkerPool;
use crate::protocol;
use crate::{
    helix_engine::traversal_core::{HelixGraphEngine, HelixGraphEngineOpts},
    helix_gateway::mcp::mcp::MCPHandlerFn,
};

pub struct GatewayOpts {}

impl GatewayOpts {
    pub const DEFAULT_WORKERS_PER_CORE: usize = 5;
}

pub static HELIX_METRICS_CLIENT: LazyLock<helix_metrics::HelixMetricsClient> =
    LazyLock::new(helix_metrics::HelixMetricsClient::new);

pub struct HelixGateway {
    address: String,
    workers_per_core: usize,
    graph_access: Arc<HelixGraphEngine>,
    router: Arc<HelixRouter>,
    opts: Option<HelixGraphEngineOpts>,
    cluster_id: Option<String>,
}

impl HelixGateway {
    pub fn new(
        address: &str,
        graph_access: Arc<HelixGraphEngine>,
        workers_per_core: usize,
        routes: Option<HashMap<String, HandlerFn>>,
        mcp_routes: Option<HashMap<String, MCPHandlerFn>>,
        opts: Option<HelixGraphEngineOpts>,
    ) -> HelixGateway {
        let router = Arc::new(HelixRouter::new(routes, mcp_routes));
        let cluster_id = std::env::var("CLUSTER_ID").ok();
        HelixGateway {
            address: address.to_string(),
            graph_access,
            router,
            workers_per_core,
            opts,
            cluster_id,
        }
    }

    pub fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        trace!("Starting Helix Gateway");

        let all_core_ids = core_affinity::get_core_ids().expect("unable to get core IDs");

        let tokio_core_ids = all_core_ids.clone();
        let tokio_core_setter = Arc::new(CoreSetter::new(tokio_core_ids, 1));

        let rt = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(tokio_core_setter.num_threads())
                .on_thread_start(move || Arc::clone(&tokio_core_setter).set_current())
                .enable_all()
                .build()?,
        );

        let worker_core_ids = all_core_ids.clone();
        let worker_core_setter = Arc::new(CoreSetter::new(worker_core_ids, self.workers_per_core));

        let worker_pool = WorkerPool::new(
            worker_core_setter,
            Arc::clone(&self.graph_access),
            Arc::clone(&self.router),
            Arc::clone(&rt),
        );

        let mut axum_app = axum::Router::new();

        axum_app = axum_app
            .route("/{*path}", post(post_handler))
            .route("/introspect", get(introspect_schema_handler));

        #[cfg(feature = "dev-instance")]
        {
            axum_app = axum_app
                .route("/nodes-edges", get(nodes_edges_handler))
                .route("/nodes-by-label", get(nodes_by_label_handler))
                .route("/node-connections", get(node_connections_handler))
                .route("/node-details", get(node_details_handler));
        }

        let axum_app = axum_app.with_state(Arc::new(AppState {
            worker_pool,
            schema_json: self.opts.and_then(|o| o.config.schema),
            cluster_id: self.cluster_id,
        }));

        rt.block_on(async move {
            let listener = tokio::net::TcpListener::bind(self.address)
                .await
                .expect("Failed to bind listener");
            info!("Listener has been bound, starting server");
            axum::serve(listener, axum_app)
                .with_graceful_shutdown(shutdown_signal())
                .await
                .expect("Failed to serve")
        });

        Ok(())
    }
}

async fn shutdown_signal() {
    // Respond to either Ctrl-C (SIGINT) or SIGTERM (e.g. `kill` or systemd stop)
    #[cfg(unix)]
    {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                info!("Received Ctrl-C, starting graceful shutdown…");
            }
            _ = sigterm() => {
                info!("Received SIGTERM, starting graceful shutdown…");
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
        info!("Received Ctrl-C, starting graceful shutdown…");
    }
}

#[cfg(unix)]
async fn sigterm() {
    use tokio::signal::unix::{SignalKind, signal};
    let mut term = signal(SignalKind::terminate()).expect("install SIGTERM handler");
    term.recv().await;
}

async fn post_handler(
    State(state): State<Arc<AppState>>,
    req: protocol::request::Request,
) -> axum::http::Response<Body> {
    // #[cfg(feature = "metrics")]
    let start_time = Instant::now();
    let body = req.body.to_vec();
    let query_name = req.name.clone();
    let res = state.worker_pool.process(req).await;

    match res {
        Ok(r) => {
            // #[cfg(feature = "metrics")]
            {
                HELIX_METRICS_CLIENT.send_event(
                    EventType::QuerySuccess,
                    QuerySuccessEvent {
                        cluster_id: state.cluster_id.clone(),
                        query_name,
                        time_taken_usec: start_time.elapsed().as_micros() as u32,
                    },
                );
            }
            r.into_response()
        }
        Err(e) => {
            info!(?e, "Got error");
            HELIX_METRICS_CLIENT.send_event(
                EventType::QueryError,
                QueryErrorEvent {
                    cluster_id: state.cluster_id.clone(),
                    query_name,
                    input_json: sonic_rs::to_string(&body).ok(),
                    output_json: sonic_rs::to_string(&json!({ "error": e.to_string() })).ok(),
                    time_taken_usec: start_time.elapsed().as_micros() as u32,
                },
            );
            e.into_response()
        }
    }
}

pub struct AppState {
    pub worker_pool: WorkerPool,
    pub schema_json: Option<String>,
    pub cluster_id: Option<String>,
}

pub struct CoreSetter {
    cores: Vec<CoreId>,
    threads_per_core: usize,
    incrementing_index: AtomicUsize,
}

impl CoreSetter {
    pub fn new(cores: Vec<CoreId>, threads_per_core: usize) -> Self {
        Self {
            cores,
            threads_per_core,
            incrementing_index: AtomicUsize::new(0),
        }
    }

    pub fn num_threads(&self) -> usize {
        self.cores.len() * self.threads_per_core
    }

    pub fn set_current(self: Arc<Self>) {
        let curr_idx = self
            .incrementing_index
            .fetch_add(1, atomic::Ordering::SeqCst);

        let core_index = curr_idx / self.threads_per_core;
        match self.cores.get(core_index) {
            Some(c) => {
                core_affinity::set_for_current(*c);
                trace!("Set core affinity to: {c:?}");
            }
            None => warn!(
                "CoreSetter::set_current called more times than cores.len() * threads_per_core. Core affinity not set"
            ),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::helix_engine::traversal_core::config::Config;
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

    // ============================================================================
    // HelixGateway Tests
    // ============================================================================

    #[test]
    fn test_gateway_new_basic() {
        let (graph, _temp_dir) = create_test_graph();
        let gateway = HelixGateway::new("127.0.0.1:8080", graph, 5, None, None, None);

        assert_eq!(gateway.address, "127.0.0.1:8080");
        assert_eq!(gateway.workers_per_core, 5);
        assert!(gateway.opts.is_none());
    }

    #[test]
    fn test_gateway_new_with_routes() {
        let (graph, _temp_dir) = create_test_graph();
        let routes = HashMap::new();
        let gateway = HelixGateway::new("127.0.0.1:8080", graph, 5, Some(routes), None, None);

        assert_eq!(gateway.address, "127.0.0.1:8080");
        assert!(gateway.router.routes.is_empty());
    }

    #[test]
    fn test_gateway_new_with_mcp_routes() {
        let (graph, _temp_dir) = create_test_graph();
        let mcp_routes = HashMap::new();
        let gateway = HelixGateway::new("127.0.0.1:8080", graph, 5, None, Some(mcp_routes), None);

        assert_eq!(gateway.address, "127.0.0.1:8080");
        assert!(gateway.router.mcp_routes.is_empty());
    }

    #[test]
    fn test_gateway_new_with_opts() {
        let (graph, temp_dir) = create_test_graph();
        let opts = HelixGraphEngineOpts {
            path: temp_dir.path().to_str().unwrap().to_string(),
            config: Config::default(),
            version_info: Default::default(),
        };
        let gateway = HelixGateway::new("127.0.0.1:8080", graph, 5, None, None, Some(opts));

        assert!(gateway.opts.is_some());
    }

    #[test]
    fn test_gateway_new_with_cluster_id() {
        unsafe {
            std::env::set_var("CLUSTER_ID", "test-cluster-123");
        }
        let (graph, _temp_dir) = create_test_graph();
        let gateway = HelixGateway::new("127.0.0.1:8080", graph, 5, None, None, None);

        assert!(gateway.cluster_id.is_some());
        assert_eq!(gateway.cluster_id.unwrap(), "test-cluster-123");
        unsafe {
            std::env::remove_var("CLUSTER_ID");
        }
    }

    #[test]
    fn test_gateway_fields() {
        let (graph, _temp_dir) = create_test_graph();
        let gateway = HelixGateway::new("0.0.0.0:3000", graph, 10, None, None, None);

        assert_eq!(gateway.address, "0.0.0.0:3000");
        assert_eq!(gateway.workers_per_core, 10);
    }

    #[test]
    fn test_gateway_address_format() {
        let (graph, _temp_dir) = create_test_graph();
        let gateway = HelixGateway::new("localhost:8080", graph.clone(), 1, None, None, None);
        assert_eq!(gateway.address, "localhost:8080");

        let gateway2 = HelixGateway::new("0.0.0.0:80", graph, 1, None, None, None);
        assert_eq!(gateway2.address, "0.0.0.0:80");
    }

    #[test]
    fn test_gateway_workers_per_core() {
        let (graph, _temp_dir) = create_test_graph();

        let gateway1 = HelixGateway::new("127.0.0.1:8080", graph.clone(), 1, None, None, None);
        assert_eq!(gateway1.workers_per_core, 1);

        let gateway2 = HelixGateway::new("127.0.0.1:8080", graph.clone(), 10, None, None, None);
        assert_eq!(gateway2.workers_per_core, 10);

        let gateway3 = HelixGateway::new(
            "127.0.0.1:8080",
            graph,
            GatewayOpts::DEFAULT_WORKERS_PER_CORE,
            None,
            None,
            None,
        );
        assert_eq!(gateway3.workers_per_core, 5);
    }

    // ============================================================================
    // AppState Tests
    // ============================================================================

    #[test]
    fn test_app_state_creation() {
        let (graph, _temp_dir) = create_test_graph();
        let router = Arc::new(HelixRouter::new(None, None));
        let rt = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .enable_all()
                .build()
                .unwrap(),
        );

        let cores = core_affinity::get_core_ids().unwrap_or_default();
        let core_setter = Arc::new(CoreSetter::new(cores, 1));
        let worker_pool = WorkerPool::new(core_setter, graph, router, rt);

        let state = AppState {
            worker_pool,
            schema_json: None,
            cluster_id: None,
        };

        assert!(state.schema_json.is_none());
        assert!(state.cluster_id.is_none());
    }

    #[test]
    fn test_app_state_with_schema() {
        let (graph, _temp_dir) = create_test_graph();
        let router = Arc::new(HelixRouter::new(None, None));
        let rt = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .enable_all()
                .build()
                .unwrap(),
        );

        let cores = core_affinity::get_core_ids().unwrap_or_default();
        let core_setter = Arc::new(CoreSetter::new(cores, 1));
        let worker_pool = WorkerPool::new(core_setter, graph, router, rt);

        let state = AppState {
            worker_pool,
            schema_json: Some("{\"schema\": \"test\"}".to_string()),
            cluster_id: None,
        };

        assert!(state.schema_json.is_some());
        assert_eq!(state.schema_json.unwrap(), "{\"schema\": \"test\"}");
    }

    #[test]
    fn test_app_state_with_cluster_id() {
        let (graph, _temp_dir) = create_test_graph();
        let router = Arc::new(HelixRouter::new(None, None));
        let rt = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .enable_all()
                .build()
                .unwrap(),
        );

        let cores = core_affinity::get_core_ids().unwrap_or_default();
        let core_setter = Arc::new(CoreSetter::new(cores, 1));
        let worker_pool = WorkerPool::new(core_setter, graph, router, rt);

        let state = AppState {
            worker_pool,
            schema_json: None,
            cluster_id: Some("cluster-456".to_string()),
        };

        assert!(state.cluster_id.is_some());
        assert_eq!(state.cluster_id.unwrap(), "cluster-456");
    }

    // ============================================================================
    // CoreSetter Tests
    // ============================================================================

    #[test]
    fn test_core_setter_new() {
        let cores = vec![CoreId { id: 0 }, CoreId { id: 1 }];
        let setter = CoreSetter::new(cores.clone(), 5);

        assert_eq!(setter.cores.len(), 2);
        assert_eq!(setter.threads_per_core, 5);
    }

    #[test]
    fn test_core_setter_num_threads_single_core() {
        let cores = vec![CoreId { id: 0 }];
        let setter = CoreSetter::new(cores, 1);

        assert_eq!(setter.num_threads(), 1);
    }

    #[test]
    fn test_core_setter_num_threads_multiple_cores() {
        let cores = vec![CoreId { id: 0 }, CoreId { id: 1 }, CoreId { id: 2 }];
        let setter = CoreSetter::new(cores, 1);

        assert_eq!(setter.num_threads(), 3);
    }

    #[test]
    fn test_core_setter_num_threads_multiple_threads_per_core() {
        let cores = vec![CoreId { id: 0 }, CoreId { id: 1 }];
        let setter = CoreSetter::new(cores, 5);

        assert_eq!(setter.num_threads(), 10);
    }

    #[test]
    fn test_core_setter_num_threads_edge_cases() {
        // Zero cores
        let setter1 = CoreSetter::new(vec![], 5);
        assert_eq!(setter1.num_threads(), 0);

        // Zero threads per core
        let cores = vec![CoreId { id: 0 }, CoreId { id: 1 }];
        let setter2 = CoreSetter::new(cores, 0);
        assert_eq!(setter2.num_threads(), 0);
    }

    #[test]
    fn test_core_setter_calculation() {
        let cores = vec![
            CoreId { id: 0 },
            CoreId { id: 1 },
            CoreId { id: 2 },
            CoreId { id: 3 },
        ];
        let setter = CoreSetter::new(cores, 8);

        assert_eq!(setter.num_threads(), 32);
    }

    #[test]
    fn test_core_setter_empty_cores() {
        let setter = CoreSetter::new(vec![], 10);

        assert_eq!(setter.cores.len(), 0);
        assert_eq!(setter.num_threads(), 0);
    }

    #[test]
    fn test_core_setter_single_thread() {
        let cores = vec![CoreId { id: 0 }, CoreId { id: 1 }];
        let setter = CoreSetter::new(cores, 1);

        assert_eq!(setter.threads_per_core, 1);
        assert_eq!(setter.num_threads(), 2);
    }

    #[test]
    fn test_core_setter_many_threads() {
        let cores = vec![CoreId { id: 0 }];
        let setter = CoreSetter::new(cores, 100);

        assert_eq!(setter.num_threads(), 100);
    }

    #[test]
    fn test_core_setter_num_threads_consistency() {
        let cores = vec![CoreId { id: 0 }, CoreId { id: 1 }];
        let setter = CoreSetter::new(cores, 5);

        assert_eq!(setter.num_threads(), 10);
        assert_eq!(setter.num_threads(), 10);
        assert_eq!(setter.num_threads(), 10);
    }

    #[test]
    fn test_core_setter_threads_per_core_zero() {
        let cores = vec![CoreId { id: 0 }];
        let setter = CoreSetter::new(cores, 0);

        assert_eq!(setter.threads_per_core, 0);
        assert_eq!(setter.num_threads(), 0);
    }

    #[test]
    fn test_core_setter_with_default_workers() {
        let cores = vec![CoreId { id: 0 }, CoreId { id: 1 }];
        let setter = CoreSetter::new(cores, GatewayOpts::DEFAULT_WORKERS_PER_CORE);

        assert_eq!(setter.threads_per_core, 5);
        assert_eq!(setter.num_threads(), 10);
    }

    #[test]
    fn test_core_setter_index_initial_value() {
        let cores = vec![CoreId { id: 0 }];
        let setter = CoreSetter::new(cores, 1);

        assert_eq!(
            setter.incrementing_index.load(atomic::Ordering::SeqCst),
            0
        );
    }

    #[test]
    fn test_gateway_opts_default_workers_per_core() {
        assert_eq!(GatewayOpts::DEFAULT_WORKERS_PER_CORE, 5);
    }
}
