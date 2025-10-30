#[cfg(test)]
mod mcp_tests {
    use std::sync::{Arc, Mutex};

    use axum::body::Bytes;
    use bumpalo::Bump;
    use tempfile::TempDir;

    use crate::{
        helix_engine::{
            storage_core::version_info::VersionInfo,
            traversal_core::{
                HelixGraphEngine, HelixGraphEngineOpts,
                config::Config,
                ops::{
                    g::G,
                    source::{add_e::AddEAdapter, add_n::AddNAdapter},
                },
                traversal_value::TraversalValue,
            },
        },
        helix_gateway::mcp::{
            mcp::{MCPConnection, MCPToolInput, McpBackend, McpConnections, collect},
            tools::{EdgeType, FilterProperties, FilterTraversal, Operator, ToolArgs},
        },
        protocol::{Format, Request, request::RequestType, value::Value},
        utils::properties::ImmutablePropertiesMap,
    };

    fn setup_engine() -> (HelixGraphEngine, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let opts = HelixGraphEngineOpts {
            path: temp_dir.path().to_str().unwrap().to_string(),
            config: Config::default(),
            version_info: VersionInfo::default(),
        };
        let engine = HelixGraphEngine::new(opts).unwrap();
        (engine, temp_dir)
    }

    #[test]
    fn execute_query_chain_out_step_returns_neighbor() {
        let (engine, _temp_dir) = setup_engine();
        let mut txn = engine.storage.graph_env.write_txn().unwrap();
        let arena = Bump::new();
        let person1 = G::new_mut(engine.storage.as_ref(), &arena, &mut txn)
            .add_n(
                "person",
                Some(ImmutablePropertiesMap::new(
                    1,
                    [("name", Value::from("John"))].into_iter(),
                    &arena,
                )),
                None,
            )
            .collect_to_obj();

        let person2 = G::new_mut(engine.storage.as_ref(), &arena, &mut txn)
            .add_n("person", None, None)
            .collect_to_obj();

        G::new_mut(engine.storage.as_ref(), &arena, &mut txn)
            .add_edge("knows", None, person1.id(), person2.id(), false)
            .collect_to_obj();

        txn.commit().unwrap();

        let storage = engine.storage.as_ref();
        let arena = Bump::new();
        let txn = storage.graph_env.read_txn().unwrap();

        let steps = vec![
            ToolArgs::NFromType {
                node_type: "person".to_string(),
            },
            ToolArgs::FilterItems {
                filter: FilterTraversal {
                    properties: Some(vec![vec![FilterProperties {
                        key: "name".to_string(),
                        value: Value::from("John"),
                        operator: Some(Operator::Eq),
                    }]]),
                    filter_traversals: None,
                },
            },
            ToolArgs::OutStep {
                edge_label: "knows".to_string(),
                edge_type: EdgeType::Node,
                filter: None,
            },
        ];

        let stream =
            crate::helix_gateway::mcp::tools::execute_query_chain(&steps, storage, &txn, &arena)
                .unwrap();

        let results = stream.collect().unwrap();

        assert_eq!(results.len(), 1);
        let TraversalValue::Node(node) = &results[0] else {
            panic!("expected node result");
        };
        assert_eq!(node.id, person2.id());
    }

    #[test]
    fn mcp_connection_next_advances_position() {
        let (engine, _temp_dir) = setup_engine();
        let mut txn = engine.storage.graph_env.write_txn().unwrap();
        let arena = Bump::new();

        let _ = G::new_mut(engine.storage.as_ref(), &arena, &mut txn)
            .add_n("person", None, None)
            .collect_to_obj();
        let _ = G::new_mut(engine.storage.as_ref(), &arena, &mut txn)
            .add_n("person", None, None)
            .collect_to_obj();

        txn.commit().unwrap();

        let storage = engine.storage.as_ref();

        let mut connection = MCPConnection::new("test".to_string());
        connection.add_query_step(ToolArgs::NFromType {
            node_type: "person".to_string(),
        });

        let first = connection.next_item(storage, &arena).unwrap();
        assert!(!matches!(
            first,
            crate::helix_engine::traversal_core::traversal_value::TraversalValue::Empty
        ));

        let second = connection.next_item(storage, &arena).unwrap();
        assert!(!matches!(
            second,
            crate::helix_engine::traversal_core::traversal_value::TraversalValue::Empty
        ));

        assert_eq!(connection.current_position, 2);
    }

