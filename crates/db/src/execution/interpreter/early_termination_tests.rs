//! Ordered-result and read-count oracle for limit execution.
use super::*;

#[tokio::test]
async fn filtered_limit_stops_property_reads_at_first_match() {
    let db = test_support::open_db("limit-property-read-oracle").await;
    let mut ids = Vec::new();
    for name in ["match", "other", "match"] {
        ids.push(test_support::add_user(&db, name).await);
    }
    let source = test_support::step(
        1,
        vec![],
        exec::ExecOp::Access {
            plan: Box::new(exec::ExecAccessPlan::Node(
                exec::ExecNodeAccessPlan::FromParam {
                    param: test_support::name("ids"),
                },
            )),
        },
    );
    let filter = test_support::step(
        2,
        vec![source.id],
        exec::ExecOp::Filter {
            predicate: ir::PredicatePlan::new(Predicate::Eq {
                left: Expr::Property("name".into()),
                right: Expr::Constant(helix_ast::value::PropertyValue::String("match".into())),
            })
            .unwrap(),
        },
    );
    let limit = test_support::step(
        3,
        vec![filter.id],
        exec::ExecOp::Limit {
            count: ir::StreamBoundPlan::Literal(1),
        },
    );
    let plan = test_support::executable(ir::PlanKind::Read, vec![source, filter, limit], 3);
    let params = context::ParamBindings::default().with_value(
        test_support::name("ids"),
        helix_ast::value::PropertyValue::I64Array(ids.iter().map(|id| *id as i64).collect()),
    );
    let mut ctx = ExecutionContext::new(&db, params);
    ctx.execute_steps(
        plan.steps(),
        plan.execution_order(),
        plan.root(),
        plan.execution_program(),
    )
    .await
    .unwrap();
    let result = ctx
        .finish(plan.root(), &exec::ExecutableReturns::None)
        .unwrap();
    assert_eq!(
        result.last,
        Some(ExecutionValue::Stream(vec![ExecutionRow::current(
            ElementRef::Node(ids[0])
        )]))
    );
    assert_eq!(ctx.projection_read_snapshot().property_gets, 1);
    assert_eq!(ctx.pull_work.snapshot().source_visits, 1);
    db.close().await.unwrap();
}

fn filter_name(value: &str) -> exec::ExecOp {
    exec::ExecOp::Filter {
        predicate: ir::PredicatePlan::new(Predicate::Eq {
            left: Expr::Property("name".into()),
            right: Expr::Constant(helix_ast::value::PropertyValue::String(value.into())),
        })
        .unwrap(),
    }
}

fn linear(ops: Vec<exec::ExecOp>) -> exec::ExecutablePlan {
    let root = ops.len();
    let steps = ops
        .into_iter()
        .enumerate()
        .map(|(index, op)| {
            test_support::step(
                index + 1,
                exec::ExecStepId::new(index).into_iter().collect(),
                op,
            )
        })
        .collect();
    test_support::executable(ir::PlanKind::Read, steps, root)
}

fn source() -> exec::ExecOp {
    exec::ExecOp::Access {
        plan: Box::new(exec::ExecAccessPlan::Node(
            exec::ExecNodeAccessPlan::FromParam {
                param: test_support::name("ids"),
            },
        )),
    }
}
fn limit(count: usize) -> exec::ExecOp {
    exec::ExecOp::Limit {
        count: ir::StreamBoundPlan::Literal(count),
    }
}

async fn run(
    ctx: &mut ExecutionContext<'_>,
    plan: &exec::ExecutablePlan,
) -> Result<ExecutionValue> {
    ctx.execute_steps(
        plan.steps(),
        plan.execution_order(),
        plan.root(),
        plan.execution_program(),
    )
    .await?;
    Ok(ctx
        .finish(plan.root(), &exec::ExecutableReturns::None)?
        .last
        .expect("root result"))
}

fn parameters(ids: &[u64]) -> context::ParamBindings {
    context::ParamBindings::default().with_value(
        test_support::name("ids"),
        helix_ast::value::PropertyValue::I64Array(ids.iter().map(|id| *id as i64).collect()),
    )
}

