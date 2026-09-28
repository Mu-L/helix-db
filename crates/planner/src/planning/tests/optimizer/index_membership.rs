//! End-to-end selection of node index membership for post-expansion filters.

use crate::planning::tests::support::*;

fn membership_indexes() -> IndexCatalogSnapshot {
    IndexCatalogSnapshot::default()
        .with_node_eq(ScopedPropertyKey::try_new("Group", "name").unwrap())
        .with_node_eq(ScopedPropertyKey::try_new("Attribute", "kind").unwrap())
        .with_node_eq(ScopedPropertyKey::try_new("Attribute", "status").unwrap())
        .with_node_range(
            ScopedPropertyDirectionKey::try_new("Attribute", "rank", RangeIndexDirection::Asc)
                .unwrap(),
        )
}

/// Statistics proving that `name = g3` selects 5,000 of 50,000 groups, so
/// the source stays an index read and the expanded stream is large enough to
/// pay for reading the membership bitmaps. Without them the point source
/// keeps its small estimate through the expansions.
fn large_ctx() -> PlannerContext {
    let mut large = ctx(membership_indexes());
    large.stats = large
        .stats
        .with_node_eq_cardinality(ScopedPropertyKey::try_new("Group", "name").unwrap(), 5_000)
        .with_node_label_cardinality(NonEmptyString::new("Group").unwrap(), 50_000);
    large
}

fn attributes_where(predicate: Predicate) -> Traversal<helix_ast::traversal::OnNodes, ReadOnly> {
    g().n_with_label_where("Group", Predicate::eq("name", "g3"))
        .in_(Some("IN_GROUP"))
        .out(Some("HAS_ATTRIBUTE"))
        .where_(predicate)
}

fn memberships(plan: &ExecutablePlan) -> Vec<&crate::exec::ExecNodeIndexMembershipPlan> {
    plan.steps()
        .iter()
        .filter_map(|step| match &step.op {
            ExecOp::IndexMembership { plan } => Some(plan.as_ref()),
            _ => None,
        })
        .collect()
}

fn only_membership(plan: &ExecutablePlan) -> &crate::exec::ExecNodeIndexMembershipPlan {
    let [membership] = memberships(plan)[..] else {
        panic!("expected one index membership step: {:#?}", plan.steps());
    };
    membership
}

fn filter_predicates(plan: &ExecutablePlan) -> Vec<&Predicate> {
    plan.steps()
        .iter()
        .filter_map(|step| match &step.op {
            ExecOp::Filter { predicate } => Some(predicate.predicate()),
            _ => None,
        })
        .collect()
}

#[test]
fn post_expansion_equality_filter_uses_index_membership_after_out_and_in() {
    for traversal in [
        attributes_where(Predicate::eq("kind", "B")),
        g().n_with_label_where("Group", Predicate::eq("name", "g3"))
            .out(Some("HAS_ATTRIBUTE"))
            .in_(Some("LINKS"))
            .where_(Predicate::eq("kind", "B")),
    ] {
        let plan = executable_traversal(traversal.values(vec!["kind"]), large_ctx());
        let membership = only_membership(&plan);

        assert_eq!(membership.label.as_ref(), "Attribute");
        assert_eq!(
            membership.outside_label,
            crate::ir::NodeMembershipOutsideLabel::Evaluate
        );
        assert_eq!(
            membership.predicate.predicate(),
            &Predicate::eq("kind", "B")
        );
        assert!(matches!(
            &membership.set,
            crate::exec::ExecNodeSecondarySetPlan::Bitmap(
                crate::exec::ExecNodeBitmapExpr::PointRead { key, .. }
            ) if key.label == "Attribute" && key.property == "kind"
        ));
        assert!(filter_predicates(&plan).is_empty(), "{:#?}", plan.steps());
        assert!(has_exec_op_family(&plan, ExecOpFamily::Expand));
        assert_eq!(
            crate::diagnostics::analyze(&plan, &large_ctx())
                .statistics
                .residual_filters,
            0
        );
    }
}

#[test]
fn post_expansion_membership_keeps_unindexed_conjuncts_as_a_residual_filter() {
    let plan = executable_traversal(
        attributes_where(Predicate::and(vec![
            Predicate::eq("kind", "B"),
            Predicate::contains("title", "x"),
        ]))
        .values(vec!["kind"]),
        large_ctx(),
    );

    assert_eq!(
        only_membership(&plan).predicate.predicate(),
        &Predicate::eq("kind", "B")
    );
    assert_eq!(
        filter_predicates(&plan),
        [&Predicate::contains("title", "x")]
    );
    let membership_position = plan
        .steps()
        .iter()
        .position(|step| matches!(step.op, ExecOp::IndexMembership { .. }))
        .unwrap();
    let filter_position = plan
        .steps()
        .iter()
        .position(|step| matches!(step.op, ExecOp::Filter { .. }))
        .unwrap();
    assert!(membership_position < filter_position);
}

