//! Ordered-result and read-count oracle for limit execution.
use super::*;

#[tokio::test]
async fn filtered_limit_characterizes_eager_property_reads() {
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
    ctx.execute_steps(plan.steps(), plan.execution_order(), plan.root())
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
    assert_eq!(ctx.projection_read_snapshot().property_gets, 3);
    db.close().await.unwrap();
}