#[tokio::test]
async fn windows_count_matches_at_their_own_position() {
    let db = test_support::open_db("pull-window-position").await;
    let mut ids = Vec::new();
    for name in ["other", "match", "other", "match", "match"] {
        ids.push(test_support::add_user(&db, name).await);
    }
    for (ops, expected, reads) in [
        (
            vec![source(), filter_name("match"), limit(1)],
            vec![ids[1]],
            2,
        ),
        (vec![source(), limit(1), filter_name("match")], vec![], 1),
        (
            vec![
                source(),
                filter_name("match"),
                exec::ExecOp::Skip {
                    count: ir::StreamBoundPlan::Literal(1),
                },
                limit(1),
            ],
            vec![ids[3]],
            4,
        ),
        (vec![source(), filter_name("match"), limit(0)], vec![], 0),
        (vec![source(), filter_name("absent"), limit(1)], vec![], 5),
        (
            vec![source(), filter_name("match"), limit(9)],
            vec![ids[1], ids[3], ids[4]],
            5,
        ),
    ] {
        let mut ctx = ExecutionContext::new(&db, parameters(&ids));
        assert_eq!(
            run(&mut ctx, &linear(ops)).await.unwrap(),
            ExecutionValue::Stream(
                expected
                    .into_iter()
                    .map(|id| ExecutionRow::current(ElementRef::Node(id)))
                    .collect()
            )
        );
        assert_eq!(ctx.projection_read_snapshot().property_gets, reads);
    }
    db.close().await.unwrap();
}

#[tokio::test]
async fn expansion_does_not_prepare_a_later_parent() {
    use crate::encoding::keys;
    let db = test_support::open_db("pull-expand-parent-boundary").await;
    let first = test_support::add_user(&db, "first").await;
    let later = test_support::add_user(&db, "later").await;
    let neighbor = test_support::add_user(&db, "match").await;
    let edge = test_support::add_edge(&db, first, neighbor, "LINK").await;
    db.inner_db()
        .put(
            keys::DataKey::Data {
                scope: keys::scope::DataScope::LegacyUnscoped,
                kind: keys::DataKeyKind::Adjacency(keys::AdjacencyKey::new(later)),
            }
            .to_bytes(),
            bytes::Bytes::from_static(b"corrupt"),
        )
        .await
        .unwrap();
    for (output, expected) in [
        (ir::ExpandOutput::Nodes, ElementRef::Node(neighbor)),
        (ir::ExpandOutput::Edges, ElementRef::Edge(edge)),
    ] {
        let plan = linear(vec![
            source(),
            exec::ExecOp::Expand {
                plan: ir::ExpandPlan {
                    direction: ir::ExpandDirection::Out,
                    label: ir::ExpandLabelPlan::Any,
                    output,
                },
            },
            limit(1),
        ]);
        let mut ctx = ExecutionContext::new(&db, parameters(&[first, later]));
        let mut expected_row = ExecutionRow::current(ElementRef::Node(first));
        expected_row.set_current(expected);
        assert_eq!(
            run(&mut ctx, &plan).await.unwrap(),
            ExecutionValue::Stream(vec![expected_row])
        );
        let mut ctx = ExecutionContext::new(&db, parameters(&[later, first]));
        assert!(run(&mut ctx, &plan).await.is_err());
    }
    db.close().await.unwrap();
}

#[tokio::test]
async fn unused_corrupt_property_is_not_decoded() {
    use crate::encoding::keys;
    let db = test_support::open_db("pull-corrupt-property-tail").await;
    let first = test_support::add_user(&db, "match").await;
    let tail = test_support::add_user(&db, "match").await;
    db.inner_db()
        .put(
            keys::DataKey::Data {
                scope: keys::scope::DataScope::LegacyUnscoped,
                kind: keys::DataKeyKind::NodeProperty(keys::NodePropertyKey::new(tail)),
            }
            .to_bytes(),
            bytes::Bytes::from_static(b"corrupt"),
        )
        .await
        .unwrap();
    let plan = linear(vec![source(), filter_name("match"), limit(1)]);
    let mut ctx = ExecutionContext::new(&db, parameters(&[first, tail]));
    assert_eq!(
        run(&mut ctx, &plan).await.unwrap(),
        ExecutionValue::Stream(vec![ExecutionRow::current(ElementRef::Node(first))])
    );
    let mut ctx = ExecutionContext::new(&db, parameters(&[tail, first]));
    assert!(run(&mut ctx, &plan).await.is_err());
    db.close().await.unwrap();
}