    #[test]
    fn collect_handler_respects_range() {
        let (engine, _temp_dir) = setup_engine();
        let mut txn = engine.storage.graph_env.write_txn().unwrap();
        let arena = Bump::new();
        for _ in 0..5 {
            let _ = G::new_mut(engine.storage.as_ref(), &arena, &mut txn)
                .add_n("person", None, None)
                .collect_to_obj();
        }
        txn.commit().unwrap();

        let backend = Arc::new(McpBackend::new(Arc::clone(&engine.storage)));
        let connections = Arc::new(Mutex::new(McpConnections::new()));

        let mut connection = MCPConnection::new("conn".to_string());
        connection.add_query_step(ToolArgs::NFromType {
            node_type: "person".to_string(),
        });
        connections.lock().unwrap().add_connection(connection);

        let request_body = Bytes::from(
            r#"{"connection_id":"conn","range":{"start":1,"end":3},"drop":false}"#.to_string(),
        );

        let request = Request {
            name: "collect".to_string(),
            req_type: RequestType::MCP,
            body: request_body,
            in_fmt: Format::Json,
            out_fmt: Format::Json,
        };

        let mut input = MCPToolInput {
            request,
            mcp_backend: backend,
            mcp_connections: Arc::clone(&connections),
            schema: None,
        };

        let response = collect(&mut input).unwrap();
        let body = String::from_utf8(response.body.clone()).unwrap();
        println!("{:?}", body);
        let id_count = body.matches("\"id\"").count();
        let label_count = body.matches("\"label\"").count();
        assert_eq!(id_count, 2);
        assert_eq!(label_count, 2);
    }

    // ============================================================================
    // MCP Handler Registration Tests
    // ============================================================================

    #[test]
    fn test_mcp_handlers_are_registered() {
        use crate::helix_gateway::mcp::mcp::MCPHandlerSubmission;

        let handler_names: Vec<&str> = inventory::iter::<MCPHandlerSubmission>
            .into_iter()
            .map(|submission| submission.0.name)
            .collect();

        // Core handlers
        assert!(handler_names.contains(&"init"));
        assert!(handler_names.contains(&"tool_call"));
        assert!(handler_names.contains(&"next"));
        assert!(handler_names.contains(&"collect"));
        assert!(handler_names.contains(&"aggregate_by"));
        assert!(handler_names.contains(&"group_by"));
        assert!(handler_names.contains(&"reset"));
        assert!(handler_names.contains(&"schema_resource"));

        // New individual tool handlers
        assert!(handler_names.contains(&"out_step"));
        assert!(handler_names.contains(&"in_step"));
        assert!(handler_names.contains(&"out_e_step"));
        assert!(handler_names.contains(&"in_e_step"));
        assert!(handler_names.contains(&"n_from_type"));
        assert!(handler_names.contains(&"e_from_type"));
        assert!(handler_names.contains(&"filter_items"));
        assert!(handler_names.contains(&"order_by"));
    }

    #[test]
    fn test_all_new_tool_endpoints_registered() {
        use crate::helix_gateway::mcp::mcp::MCPHandlerSubmission;

        let handler_names: Vec<&str> = inventory::iter::<MCPHandlerSubmission>
            .into_iter()
            .map(|submission| submission.0.name)
            .collect();

        let required_tool_endpoints = vec![
            "out_step",
            "in_step",
            "out_e_step",
            "in_e_step",
            "n_from_type",
            "e_from_type",
            "filter_items",
            "order_by",
        ];

        for endpoint in required_tool_endpoints {
            assert!(
                handler_names.contains(&endpoint),
                "MCP endpoint '{}' is not registered",
                endpoint
            );
        }
    }

    // ============================================================================
    // Individual Tool Endpoint HTTP Tests
    // ============================================================================

