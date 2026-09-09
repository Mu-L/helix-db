//! End-to-end coverage of the HEL-850 empty-statistics plan regression.

use crate::planning::tests::support::*;

#[test]
fn selective_equality_uses_three_bitmaps_with_empty_or_populated_statistics() {
    let indexes = ["tenant", "type", "deleted"].into_iter().fold(
        IndexCatalogSnapshot::default(),
        |indexes, property| {
            let key = ScopedPropertyKey::try_new("Resource", property).unwrap();
            indexes.with_node_eq(key.clone()).with_edge_eq(key)
        },
    );
    // An estimate of zero is not a proof of emptiness: stale statistics must
    // never suppress the actual lookup. Include rare and unknown matches.
    for deletion_rows in [None, Some(0), Some(1), Some(10)] {
        let stats = deletion_rows.map_or_else(StatsSnapshot::default, |rows| {
            let mut stats = StatsSnapshot::default();
            stats = stats
                .with_node_label_cardinality(NonEmptyString::new("Resource").unwrap(), 100_000)
                .with_edge_label_cardinality(NonEmptyString::new("Resource").unwrap(), 100_000);
            for (property, count) in [("tenant", 1000), ("type", 2000), ("deleted", rows)] {
                let key = ScopedPropertyKey::try_new("Resource", property).unwrap();
                stats = stats
                    .with_node_eq_cardinality(key.clone(), count)
                    .with_edge_eq_cardinality(key, count);
            }
            stats
        });
        for parameterized in [false, true] {
            for nested in [false, true] {
                let terms = vec![
                    Predicate::eq("tenant", "one"),
                    Predicate::eq("type", "pod"),
                    if parameterized {
                        Predicate::eq_param("deleted", "deleted")
                    } else {
                        Predicate::eq("deleted", true)
                    },
                ];
                let predicate = if nested {
                    Predicate::and(vec![Predicate::and(terms)])
                } else {
                    Predicate::and(terms)
                };
                let context = PlannerContext {
                    stats: stats.clone(),
                    params: ParamBindings::default().with_value(
                        NonEmptyString::new("deleted").unwrap(),
                        PropertyValue::Bool(true),
                    ),
                    ..ctx(indexes.clone())
                };
                for traversal in [
                    g().n_with_label_where("Resource", predicate.clone())
                        .values(vec!["id"]),
                    g().e_with_label_where("Resource", predicate)
                        .values(vec!["id"]),
                ] {
                    let plan = executable_traversal(traversal, context.clone());
                    assert!(
                        matches!(first_exec_access(&plan),
                            ExecAccessPlan::Node(ExecNodeAccessPlan::SecondarySet { set: crate::exec::ExecNodeSecondarySetPlan::Intersect { rest, .. } }) if rest.len() == 2
                        ) || matches!(first_exec_access(&plan),
                            ExecAccessPlan::Edge(ExecEdgeAccessPlan::SecondarySet { set: crate::exec::ExecEdgeSecondarySetPlan::Intersect { rest, .. } }) if rest.len() == 2
                        ),
                        "{deletion_rows:?}, parameterized={parameterized}, nested={nested}: {:#?}",
                        plan.steps()
                    );
                    assert_no_exec_op_family(&plan, ExecOpFamily::Filter);
                    assert_eq!(plan.metrics().selected_cost.object_reads, 3);
                    assert_eq!(plan.metrics().selected_cost.range_nexts, 0);
                    assert_eq!(plan.metrics().selected_cost.parallel_width, 1);
                    if deletion_rows.is_none() {
                        // Full selected plan: serial memberships plus projection.
                        assert_eq!(plan.metrics().selected_cost.latency.as_micros(), 16_220);
                    }
                    let diagnostics = crate::diagnostics::analyze(&plan, &context);
                    assert!(diagnostics.insights.iter().all(|insight| !matches!(
                        insight,
                        crate::diagnostics::PlannerInsight::UnboundedScan(_)
                    )));
                }
            }
        }
    }
}

#[test]
fn selective_equality_costing_still_allows_measurably_small_label_scans() {
    let mut context = PlannerContext::default();
    for property in ["tenant", "type", "deleted"] {
        let key = ScopedPropertyKey::try_new("Resource", property).unwrap();
        context.indexes = context
            .indexes
            .with_node_eq(key.clone())
            .with_edge_eq(key.clone());
        context.stats = context
            .stats
            .with_node_eq_cardinality(key.clone(), 1)
            .with_edge_eq_cardinality(key, 1);
    }
    context.stats = context
        .stats
        .with_node_label_cardinality(NonEmptyString::new("Resource").unwrap(), 1)
        .with_edge_label_cardinality(NonEmptyString::new("Resource").unwrap(), 1);
    let predicate = Predicate::and(vec![
        Predicate::eq("tenant", "one"),
        Predicate::eq("type", "pod"),
        Predicate::eq("deleted", true),
    ]);
    for traversal in [
        g().n_with_label_where("Resource", predicate.clone())
            .values(vec!["id"]),
        g().e_with_label_where("Resource", predicate)
            .values(vec!["id"]),
    ] {
        let plan = executable_traversal(traversal, context.clone());
        assert!(
            matches!(
                first_exec_access(&plan),
                ExecAccessPlan::Node(ExecNodeAccessPlan::LabelScan { .. })
                    | ExecAccessPlan::Edge(ExecEdgeAccessPlan::LabelScan { .. })
            ),
            "{:#?}",
            plan.steps()
        );
        assert_eq!(plan.metrics().selected_cost.authoritative_graph_reads, 1);
        assert!(has_exec_op_family(&plan, ExecOpFamily::Filter));
    }
}