#[tokio::test]
async fn pull_and_eager_execution_agree_on_order_duplicates_and_value_shapes() {
    let db = test_support::open_db("pull-differential-operators").await;
    let mut ids = Vec::new();
    for name in ["b", "a", "b", "c", "a"] {
        ids.push(test_support::add_user(&db, name).await);
    }
    let input = [ids[2], ids[0], ids[2], ids[4], ids[1], ids[3]];
    let middle = vec![
        vec![exec::ExecOp::Noop],
        vec![filter_name("a")],
        vec![exec::ExecOp::Distinct],
        vec![
            exec::ExecOp::Project {
                projection: ir::ProjectionPlan::Id,
            },
            exec::ExecOp::Distinct,
        ],
        vec![exec::ExecOp::Order {
            plan: ir::OrderPlan::ExplicitSort(ir::OrderKeys::from(ir::OrderKey {
                property: test_support::name("name"),
                order: helix_ast::traversal::Order::Asc,
            })),
        }],
        vec![exec::ExecOp::Project {
            projection: ir::ProjectionPlan::Values(
                ir::PropertyNames::new(ir::AtLeast::from_one_and_rest(
                    test_support::name("name"),
                    vec![],
                ))
                .unwrap(),
            ),
        }],
        vec![
            exec::ExecOp::Reserved {
                op: ir::ReservedOp::WithSack(helix_ast::value::PropertyValue::I64(4)),
            },
            exec::ExecOp::Reserved {
                op: ir::ReservedOp::SackGet,
            },
        ],
        vec![
            exec::ExecOp::Reserved {
                op: ir::ReservedOp::Fold,
            },
            exec::ExecOp::Reserved {
                op: ir::ReservedOp::Unfold,
            },
        ],
    ];
    for operators in middle {
        for skip in [0, 1, 7] {
            for take in [0, 1, 3, 9] {
                let mut ops = vec![source()];
                ops.extend(operators.clone());
                ops.extend([
                    exec::ExecOp::Skip {
                        count: ir::StreamBoundPlan::Literal(skip),
                    },
                    limit(take),
                ]);
                let plan = linear(ops);
                let mut eager = ExecutionContext::new(&db, parameters(&input));
                eager
                    .execute_steps(
                        plan.steps(),
                        plan.execution_order(),
                        plan.root(),
                        &exec::ExecProgram::default(),
                    )
                    .await
                    .unwrap();
                let expected = eager
                    .finish(plan.root(), &exec::ExecutableReturns::None)
                    .unwrap()
                    .last
                    .unwrap();
                let mut pull = ExecutionContext::new(&db, parameters(&input));
                assert_eq!(
                    run(&mut pull, &plan).await.unwrap(),
                    expected,
                    "skip={skip} take={take} operators={operators:?}"
                );
            }
        }
    }
    db.close().await.unwrap();
}