    #[test]
    fn test_out_step_handler_http() {
        use crate::helix_gateway::mcp::mcp::out_step;

        let (engine, _temp_dir) = setup_engine();
        let mut txn = engine.storage.graph_env.write_txn().unwrap();
        let arena = Bump::new();

        let person1 = G::new_mut(engine.storage.as_ref(), &arena, &mut txn)
            .add_n(
                "person",
                Some(ImmutablePropertiesMap::new(
                    1,
                    [("name", Value::from("Alice"))].into_iter(),
                    &arena,
                )),
                None,
            )
            .collect_to_obj();

        let person2 = G::new_mut(engine.storage.as_ref(), &arena, &mut txn)
            .add_n("person", None, None)
            .collect_to_obj();

        G::new_mut(engine.storage.as_ref(), &arena, &mut txn)
            .add_edge("knows", None, person1.id(), person2.id(), false)
            .collect_to_obj();

        txn.commit().unwrap();

        let backend = Arc::new(McpBackend::new(Arc::clone(&engine.storage)));
        let connections = Arc::new(Mutex::new(McpConnections::new()));

        let mut connection = MCPConnection::new("conn1".to_string());
        connection.add_query_step(ToolArgs::NFromType {
            node_type: "person".to_string(),
        });
        connection.add_query_step(ToolArgs::FilterItems {
            filter: FilterTraversal {
                properties: Some(vec![vec![FilterProperties {
                    key: "name".to_string(),
                    value: Value::from("Alice"),
                    operator: Some(Operator::Eq),
                }]]),
                filter_traversals: None,
            },
        });
        connections.lock().unwrap().add_connection(connection);

        let request_body = Bytes::from(
            r#"{"connection_id":"conn1","edge_label":"knows","edge_type":"node","filter":null}"#
                .to_string(),
        );

        let request = Request {
            name: "out_step".to_string(),
            req_type: RequestType::MCP,
            body: request_body,
            in_fmt: Format::Json,
            out_fmt: Format::Json,
        };

        let mut input = MCPToolInput {
            request,
            mcp_backend: backend,
            mcp_connections: Arc::clone(&connections),
            schema: None,
        };

        let response = out_step(&mut input).unwrap();
        let body = String::from_utf8(response.body.clone()).unwrap();
        assert!(body.contains(&person2.id().to_string()));
    }

    #[test]
    fn test_in_step_handler_http() {
        use crate::helix_gateway::mcp::mcp::in_step;

        let (engine, _temp_dir) = setup_engine();
        let mut txn = engine.storage.graph_env.write_txn().unwrap();
        let arena = Bump::new();

        let person1 = G::new_mut(engine.storage.as_ref(), &arena, &mut txn)
            .add_n("person", None, None)
            .collect_to_obj();

        let person2 = G::new_mut(engine.storage.as_ref(), &arena, &mut txn)
            .add_n("person", None, None)
            .collect_to_obj();

        G::new_mut(engine.storage.as_ref(), &arena, &mut txn)
            .add_edge("knows", None, person1.id(), person2.id(), false)
            .collect_to_obj();

        txn.commit().unwrap();

        let backend = Arc::new(McpBackend::new(Arc::clone(&engine.storage)));
        let connections = Arc::new(Mutex::new(McpConnections::new()));

        let mut connection = MCPConnection::new("conn2".to_string());
        connection.add_query_step(ToolArgs::NFromType {
            node_type: "person".to_string(),
        });
        connections.lock().unwrap().add_connection(connection);

        let request_body = Bytes::from(
            r#"{"connection_id":"conn2","edge_label":"knows","edge_type":"node","filter":null}"#
                .to_string(),
        );

        let request = Request {
            name: "in_step".to_string(),
            req_type: RequestType::MCP,
            body: request_body,
            in_fmt: Format::Json,
            out_fmt: Format::Json,
        };

        let mut input = MCPToolInput {
            request,
            mcp_backend: backend,
            mcp_connections: Arc::clone(&connections),
            schema: None,
        };

        let response = in_step(&mut input).unwrap();
        assert!(!response.body.is_empty());
    }

