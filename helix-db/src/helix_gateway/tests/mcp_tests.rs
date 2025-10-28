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
}