#[tokio::test]
async fn count_and_exists_stop_their_filtered_input() {
    let db = test_support::open_db("pull-count-exists").await;
    let mut ids = Vec::new();
    for name in ["other", "match", "match"] {
        ids.push(test_support::add_user(&db, name).await);
    }
    let terminals = [
        (
            exec::ExecOp::Project {
                projection: ir::ProjectionPlan::Exists,
            },
            ExecutionValue::Bool(true),
        ),
        (
            exec::ExecOp::Count {
                plan: Box::new(exec::ExecCountPlan::InputRows {
                    window: exec::ExecCountWindowPlan::identity()
                        .then_limit(exec::ExecUsizeExpr::literal(1)),
                }),
            },
            ExecutionValue::Count(1),
        ),
        (
            exec::ExecOp::Count {
                plan: Box::new(exec::ExecCountPlan::Stream(exec::ExecCountStreamPlan {
                    cursor: exec::ExecCountCursorPlan::InputRows,
                    window: exec::ExecCountWindowPlan::identity()
                        .then_limit(exec::ExecUsizeExpr::literal(1)),
                })),
            },
            ExecutionValue::Count(1),
        ),
    ];
    for (terminal, expected) in terminals {
        let mut ctx = ExecutionContext::new(&db, parameters(&ids));
        assert_eq!(
            run(
                &mut ctx,
                &linear(vec![source(), filter_name("match"), terminal])
            )
            .await
            .unwrap(),
            expected
        );
        assert_eq!(ctx.projection_read_snapshot().property_gets, 2);
    }
    for input in [
        exec::ExecCountCursorPlan::NodeRuntimeInput(exec::ExecRuntimeInputPlan::Param(
            test_support::name("ids"),
        )),
        exec::ExecCountCursorPlan::NodeFullScan,
    ] {
        let predicate = ir::PredicatePlan::new(Predicate::eq("name", "match")).unwrap();
        let cursor = exec::ExecCountCursorPlan::Filter {
            input: Box::new(input),
            predicate,
        };
        let plan = linear(vec![exec::ExecOp::Count {
            plan: Box::new(exec::ExecCountPlan::Stream(exec::ExecCountStreamPlan {
                cursor,
                window: exec::ExecCountWindowPlan::identity()
                    .then_limit(exec::ExecUsizeExpr::literal(1)),
            })),
        }]);
        let mut ctx = ExecutionContext::new(&db, parameters(&ids));
        assert_eq!(
            run(&mut ctx, &plan).await.unwrap(),
            ExecutionValue::Count(1)
        );
        assert_eq!(ctx.projection_read_snapshot().property_gets, 2);
    }
    db.close().await.unwrap();
}

#[tokio::test]
async fn activated_inner_bound_is_validated_even_under_zero_demand() {
    let db = test_support::open_db("pull-invalid-inner-bound").await;
    let plan = linear(vec![
        source(),
        exec::ExecOp::Limit {
            count: ir::StreamBoundPlan::Expr(
                ir::StreamBoundExprPlan::new(Expr::Param("missing".into())).unwrap(),
            ),
        },
        limit(0),
    ]);
    let mut ctx = ExecutionContext::new(&db, parameters(&[]));
    assert!(run(&mut ctx, &plan).await.is_err());
    assert_eq!(ctx.projection_read_snapshot().property_gets, 0);
    db.close().await.unwrap();
}

#[tokio::test]
async fn pure_union_does_not_open_unused_children() {
    let db = test_support::open_db("pull-union-unused-child").await;
    let first = test_support::add_user(&db, "first").await;
    let bad_source = exec::ExecOp::Access {
        plan: Box::new(exec::ExecAccessPlan::Node(
            exec::ExecNodeAccessPlan::FromParam {
                param: test_support::name("missing"),
            },
        )),
    };
    let plan = test_support::executable(
        ir::PlanKind::Read,
        vec![
            test_support::step(1, vec![], source()),
            test_support::step(2, vec![], bad_source),
            test_support::step(
                3,
                vec![
                    exec::ExecStepId::new(1).unwrap(),
                    exec::ExecStepId::new(2).unwrap(),
                ],
                exec::ExecOp::Merge {
                    mode: exec::ExecMergeMode::Union,
                },
            ),
            test_support::step(4, vec![exec::ExecStepId::new(3).unwrap()], limit(1)),
        ],
        4,
    );
    let mut ctx = ExecutionContext::new(&db, parameters(&[first]));
    assert_eq!(
        run(&mut ctx, &plan).await.unwrap(),
        ExecutionValue::Stream(vec![ExecutionRow::current(ElementRef::Node(first))])
    );
    db.close().await.unwrap();
}

fn child(ops: Vec<exec::ExecOp>) -> exec::ExecutableSubplan {
    let plan = linear(ops);
    test_support::subplan(plan.steps().to_vec(), plan.root().get())
}

