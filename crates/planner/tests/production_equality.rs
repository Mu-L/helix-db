//! Exact query ASTs recovered from HEL-850's production insights ledger.
//! Bound values are synthetic; empty statistics and the proven equality indexes
//! isolate planning from customer data and production storage latency.

use std::collections::BTreeMap;

use helix_ast::{query, value};
use helix_planner::{catalog, context, diagnostics, ir, planning};

#[test]
fn production_queries_keep_their_equality_anchors() {
    let requests: BTreeMap<String, query::QueryRequest> = serde_json::from_str(include_str!(
        "../../../docker-image/tests/fixtures/hel850-production-queries.json"
    ))
    .unwrap();
    assert_eq!(requests.len(), 9);
    let indexes = ["tenant", "type", "deleted"].into_iter().fold(
        catalog::IndexCatalogSnapshot::default(),
        |indexes, property| {
            indexes.with_node_eq(catalog::ScopedPropertyKey::try_new("Resource", property).unwrap())
        },
    );
    for (name, request) in requests {
        for limit in [0_i64, 1, 1_000] {
            let params = request.parameters().unwrap().iter().fold(
                context::ParamBindings::default(),
                |params, (name, input)| {
                    let value = if name == "limit" {
                        value::PropertyValue::I64(limit)
                    } else {
                        value::PropertyValue::from(input)
                    };
                    params.with_value(ir::NonEmptyString::new(name).unwrap(), value)
                },
            );
            let ctx = context::PlannerContext {
                params,
                indexes: indexes.clone(),
                ..Default::default()
            };
            assert_eq!(ctx.stats, context::StatsSnapshot::default());
            let output = planning::plan_with_diagnostics(request.query(), &ctx).unwrap();
            let stats = &output.diagnostics().statistics;
            eprintln!(
                "{name} limit={limit}: {}",
                serde_json::to_string(stats).unwrap()
            );
            assert!(!stats.guardrail_hit, "{name}");
            assert_eq!(stats.node_accesses.label_scans, 0, "{name}");
            assert_eq!(stats.node_accesses.all_scans, 0, "{name}");
            assert!(
                output
                    .diagnostics()
                    .insights
                    .iter()
                    .all(|insight| !matches!(
                        insight,
                        diagnostics::PlannerInsight::UnboundedScan(_)
                    )),
                "{name}"
            );
            if limit > 0 {
                let lookups = match name.as_str() {
                    "ab4_deleted_only" => 1,
                    "ab2_tenant_type_deleted" | "find_deleted_resource_ids_by_type" => 3,
                    "covered_workloads" => 6,
                    "ab1_tenant_type"
                    | "ab3_type_deleted"
                    | "find_resources_by_type"
                    | "find_resource_dedup_keys_by_type"
                    | "service_workload_map" => 2,
                    other => panic!("unexpected production fixture: {other}"),
                };
                assert_eq!(
                    stats.node_accesses.equality_index_lookups, lookups,
                    "{name}"
                );
            }
            if name == "service_workload_map" {
                assert_eq!(stats.expansions, 3);
                assert_eq!(stats.branches, 1);
            }
            if name == "covered_workloads" {
                assert_eq!(stats.expansions, 1);
                assert_eq!(stats.intersections, 1);
            }
        }
    }
}