#[test]
fn post_expansion_membership_covers_in_and_multiple_indexed_conjuncts_but_not_ranges() {
    let is_in = executable_traversal(
        attributes_where(Predicate::is_in(
            "kind",
            PropertyValue::StringArray(vec!["A".to_owned(), "B".to_owned()]),
        ))
        .values(vec!["kind"]),
        large_ctx(),
    );
    assert!(matches!(
        &only_membership(&is_in).set,
        crate::exec::ExecNodeSecondarySetPlan::Bitmap(
            crate::exec::ExecNodeBitmapExpr::BatchedUnionRead { values, .. }
        ) if values.len() == 2
    ));

    // A range set would verify every in-range record of the label, so range
    // conjuncts keep the per-row filter, alone or behind an equality set.
    let range = executable_traversal(
        attributes_where(Predicate::gte("rank", 3)).values(vec!["kind"]),
        large_ctx(),
    );
    assert!(memberships(&range).is_empty(), "{:#?}", range.steps());
    assert_eq!(filter_predicates(&range), [&Predicate::gte("rank", 3)]);
    let ranged = executable_traversal(
        attributes_where(Predicate::and(vec![
            Predicate::eq("kind", "B"),
            Predicate::gte("rank", 3),
        ]))
        .values(vec!["kind"]),
        large_ctx(),
    );
    assert_eq!(
        only_membership(&ranged).predicate.predicate(),
        &Predicate::eq("kind", "B")
    );
    assert_eq!(filter_predicates(&ranged), [&Predicate::gte("rank", 3)]);

    let both = executable_traversal(
        attributes_where(Predicate::and(vec![
            Predicate::eq("kind", "B"),
            Predicate::eq("status", "live"),
        ]))
        .values(vec!["kind"]),
        large_ctx(),
    );
    assert!(matches!(
        &only_membership(&both).set,
        crate::exec::ExecNodeSecondarySetPlan::Bitmap(
            crate::exec::ExecNodeBitmapExpr::Intersect { .. }
        ) | crate::exec::ExecNodeSecondarySetPlan::Intersect { .. }
    ));
    assert!(filter_predicates(&both).is_empty());
}

#[test]
fn label_scoped_post_expansion_membership_rejects_other_labels_without_reads() {
    let plan = executable_traversal(
        attributes_where(Predicate::eq("kind", "B"))
            .has_label("Attribute")
            .values(vec!["kind"]),
        large_ctx(),
    );
    let membership = only_membership(&plan);

    assert_eq!(
        membership.outside_label,
        crate::ir::NodeMembershipOutsideLabel::Reject
    );
    assert!(filter_predicates(&plan).is_empty(), "{:#?}", plan.steps());
}

#[test]
fn null_unindexed_and_edge_stream_filters_keep_the_per_row_filter() {
    for traversal in [
        attributes_where(Predicate::eq("kind", PropertyValue::Null)),
        attributes_where(Predicate::eq("color", "red")),
        attributes_where(Predicate::contains("kind", "B")),
    ] {
        let plan = executable_traversal(traversal.values(vec!["kind"]), large_ctx());
        assert!(memberships(&plan).is_empty(), "{:#?}", plan.steps());
        assert_eq!(filter_predicates(&plan).len(), 1);
    }

    let edges = executable_traversal(
        g().n_with_label_where("Group", Predicate::eq("name", "g3"))
            .out_e(Some("HAS_ATTRIBUTE"))
            .where_(Predicate::eq("kind", "B"))
            .values(vec!["kind"]),
        large_ctx(),
    );
    assert!(memberships(&edges).is_empty(), "{:#?}", edges.steps());
    assert_eq!(filter_predicates(&edges).len(), 1);
}