fn context_source() -> exec::ExecOp {
    exec::ExecOp::Variable {
        op: exec::ExecVariableOp::SourceInject {
            variable: test_support::name("$context"),
        },
    }
}

#[tokio::test]
async fn pure_branches_stop_before_unused_children_and_restore_context() {
    let db = test_support::open_db("pull-branch-demand").await;
    let first = test_support::add_user(&db, "match").await;
    let later = test_support::add_user(&db, "other").await;
    let identity = child(vec![context_source(), filter_name("match")]);
    let bad = child(vec![exec::ExecOp::Access {
        plan: Box::new(exec::ExecAccessPlan::Node(
            exec::ExecNodeAccessPlan::FromParam {
                param: test_support::name("missing"),
            },
        )),
    }]);
    for branch in [
        exec::ExecBranchPlan::Union(
            ir::AtLeast::try_from_vec(vec![identity.clone(), bad.clone()]).unwrap(),
        ),
        exec::ExecBranchPlan::Coalesce(
            ir::AtLeast::try_from_vec(vec![identity.clone(), bad.clone()]).unwrap(),
        ),
        exec::ExecBranchPlan::Optional(Box::new(identity)),
    ] {
        let mut ctx = ExecutionContext::new(&db, parameters(&[first, later]));
        let original = ExecutionValue::Count(42);
        ctx.variables
            .insert(test_support::name("$context"), original.clone());
        let plan = linear(vec![
            source(),
            exec::ExecOp::Branch { plan: branch },
            limit(1),
        ]);
        ctx.execute_steps(
            plan.steps(),
            plan.execution_order(),
            plan.root(),
            plan.execution_program(),
        )
        .await
        .unwrap();
        assert_eq!(
            ctx.variable_value(&test_support::name("$context")).unwrap(),
            &original
        );
        let result = ctx
            .finish(plan.root(), &exec::ExecutableReturns::None)
            .unwrap()
            .last
            .unwrap();
        assert_eq!(
            result,
            ExecutionValue::Stream(vec![ExecutionRow::current(ElementRef::Node(first))])
        );
        assert_eq!(ctx.projection_read_snapshot().property_gets, 1);
    }
    let mut ctx = ExecutionContext::new(&db, parameters(&[first]));
    assert!(run(
        &mut ctx,
        &linear(vec![
            source(),
            exec::ExecOp::Branch {
                plan: exec::ExecBranchPlan::Optional(Box::new(bad))
            },
            limit(1)
        ])
    )
    .await
    .is_err());
    assert!(ctx.variable_value(&test_support::name("$context")).is_err());
    db.close().await.unwrap();
}

#[tokio::test]
async fn repeat_before_can_stop_without_starting_body() {
    let db = test_support::open_db("pull-repeat-before-demand").await;
    let first = test_support::add_user(&db, "match").await;
    let body = child(vec![exec::ExecOp::Access {
        plan: Box::new(exec::ExecAccessPlan::Node(
            exec::ExecNodeAccessPlan::FromParam {
                param: test_support::name("unused"),
            },
        )),
    }]);
    for emit in [ir::RepeatEmitPlan::Before, ir::RepeatEmitPlan::All] {
        let mut ctx = ExecutionContext::new(&db, parameters(&[first]));
        let result = run(
            &mut ctx,
            &linear(vec![
                source(),
                exec::ExecOp::Repeat {
                    plan: exec::ExecRepeatPlan {
                        body: Box::new(body.clone()),
                        emit,
                        stop: ir::RepeatStopPlan::MaxDepthOnly,
                        max_depth: std::num::NonZeroUsize::new(3).unwrap(),
                    },
                },
                limit(1),
            ]),
        )
        .await
        .unwrap();
        assert_eq!(
            result,
            ExecutionValue::Stream(vec![ExecutionRow::current(ElementRef::Node(first))])
        );
        assert!(ctx.variable_value(&test_support::name("$context")).is_err());
    }
    db.close().await.unwrap();
}