    #[test]
    fn test_out_e_step_handler_http() {
        use crate::helix_gateway::mcp::mcp::out_e_step;

        let (engine, _temp_dir) = setup_engine();
        let mut txn = engine.storage.graph_env.write_txn().unwrap();
        let arena = Bump::new();

        let person1 = G::new_mut(engine.storage.as_ref(), &arena, &mut txn)
            .add_n("person", None, None)
            .collect_to_obj();

        let person2 = G::new_mut(engine.storage.as_ref(), &arena, &mut txn)
            .add_n("person", None, None)
            .collect_to_obj();

        G::new_mut(engine.storage.as_ref(), &arena, &mut txn)
            .add_edge("knows", None, person1.id(), person2.id(), false)
            .collect_to_obj();

        txn.commit().unwrap();

        let backend = Arc::new(McpBackend::new(Arc::clone(&engine.storage)));
        let connections = Arc::new(Mutex::new(McpConnections::new()));

        let mut connection = MCPConnection::new("conn3".to_string());
        connection.add_query_step(ToolArgs::NFromType {
            node_type: "person".to_string(),
        });
        connections.lock().unwrap().add_connection(connection);

        let request_body = Bytes::from(
            r#"{"connection_id":"conn3","edge_label":"knows","filter":null}"#.to_string(),
        );

        let request = Request {
            name: "out_e_step".to_string(),
            req_type: RequestType::MCP,
            body: request_body,
            in_fmt: Format::Json,
            out_fmt: Format::Json,
        };

        let mut input = MCPToolInput {
            request,
            mcp_backend: backend,
            mcp_connections: Arc::clone(&connections),
            schema: None,
        };

        let response = out_e_step(&mut input).unwrap();
        let body = String::from_utf8(response.body.clone()).unwrap();
        assert!(body.contains("\"label\":\"knows\""));
    }