#[test]
fn late_bound_parameters_keep_runtime_classified_membership() {
    let mut planner_ctx = large_ctx();
    planner_ctx.late_bound_params = [
        NonEmptyString::new("kind").unwrap(),
        NonEmptyString::new("kinds").unwrap(),
    ]
    .into_iter()
    .collect();
    let equality = executable_traversal(
        attributes_where(Predicate::eq_param("kind", "kind")).values(vec!["kind"]),
        planner_ctx.clone(),
    );
    assert!(matches!(
        &only_membership(&equality).set,
        crate::exec::ExecNodeSecondarySetPlan::DynamicEquality { param, .. }
            if param.as_ref() == "kind"
    ));

    let set = executable_traversal(
        attributes_where(Predicate::is_in_param("kind", "kinds")).values(vec!["kind"]),
        planner_ctx.clone(),
    );
    assert!(matches!(
        &only_membership(&set).set,
        crate::exec::ExecNodeSecondarySetPlan::DynamicMembership { values, .. }
            if values.param().as_ref() == "kinds"
    ));

    // Late-bound range bounds may bind values without a range encoding, so
    // they keep the per-row filter instead of a runtime range scan.
    planner_ctx.late_bound_params = [NonEmptyString::new("min").unwrap()].into_iter().collect();
    let range = executable_traversal(
        attributes_where(Predicate::gte_param("rank", "min")).values(vec!["kind"]),
        planner_ctx,
    );
    assert!(memberships(&range).is_empty(), "{:#?}", range.steps());
    assert_eq!(
        filter_predicates(&range),
        [&Predicate::gte_param("rank", "min")]
    );
}

#[test]
fn source_filters_still_use_index_access_instead_of_membership() {
    let plan = executable_traversal(
        g().n_with_label_where("Attribute", Predicate::eq("kind", "B"))
            .out(Some("LINKS"))
            .values(vec!["kind"]),
        ctx(membership_indexes()),
    );

    assert!(memberships(&plan).is_empty());
    assert!(matches!(
        unwrapped_first_exec_access(&plan),
        ExecAccessPlan::Node(ExecNodeAccessPlan::Bitmap { .. })
    ));
}

#[test]
fn tiny_bounded_inputs_keep_the_per_row_filter() {
    let plan = executable_traversal(
        g().n_with_label_where("Group", Predicate::eq("name", "g3"))
            .out(Some("HAS_ATTRIBUTE"))
            .limit(2)
            .where_(Predicate::eq("kind", "B"))
            .values(vec!["kind"]),
        large_ctx(),
    );

    assert!(memberships(&plan).is_empty(), "{:#?}", plan.steps());
    assert_eq!(filter_predicates(&plan), [&Predicate::eq("kind", "B")]);
}

#[test]
fn unknown_fan_out_alone_keeps_the_per_row_filter() {
    // Without statistics the point source keeps its small estimate through
    // both expansions, so the label-sized set reads never look cheaper than
    // reading the stream's own records.
    for predicate in [
        Predicate::eq("kind", "B"),
        Predicate::and(vec![
            Predicate::eq("$label", "Attribute"),
            Predicate::eq("kind", "B"),
        ]),
        Predicate::and(vec![
            Predicate::eq("kind", "B"),
            Predicate::eq("status", "live"),
        ]),
    ] {
        let plan = executable_traversal(
            attributes_where(predicate.clone()).values(vec!["kind"]),
            ctx(membership_indexes()),
        );
        assert!(memberships(&plan).is_empty(), "{:#?}", plan.steps());
        assert_eq!(filter_predicates(&plan), [&predicate]);
    }
}

#[test]
fn post_expansion_membership_feeds_counts_and_variable_pipelines() {
    // Count cursors price their operators at the unknown-input default, so
    // only a label-scoped predicate pays for the set read there.
    let count = executable_traversal(
        attributes_where(Predicate::eq("kind", "B"))
            .has_label("Attribute")
            .count(),
        large_ctx(),
    );
    let counted = count
        .steps()
        .iter()
        .find_map(|step| match &step.op {
            ExecOp::Count { plan } => Some(plan.as_ref()),
            _ => None,
        })
        .expect("count step");
    assert!(
        matches!(
            counted,
            ExecCountPlan::Stream(crate::exec::ExecCountStreamPlan {
                cursor: crate::exec::ExecCountCursorPlan::IndexMembership { .. },
                ..
            })
        ),
        "{counted:#?}"
    );

    // A runtime input keeps the unknown-input estimate, which pays for a
    // label-scoped set read without statistics.
    let batch = read_batch()
        .var_as(
            "groups",
            g().n_with_label_where("Group", Predicate::eq("name", "g3")),
        )
        .var_as(
            "result",
            g().n(NodeRef::var("groups"))
                .in_(Some("IN_GROUP"))
                .out(Some("HAS_ATTRIBUTE"))
                .where_(Predicate::eq("kind", "B"))
                .has_label("Attribute")
                .values(vec!["kind"]),
        )
        .returning(["result"]);
    let plan = crate::planning::plan_read_batch(&batch, &ctx(membership_indexes())).unwrap();
    assert_eq!(only_membership(&plan).label.as_ref(), "Attribute");
    assert!(filter_predicates(&plan).is_empty(), "{:#?}", plan.steps());
}