#[tokio::test]
async fn shared_source_keeps_its_complete_result_for_other_consumers() {
    let db = test_support::open_db("pull-shared-source").await;
    let mut ids = Vec::new();
    for _ in 0..3 {
        ids.push(test_support::add_user(&db, "match").await);
    }
    let plan = test_support::executable(
        ir::PlanKind::Read,
        vec![
            test_support::step(1, vec![], source()),
            test_support::step(2, vec![exec::ExecStepId::new(1).unwrap()], limit(1)),
            test_support::step(
                3,
                vec![exec::ExecStepId::new(1).unwrap()],
                exec::ExecOp::Noop,
            ),
            test_support::step(
                4,
                vec![
                    exec::ExecStepId::new(2).unwrap(),
                    exec::ExecStepId::new(3).unwrap(),
                ],
                exec::ExecOp::Merge {
                    mode: exec::ExecMergeMode::Concat,
                },
            ),
        ],
        4,
    );
    assert!(!plan
        .execution_program()
        .is_absorbed(exec::ExecStepId::new(1).unwrap()));
    let mut ctx = ExecutionContext::new(&db, parameters(&ids));
    let result = run(&mut ctx, &plan).await.unwrap();
    assert_eq!(
        result,
        ExecutionValue::Stream(
            [ids[0]]
                .into_iter()
                .chain(ids)
                .map(|id| ExecutionRow::current(ElementRef::Node(id)))
                .collect()
        )
    );
    db.close().await.unwrap();
}

#[tokio::test]
async fn edge_limit_respects_edge_id_order_across_neighbor_pairs() {
    let db = test_support::open_db("pull-edge-global-order").await;
    let root = test_support::add_user(&db, "root").await;
    let low = test_support::add_user(&db, "low").await;
    let high = test_support::add_user(&db, "high").await;
    let earliest = test_support::add_edge(&db, root, high, "LINK").await;
    test_support::add_edge(&db, root, low, "LINK").await;
    test_support::add_edge(&db, root, high, "LINK").await;
    for direction in [ir::ExpandDirection::Out, ir::ExpandDirection::Both] {
        let mut ctx = ExecutionContext::new(&db, parameters(&[root]));
        let result = run(
            &mut ctx,
            &linear(vec![
                source(),
                exec::ExecOp::Expand {
                    plan: ir::ExpandPlan {
                        direction,
                        label: ir::ExpandLabelPlan::Any,
                        output: ir::ExpandOutput::Edges,
                    },
                },
                limit(1),
                exec::ExecOp::Project {
                    projection: ir::ProjectionPlan::Id,
                },
            ]),
        )
        .await
        .unwrap();
        assert_eq!(
            result,
            ExecutionValue::Scalars(vec![ExecutionScalar::EdgeId(earliest)])
        );
        assert_eq!(ctx.pull_work.snapshot().expansion_parents, 1);
    }
    db.close().await.unwrap();
}

#[tokio::test]
async fn ordered_range_refills_stale_entries_and_preserves_reverse_ties() {
    use crate::encoding::keys;
    use helix_ast::index::RangeIndexDirection;
    use helix_ast::value::PropertyValue;
    use helix_planner::catalog;
    let db = test_support::open_db_with_config(
        test_support::in_memory_config("pull-range-refill").with_range_index("User", "score"),
    )
    .await;
    let mut ids = Vec::new();
    for score in [10, 10, 20, 20] {
        ids.push(
            test_support::add_node_with_properties(
                &db,
                "User",
                vec![("score", PropertyValue::I64(score))],
            )
            .await,
        );
    }
    let key = |id| {
        keys::DataKey::Data {
            scope: keys::scope::DataScope::LegacyUnscoped,
            kind: keys::DataKeyKind::NodeProperty(keys::NodePropertyKey::new(id)),
        }
        .to_bytes()
    };
    // A stale index owner must not consume accepted-row demand.
    db.inner_db().delete(key(ids[0])).await.unwrap();
    for (iteration, expected) in [
        (ir::RangeScanIteration::Forward, vec![ids[1], ids[2]]),
        (ir::RangeScanIteration::Reverse, vec![ids[2], ids[3]]),
    ] {
        let plan = linear(vec![
            exec::ExecOp::Access {
                plan: Box::new(exec::ExecAccessPlan::Node(
                    exec::ExecNodeAccessPlan::RangeIndex {
                        index: catalog::NodeRangeIndexMeta::new(test_support::name(
                            "node_range:User:score:asc",
                        )),
                        key: catalog::ScopedPropertyDirectionKey::try_new(
                            "User",
                            "score",
                            RangeIndexDirection::Asc,
                        )
                        .unwrap(),
                        range: ir::IndexRange::All,
                        iteration,
                    },
                )),
            },
            limit(2),
            exec::ExecOp::Project {
                projection: ir::ProjectionPlan::Id,
            },
        ]);
        let mut ctx = ExecutionContext::new(&db, context::ParamBindings::default());
        assert_eq!(
            run(&mut ctx, &plan).await.unwrap(),
            ExecutionValue::Scalars(expected.into_iter().map(ExecutionScalar::NodeId).collect())
        );
    }
    db.close().await.unwrap();
}

