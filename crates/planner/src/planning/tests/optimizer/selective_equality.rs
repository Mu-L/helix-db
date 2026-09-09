use crate::planning::tests::support::*;

#[test]
fn selective_equality_empty_statistics_uses_three_bitmaps() {
    let indexes = ["tenant", "type", "deleted"].into_iter().fold(
        IndexCatalogSnapshot::default(),
        |indexes, property| {
            indexes.with_node_eq(ScopedPropertyKey::try_new("Resource", property).unwrap())
        },
    );
    let plan = executable_traversal(
        g().n_with_label_where(
            "Resource",
            Predicate::and(vec![
                Predicate::eq("tenant", "one"),
                Predicate::eq("type", "pod"),
                Predicate::eq("deleted", true),
            ]),
        )
        .values(vec!["id"]),
        ctx(indexes),
    );
    eprintln!("selected cost: {:#?}\nsteps: {:#?}\ntrace: {:#?}", plan.metrics().selected_cost, plan.steps(), plan.trace());
    assert!(matches!(
        first_exec_access(&plan),
        ExecAccessPlan::Node(ExecNodeAccessPlan::SecondarySet { .. })
    ));
    assert_no_exec_op_family(&plan, ExecOpFamily::Filter);
}