#[test]
fn unscoped_membership_needs_one_indexed_label_and_pays_for_other_labels() {
    // Without statistics a label scan keeps the unknown-scan estimate. That
    // pays for a label-scoped set read, but not for an unscoped one, whose
    // rows of other labels still read their records.
    let label_scan = |predicate: Predicate| {
        g().n_with_label("Group")
            .in_(Some("IN_GROUP"))
            .out(Some("HAS_ATTRIBUTE"))
            .where_(predicate)
            .values(vec!["kind"])
    };
    let unscoped = executable_traversal(
        label_scan(Predicate::eq("kind", "B")),
        ctx(membership_indexes()),
    );
    assert!(memberships(&unscoped).is_empty(), "{:#?}", unscoped.steps());
    let scoped = executable_traversal(
        label_scan(Predicate::and(vec![
            Predicate::eq("$label", "Attribute"),
            Predicate::eq("kind", "B"),
        ])),
        ctx(membership_indexes()),
    );
    assert_eq!(
        only_membership(&scoped).outside_label,
        crate::ir::NodeMembershipOutsideLabel::Reject
    );

    // A second label indexing `kind` leaves the expansion's label ambiguous,
    // so even a large stream keeps the per-row filter unless it is scoped.
    let mut ambiguous = large_ctx();
    ambiguous.indexes =
        membership_indexes().with_node_eq(ScopedPropertyKey::try_new("Note", "kind").unwrap());
    let plan = executable_traversal(
        attributes_where(Predicate::eq("kind", "B")).values(vec!["kind"]),
        ambiguous.clone(),
    );
    assert!(memberships(&plan).is_empty(), "{:#?}", plan.steps());
    assert_eq!(filter_predicates(&plan), [&Predicate::eq("kind", "B")]);
    let plan = executable_traversal(
        attributes_where(Predicate::and(vec![
            Predicate::eq("$label", "Note"),
            Predicate::eq("kind", "B"),
        ]))
        .values(vec!["kind"]),
        ambiguous,
    );
    assert_eq!(only_membership(&plan).label.as_ref(), "Note");
}

/// `large_ctx` with a second, broad `Group` equality index. `name = g3`
/// still selects 5,000 of 5,000,000 groups while `region = west` selects
/// 4,900,000, so reading the region bitmap costs far more than verifying
/// the seed's candidates: the source keeps `name` as its only index seed
/// and evaluates `region` as a per-row residual filter.
fn seeded_ctx() -> PlannerContext {
    let mut seeded =
        ctx(membership_indexes()
            .with_node_eq(ScopedPropertyKey::try_new("Group", "region").unwrap()));
    seeded.stats = seeded
        .stats
        .with_node_eq_cardinality(ScopedPropertyKey::try_new("Group", "name").unwrap(), 5_000)
        .with_node_eq_cardinality(
            ScopedPropertyKey::try_new("Group", "region").unwrap(),
            4_900_000,
        )
        .with_node_label_cardinality(NonEmptyString::new("Group").unwrap(), 5_000_000);
    seeded
}

/// Attributes behind the groups named `g3` in region `west`.
fn seeded_attributes_where(
    predicate: Predicate,
) -> Traversal<helix_ast::traversal::OnNodes, ReadOnly> {
    g().n_with_label_where(
        "Group",
        Predicate::and(vec![
            Predicate::eq("name", "g3"),
            Predicate::eq("region", "west"),
        ]),
    )
    .in_(Some("IN_GROUP"))
    .out(Some("HAS_ATTRIBUTE"))
    .where_(predicate)
}