#[tokio::test]
async fn limits_before_and_after_mutations_preserve_selected_effects() {
    use helix_ast::{batch, graph::NodeRef, query::QueryRequest, traversal};
    let db = test_support::open_db("pull-mutation-demand-boundary").await;
    let mut ids = Vec::new();
    for _ in 0..3 {
        ids.push(test_support::add_user(&db, "match").await);
    }
    for take in [0_usize, 1, 3] {
        for before in [true, false] {
            db.query(QueryRequest::write(
                batch::write_batch()
                    .var_as(
                        "reset",
                        traversal::g()
                            .n(NodeRef::all())
                            .set_property("marker", 0_i64),
                    )
                    .returning(Vec::<String>::new()),
            ))
            .await
            .unwrap();
            let update = if before {
                traversal::g()
                    .n(NodeRef::all())
                    .limit(take)
                    .set_property("marker", 1_i64)
            } else {
                traversal::g()
                    .n(NodeRef::all())
                    .set_property("marker", 1_i64)
                    .limit(take)
            };
            db.query(QueryRequest::write(
                batch::write_batch()
                    .var_as("updated", update)
                    .returning(Vec::<String>::new()),
            ))
            .await
            .unwrap();
            let ctx = ExecutionContext::new(&db, context::ParamBindings::default());
            for (index, id) in ids.iter().enumerate() {
                let value = ctx
                    .row_property(
                        &ExecutionRow::current(ElementRef::Node(*id)),
                        &test_support::name("marker"),
                    )
                    .await
                    .unwrap();
                assert_eq!(
                    value,
                    Some(DbPropertyValue::I64(i64::from(!before || index < take))),
                    "take={take}, before={before}, index={index}"
                );
            }
        }
    }
    db.close().await.unwrap();
}

#[tokio::test]
async fn terminal_adapters_reject_invalid_input_shapes() {
    let db = test_support::open_db("pull-invalid-value-shapes").await;
    let id = test_support::add_user(&db, "match").await;
    for (middle, operation) in [
        (
            vec![
                exec::ExecOp::Reserved {
                    op: ir::ReservedOp::Fold,
                },
                exec::ExecOp::Project {
                    projection: ir::ProjectionPlan::Exists,
                },
            ],
            "project expected stream input",
        ),
        (
            vec![
                exec::ExecOp::Project {
                    projection: ir::ProjectionPlan::Id,
                },
                exec::ExecOp::Count {
                    plan: Box::new(exec::ExecCountPlan::InputRows {
                        window: exec::ExecCountWindowPlan::identity(),
                    }),
                },
            ],
            "count plan expected rows",
        ),
        (
            vec![exec::ExecOp::Count {
                plan: Box::new(exec::ExecCountPlan::InputScalars {
                    window: exec::ExecCountWindowPlan::identity(),
                }),
            }],
            "count plan expected scalar items",
        ),
    ] {
        let plan = linear(std::iter::once(source()).chain(middle).collect());
        let mut ctx = ExecutionContext::new(&db, parameters(&[id]));
        let error = run(&mut ctx, &plan).await.unwrap_err();
        assert!(error.to_string().contains(operation), "{error}");
    }
    db.close().await.unwrap();
}