    #[test]
    fn test_in_e_step_handler_http() {
        use crate::helix_gateway::mcp::mcp::in_e_step;

        let (engine, _temp_dir) = setup_engine();
        let mut txn = engine.storage.graph_env.write_txn().unwrap();
        let arena = Bump::new();

        let person1 = G::new_mut(engine.storage.as_ref(), &arena, &mut txn)
            .add_n("person", None, None)
            .collect_to_obj();

        let person2 = G::new_mut(engine.storage.as_ref(), &arena, &mut txn)
            .add_n("person", None, None)
            .collect_to_obj();

        G::new_mut(engine.storage.as_ref(), &arena, &mut txn)
            .add_edge("knows", None, person1.id(), person2.id(), false)
            .collect_to_obj();

        txn.commit().unwrap();

        let backend = Arc::new(McpBackend::new(Arc::clone(&engine.storage)));
        let connections = Arc::new(Mutex::new(McpConnections::new()));

        let mut connection = MCPConnection::new("conn4".to_string());
        connection.add_query_step(ToolArgs::NFromType {
            node_type: "person".to_string(),
        });
        connections.lock().unwrap().add_connection(connection);

        let request_body =
            Bytes::from(r#"{"connection_id":"conn4","edge_label":"knows","filter":null}"#.to_string());

        let request = Request {
            name: "in_e_step".to_string(),
            req_type: RequestType::MCP,
            body: request_body,
            in_fmt: Format::Json,
            out_fmt: Format::Json,
        };

        let mut input = MCPToolInput {
            request,
            mcp_backend: backend,
            mcp_connections: Arc::clone(&connections),
            schema: None,
        };

        let response = in_e_step(&mut input).unwrap();
        assert!(!response.body.is_empty());
    }

    #[test]
    fn test_n_from_type_handler_http() {
        use crate::helix_gateway::mcp::mcp::n_from_type;

        let (engine, _temp_dir) = setup_engine();
        let mut txn = engine.storage.graph_env.write_txn().unwrap();
        let arena = Bump::new();

        for _ in 0..3 {
            G::new_mut(engine.storage.as_ref(), &arena, &mut txn)
                .add_n("person", None, None)
                .collect_to_obj();
        }

        txn.commit().unwrap();

        let backend = Arc::new(McpBackend::new(Arc::clone(&engine.storage)));
        let connections = Arc::new(Mutex::new(McpConnections::new()));

        let connection = MCPConnection::new("conn5".to_string());
        connections.lock().unwrap().add_connection(connection);

        let request_body =
            Bytes::from(r#"{"connection_id":"conn5","node_type":"person"}"#.to_string());

        let request = Request {
            name: "n_from_type".to_string(),
            req_type: RequestType::MCP,
            body: request_body,
            in_fmt: Format::Json,
            out_fmt: Format::Json,
        };

        let mut input = MCPToolInput {
            request,
            mcp_backend: backend,
            mcp_connections: Arc::clone(&connections),
            schema: None,
        };

        let response = n_from_type(&mut input).unwrap();
        let body = String::from_utf8(response.body.clone()).unwrap();
        assert!(body.contains("\"label\":\"person\""));
    }

    #[test]
    fn test_e_from_type_handler_http() {
        use crate::helix_gateway::mcp::mcp::e_from_type;

        let (engine, _temp_dir) = setup_engine();
        let mut txn = engine.storage.graph_env.write_txn().unwrap();
        let arena = Bump::new();

        let person1 = G::new_mut(engine.storage.as_ref(), &arena, &mut txn)
            .add_n("person", None, None)
            .collect_to_obj();

        let person2 = G::new_mut(engine.storage.as_ref(), &arena, &mut txn)
            .add_n("person", None, None)
            .collect_to_obj();

        G::new_mut(engine.storage.as_ref(), &arena, &mut txn)
            .add_edge("knows", None, person1.id(), person2.id(), false)
            .collect_to_obj();

        txn.commit().unwrap();

        let backend = Arc::new(McpBackend::new(Arc::clone(&engine.storage)));
        let connections = Arc::new(Mutex::new(McpConnections::new()));

        let connection = MCPConnection::new("conn6".to_string());
        connections.lock().unwrap().add_connection(connection);

        let request_body =
            Bytes::from(r#"{"connection_id":"conn6","edge_type":"knows"}"#.to_string());

        let request = Request {
            name: "e_from_type".to_string(),
            req_type: RequestType::MCP,
            body: request_body,
            in_fmt: Format::Json,
            out_fmt: Format::Json,
        };

        let mut input = MCPToolInput {
            request,
            mcp_backend: backend,
            mcp_connections: Arc::clone(&connections),
            schema: None,
        };

        let response = e_from_type(&mut input).unwrap();
        let body = String::from_utf8(response.body.clone()).unwrap();
        assert!(body.contains("\"label\":\"knows\""));
    }

    #[test]
    fn test_filter_items_handler_http() {
        use crate::helix_gateway::mcp::mcp::filter_items;

        let (engine, _temp_dir) = setup_engine();
        let mut txn = engine.storage.graph_env.write_txn().unwrap();
        let arena = Bump::new();

        G::new_mut(engine.storage.as_ref(), &arena, &mut txn)
            .add_n(
                "person",
                Some(ImmutablePropertiesMap::new(
                    1,
                    [("age", Value::from(25))].into_iter(),
                    &arena,
                )),
                None,
            )
            .collect_to_obj();

        G::new_mut(engine.storage.as_ref(), &arena, &mut txn)
            .add_n(
                "person",
                Some(ImmutablePropertiesMap::new(
                    1,
                    [("age", Value::from(30))].into_iter(),
                    &arena,
                )),
                None,
            )
            .collect_to_obj();

        txn.commit().unwrap();

        let backend = Arc::new(McpBackend::new(Arc::clone(&engine.storage)));
        let connections = Arc::new(Mutex::new(McpConnections::new()));

        let mut connection = MCPConnection::new("conn7".to_string());
        connection.add_query_step(ToolArgs::NFromType {
            node_type: "person".to_string(),
        });
        connections.lock().unwrap().add_connection(connection);

        let request_body = Bytes::from(
            r#"{"connection_id":"conn7","filter":{"properties":[[{"key":"age","value":30,"operator":"=="}]],"filter_traversals":null}}"#
                .to_string(),
        );

        let request = Request {
            name: "filter_items".to_string(),
            req_type: RequestType::MCP,
            body: request_body,
            in_fmt: Format::Json,
            out_fmt: Format::Json,
        };

        let mut input = MCPToolInput {
            request,
            mcp_backend: backend,
            mcp_connections: Arc::clone(&connections),
            schema: None,
        };

        let response = filter_items(&mut input).unwrap();
        let body = String::from_utf8(response.body.clone()).unwrap();
        assert!(body.contains("30"));
    }

    #[test]
    fn test_order_by_handler_http() {
        use crate::helix_gateway::mcp::mcp::order_by;

        let (engine, _temp_dir) = setup_engine();
        let mut txn = engine.storage.graph_env.write_txn().unwrap();
        let arena = Bump::new();

        G::new_mut(engine.storage.as_ref(), &arena, &mut txn)
            .add_n(
                "person",
                Some(ImmutablePropertiesMap::new(
                    1,
                    [("age", Value::from(30))].into_iter(),
                    &arena,
                )),
                None,
            )
            .collect_to_obj();

        G::new_mut(engine.storage.as_ref(), &arena, &mut txn)
            .add_n(
                "person",
                Some(ImmutablePropertiesMap::new(
                    1,
                    [("age", Value::from(25))].into_iter(),
                    &arena,
                )),
                None,
            )
            .collect_to_obj();

        txn.commit().unwrap();

        let backend = Arc::new(McpBackend::new(Arc::clone(&engine.storage)));
        let connections = Arc::new(Mutex::new(McpConnections::new()));

        let mut connection = MCPConnection::new("conn8".to_string());
        connection.add_query_step(ToolArgs::NFromType {
            node_type: "person".to_string(),
        });
        connections.lock().unwrap().add_connection(connection);

        let request_body = Bytes::from(
            r#"{"connection_id":"conn8","properties":"age","order":"asc"}"#.to_string(),
        );

        let request = Request {
            name: "order_by".to_string(),
            req_type: RequestType::MCP,
            body: request_body,
            in_fmt: Format::Json,
            out_fmt: Format::Json,
        };

        let mut input = MCPToolInput {
            request,
            mcp_backend: backend,
            mcp_connections: Arc::clone(&connections),
            schema: None,
        };

        let response = order_by(&mut input).unwrap();
        let body = String::from_utf8(response.body.clone()).unwrap();
        // Verify response contains age property
        assert!(body.contains("25") || body.contains("30"));
    }

    // ============================================================================
    // Integration Tests - Tool Execution Logic
    // ============================================================================

    #[test]
    fn test_out_step_traversal_integration() {
        let (engine, _temp_dir) = setup_engine();
        let mut txn = engine.storage.graph_env.write_txn().unwrap();
        let arena = Bump::new();

        let alice = G::new_mut(engine.storage.as_ref(), &arena, &mut txn)
            .add_n(
                "person",
                Some(ImmutablePropertiesMap::new(
                    1,
                    [("name", Value::from("Alice"))].into_iter(),
                    &arena,
                )),
                None,
            )
            .collect_to_obj();

        let bob = G::new_mut(engine.storage.as_ref(), &arena, &mut txn)
            .add_n(
                "person",
                Some(ImmutablePropertiesMap::new(
                    1,
                    [("name", Value::from("Bob"))].into_iter(),
                    &arena,
                )),
                None,
            )
            .collect_to_obj();

        let charlie = G::new_mut(engine.storage.as_ref(), &arena, &mut txn)
            .add_n(
                "person",
                Some(ImmutablePropertiesMap::new(
                    1,
                    [("name", Value::from("Charlie"))].into_iter(),
                    &arena,
                )),
                None,
            )
            .collect_to_obj();

        G::new_mut(engine.storage.as_ref(), &arena, &mut txn)
            .add_edge("knows", None, alice.id(), bob.id(), false)
            .collect_to_obj();

        G::new_mut(engine.storage.as_ref(), &arena, &mut txn)
            .add_edge("knows", None, bob.id(), charlie.id(), false)
            .collect_to_obj();

        txn.commit().unwrap();

        let storage = engine.storage.as_ref();
        let arena = Bump::new();
        let txn = storage.graph_env.read_txn().unwrap();

        let steps = vec![
            ToolArgs::NFromType {
                node_type: "person".to_string(),
            },
            ToolArgs::FilterItems {
                filter: FilterTraversal {
                    properties: Some(vec![vec![FilterProperties {
                        key: "name".to_string(),
                        value: Value::from("Alice"),
                        operator: Some(Operator::Eq),
                    }]]),
                    filter_traversals: None,
                },
            },
            ToolArgs::OutStep {
                edge_label: "knows".to_string(),
                edge_type: EdgeType::Node,
                filter: None,
            },
        ];

        let stream =
            crate::helix_gateway::mcp::tools::execute_query_chain(&steps, storage, &txn, &arena)
                .unwrap();

        let results = stream.collect().unwrap();
        assert_eq!(results.len(), 1);

        let TraversalValue::Node(node) = &results[0] else {
            panic!("expected node result");
        };
        assert_eq!(node.id, bob.id());
    }

    #[test]
    fn test_in_step_traversal_integration() {
        let (engine, _temp_dir) = setup_engine();
        let mut txn = engine.storage.graph_env.write_txn().unwrap();
        let arena = Bump::new();

        let alice = G::new_mut(engine.storage.as_ref(), &arena, &mut txn)
            .add_n("person", None, None)
            .collect_to_obj();

        let bob = G::new_mut(engine.storage.as_ref(), &arena, &mut txn)
            .add_n("person", None, None)
            .collect_to_obj();

        G::new_mut(engine.storage.as_ref(), &arena, &mut txn)
            .add_edge("knows", None, alice.id(), bob.id(), false)
            .collect_to_obj();

        txn.commit().unwrap();

        let storage = engine.storage.as_ref();
        let arena = Bump::new();
        let txn = storage.graph_env.read_txn().unwrap();

        // Start from Bob and traverse back to Alice via in_step
        let steps = vec![
            ToolArgs::NFromType {
                node_type: "person".to_string(),
            },
            ToolArgs::InStep {
                edge_label: "knows".to_string(),
                edge_type: EdgeType::Node,
                filter: None,
            },
        ];

        let stream =
            crate::helix_gateway::mcp::tools::execute_query_chain(&steps, storage, &txn, &arena)
                .unwrap();

        let results = stream.collect().unwrap();
        assert!(results.len() > 0);
    }

    #[test]
    fn test_filter_with_multiple_conditions() {
        let (engine, _temp_dir) = setup_engine();
        let mut txn = engine.storage.graph_env.write_txn().unwrap();
        let arena = Bump::new();

        G::new_mut(engine.storage.as_ref(), &arena, &mut txn)
            .add_n(
                "person",
                Some(ImmutablePropertiesMap::new(
                    2,
                    [
                        ("age", Value::from(25)),
                        ("name", Value::from("Alice")),
                    ]
                    .into_iter(),
                    &arena,
                )),
                None,
            )
            .collect_to_obj();

        G::new_mut(engine.storage.as_ref(), &arena, &mut txn)
            .add_n(
                "person",
                Some(ImmutablePropertiesMap::new(
                    2,
                    [
                        ("age", Value::from(30)),
                        ("name", Value::from("Bob")),
                    ]
                    .into_iter(),
                    &arena,
                )),
                None,
            )
            .collect_to_obj();

        txn.commit().unwrap();

        let storage = engine.storage.as_ref();
        let arena = Bump::new();
        let txn = storage.graph_env.read_txn().unwrap();

        let steps = vec![
            ToolArgs::NFromType {
                node_type: "person".to_string(),
            },
            ToolArgs::FilterItems {
                filter: FilterTraversal {
                    properties: Some(vec![vec![
                        FilterProperties {
                            key: "age".to_string(),
                            value: Value::from(30),
                            operator: Some(Operator::Eq),
                        },
                        FilterProperties {
                            key: "name".to_string(),
                            value: Value::from("Bob"),
                            operator: Some(Operator::Eq),
                        },
                    ]]),
                    filter_traversals: None,
                },
            },
        ];

        let stream =
            crate::helix_gateway::mcp::tools::execute_query_chain(&steps, storage, &txn, &arena)
                .unwrap();

        let results = stream.collect().unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_order_by_ascending() {
        let (engine, _temp_dir) = setup_engine();
        let mut txn = engine.storage.graph_env.write_txn().unwrap();
        let arena = Bump::new();

        for age in [30, 20, 25] {
            G::new_mut(engine.storage.as_ref(), &arena, &mut txn)
                .add_n(
                    "person",
                    Some(ImmutablePropertiesMap::new(
                        1,
                        [("age", Value::from(age))].into_iter(),
                        &arena,
                    )),
                    None,
                )
                .collect_to_obj();
        }

        txn.commit().unwrap();

        let storage = engine.storage.as_ref();
        let arena = Bump::new();
        let txn = storage.graph_env.read_txn().unwrap();

        let steps = vec![
            ToolArgs::NFromType {
                node_type: "person".to_string(),
            },
            ToolArgs::OrderBy {
                properties: "age".to_string(),
                order: crate::helix_gateway::mcp::tools::Order::Asc,
            },
        ];

        let stream =
            crate::helix_gateway::mcp::tools::execute_query_chain(&steps, storage, &txn, &arena)
                .unwrap();

        let results = stream.collect().unwrap();
        assert_eq!(results.len(), 3);

        // Verify ordering
        let TraversalValue::Node(node1) = &results[0] else {
            panic!("expected node");
        };
        let age1 = node1.get_property("age").unwrap();
        assert_eq!(age1, &Value::from(20));
    }

    #[test]
    fn test_order_by_descending() {
        let (engine, _temp_dir) = setup_engine();
        let mut txn = engine.storage.graph_env.write_txn().unwrap();
        let arena = Bump::new();

        for age in [30, 20, 25] {
            G::new_mut(engine.storage.as_ref(), &arena, &mut txn)
                .add_n(
                    "person",
                    Some(ImmutablePropertiesMap::new(
                        1,
                        [("age", Value::from(age))].into_iter(),
                        &arena,
                    )),
                    None,
                )
                .collect_to_obj();
        }

        txn.commit().unwrap();

        let storage = engine.storage.as_ref();
        let arena = Bump::new();
        let txn = storage.graph_env.read_txn().unwrap();

        let steps = vec![
            ToolArgs::NFromType {
                node_type: "person".to_string(),
            },
            ToolArgs::OrderBy {
                properties: "age".to_string(),
                order: crate::helix_gateway::mcp::tools::Order::Desc,
            },
        ];

        let stream =
            crate::helix_gateway::mcp::tools::execute_query_chain(&steps, storage, &txn, &arena)
                .unwrap();

        let results = stream.collect().unwrap();
        assert_eq!(results.len(), 3);

        // Verify ordering
        let TraversalValue::Node(node1) = &results[0] else {
            panic!("expected node");
        };
        let age1 = node1.get_property("age").unwrap();
        assert_eq!(age1, &Value::from(30));
    }

    #[test]
    fn test_combined_out_and_in_steps() {
        let (engine, _temp_dir) = setup_engine();
        let mut txn = engine.storage.graph_env.write_txn().unwrap();
        let arena = Bump::new();

        let alice = G::new_mut(engine.storage.as_ref(), &arena, &mut txn)
            .add_n("person", None, None)
            .collect_to_obj();

        let bob = G::new_mut(engine.storage.as_ref(), &arena, &mut txn)
            .add_n("person", None, None)
            .collect_to_obj();

        let charlie = G::new_mut(engine.storage.as_ref(), &arena, &mut txn)
            .add_n("person", None, None)
            .collect_to_obj();

        G::new_mut(engine.storage.as_ref(), &arena, &mut txn)
            .add_edge("knows", None, alice.id(), bob.id(), false)
            .collect_to_obj();

        G::new_mut(engine.storage.as_ref(), &arena, &mut txn)
            .add_edge("knows", None, bob.id(), charlie.id(), false)
            .collect_to_obj();

        txn.commit().unwrap();

        let storage = engine.storage.as_ref();
        let arena = Bump::new();
        let txn = storage.graph_env.read_txn().unwrap();

        // Navigate: Alice -> out(knows) -> Bob -> in(knows) -> Alice
        let steps = vec![
            ToolArgs::NFromType {
                node_type: "person".to_string(),
            },
            ToolArgs::OutStep {
                edge_label: "knows".to_string(),
                edge_type: EdgeType::Node,
                filter: None,
            },
            ToolArgs::InStep {
                edge_label: "knows".to_string(),
                edge_type: EdgeType::Node,
                filter: None,
            },
        ];

        let stream =
            crate::helix_gateway::mcp::tools::execute_query_chain(&steps, storage, &txn, &arena)
                .unwrap();

        let results = stream.collect().unwrap();
        assert!(results.len() > 0);
    }

    #[test]
    fn test_e_from_type_returns_edges() {
        let (engine, _temp_dir) = setup_engine();
        let mut txn = engine.storage.graph_env.write_txn().unwrap();
        let arena = Bump::new();

        let person1 = G::new_mut(engine.storage.as_ref(), &arena, &mut txn)
            .add_n("person", None, None)
            .collect_to_obj();

        let person2 = G::new_mut(engine.storage.as_ref(), &arena, &mut txn)
            .add_n("person", None, None)
            .collect_to_obj();

        G::new_mut(engine.storage.as_ref(), &arena, &mut txn)
            .add_edge("knows", None, person1.id(), person2.id(), false)
            .collect_to_obj();

        G::new_mut(engine.storage.as_ref(), &arena, &mut txn)
            .add_edge("likes", None, person1.id(), person2.id(), false)
            .collect_to_obj();

        txn.commit().unwrap();

        let storage = engine.storage.as_ref();
        let arena = Bump::new();
        let txn = storage.graph_env.read_txn().unwrap();

        let steps = vec![ToolArgs::EFromType {
            edge_type: "knows".to_string(),
        }];

        let stream =
            crate::helix_gateway::mcp::tools::execute_query_chain(&steps, storage, &txn, &arena)
                .unwrap();

        let results = stream.collect().unwrap();
        assert_eq!(results.len(), 1);

        let TraversalValue::Edge(edge) = &results[0] else {
            panic!("expected edge result");
        };
        assert_eq!(edge.label, "knows");
    }
}