/// Assert the plan reads the `name` seed, keeps `region` as a per-row filter
/// ahead of every expansion, and answers the post-expansion `kind` equality
/// with membership after the expansions.
fn assert_seed_residual_then_membership(plan: &ExecutablePlan) {
    assert!(
        matches!(
            unwrapped_first_exec_access(plan),
            ExecAccessPlan::Node(ExecNodeAccessPlan::Bitmap {
                bitmap: crate::exec::ExecNodeBitmapExpr::PointRead { key, .. },
            }) if key.label == "Group" && key.property == "name"
        ),
        "{:#?}",
        plan.steps()
    );
    let membership = only_membership(plan);
    assert_eq!(membership.label.as_ref(), "Attribute");
    assert_eq!(
        membership.predicate.predicate(),
        &Predicate::eq("kind", "B")
    );
    let position = |matches: &dyn Fn(&ExecOp) -> bool| {
        plan.steps()
            .iter()
            .position(|step| matches(&step.op))
            .unwrap_or_else(|| panic!("missing step: {:#?}", plan.steps()))
    };
    // A seed keeps every other conjunct inside one residual conjunction.
    let residual = position(&|op| {
        matches!(op, ExecOp::Filter { predicate }
            if predicate.predicate() == &Predicate::and(vec![Predicate::eq("region", "west")]))
    });
    let expand = position(&|op| ExecOpFamily::Expand.matches(op));
    let membership = position(&|op| matches!(op, ExecOp::IndexMembership { .. }));
    assert!(
        residual < expand && expand < membership,
        "{:#?}",
        plan.steps()
    );
}

#[test]
fn equality_seed_residual_stays_a_filter_while_later_filters_use_membership() {
    // Plain and projected streams reach the seed through the access-pipeline
    // and root-stream filter rules. Either way the membership rule starts
    // after the leading residual, so the `Group` region index is never read.
    let residual = Predicate::and(vec![Predicate::eq("region", "west")]);
    for plan in [
        executable_traversal(
            seeded_attributes_where(Predicate::eq("kind", "B")),
            seeded_ctx(),
        ),
        executable_traversal(
            seeded_attributes_where(Predicate::eq("kind", "B")).values(vec!["kind"]),
            seeded_ctx(),
        ),
    ] {
        assert_seed_residual_then_membership(&plan);
        assert_eq!(filter_predicates(&plan), [&residual]);
    }

    // An unindexed post-expansion conjunct stays a residual behind the
    // membership, separate from the seed's residual.
    let plan = executable_traversal(
        seeded_attributes_where(Predicate::and(vec![
            Predicate::eq("kind", "B"),
            Predicate::contains("title", "x"),
        ]))
        .values(vec!["kind"]),
        seeded_ctx(),
    );
    assert_seed_residual_then_membership(&plan);
    assert_eq!(
        filter_predicates(&plan),
        [&residual, &Predicate::contains("title", "x")]
    );
}

#[test]
fn equality_seed_residual_stays_a_filter_under_a_count_with_membership() {
    // Count cursors price their operators at the unknown-input default, so
    // only a label-scoped predicate pays for the set read there.
    let plan = executable_traversal(
        seeded_attributes_where(Predicate::eq("kind", "B"))
            .has_label("Attribute")
            .count(),
        seeded_ctx(),
    );
    let counted = plan
        .steps()
        .iter()
        .find_map(|step| match &step.op {
            ExecOp::Count { plan } => Some(plan.as_ref()),
            _ => None,
        })
        .expect("count step");
    let ExecCountPlan::Stream(crate::exec::ExecCountStreamPlan { cursor, .. }) = counted else {
        panic!("expected a streamed count: {counted:#?}");
    };
    // The cursor chain from the count down to its source.
    let chain = core::iter::successors(Some(cursor), |cursor| match cursor {
        crate::exec::ExecCountCursorPlan::IndexMembership { input, .. }
        | crate::exec::ExecCountCursorPlan::Expand { input, .. }
        | crate::exec::ExecCountCursorPlan::Filter { input, .. } => Some(input.as_ref()),
        _ => None,
    })
    .collect::<Vec<_>>();
    assert!(
        matches!(
            chain[..],
            [
                crate::exec::ExecCountCursorPlan::IndexMembership { plan: membership, .. },
                crate::exec::ExecCountCursorPlan::Expand { .. },
                crate::exec::ExecCountCursorPlan::Expand { .. },
                crate::exec::ExecCountCursorPlan::Filter { predicate, .. },
                crate::exec::ExecCountCursorPlan::NodeBitmap(
                    crate::exec::ExecNodeBitmapExpr::PointRead { key, .. }
                ),
            ] if membership.label.as_ref() == "Attribute"
                && membership.outside_label == crate::ir::NodeMembershipOutsideLabel::Reject
                && predicate.predicate()
                    == &Predicate::and(vec![Predicate::eq("region", "west")])
                && key.property == "name"
        ),
        "{counted:#?}"
    );
}
