//! Structural count cursors use the same pull operators as row-returning plans.
//! The selected leaf primitives and identity rules remain unchanged.
use super::*;

enum Program<'a> {
    Input,
    Empty,
    Leaf(&'a exec::ExecCountCursorPlan),
    Apply {
        input: Box<Self>,
        op: Box<exec::ExecOp>,
    },
    Window {
        input: Box<Self>,
        window: &'a exec::ExecCountWindowPlan,
    },
    Set {
        inputs: Vec<Self>,
        intersect: bool,
    },
    OrderedDistinct(Box<Self>),
}

impl<'a> Program<'a> {
    fn new(plan: &'a exec::ExecCountCursorPlan) -> Self {
        use exec::ExecCountCursorPlan as C;
        let apply = |input: &'a C, op| Self::Apply {
            input: Box::new(Self::new(input)),
            op: Box::new(op),
        };
        let source = |plan| Self::Apply {
            input: Box::new(Self::Empty),
            op: Box::new(exec::ExecOp::Access {
                plan: Box::new(plan),
            }),
        };
        match plan {
            C::InputRows => Self::Input,
            C::NodeFullScan => source(exec::ExecAccessPlan::Node(
                exec::ExecNodeAccessPlan::AllScan,
            )),
            C::EdgeFullScan => source(exec::ExecAccessPlan::Edge(
                exec::ExecEdgeAccessPlan::AllScan,
            )),
            C::NodeAuthoritativeScan(predicate) => source(exec::ExecAccessPlan::Node(
                exec::ExecNodeAccessPlan::AuthoritativeScan {
                    predicate: predicate.clone(),
                },
            )),
            C::EdgeAuthoritativeScan(predicate) => source(exec::ExecAccessPlan::Edge(
                exec::ExecEdgeAccessPlan::AuthoritativeScan {
                    predicate: predicate.clone(),
                },
            )),
            C::NodeBitmap(bitmap) => source(exec::ExecAccessPlan::Node(
                exec::ExecNodeAccessPlan::Bitmap {
                    bitmap: bitmap.clone(),
                },
            )),
            C::EdgeBitmap(bitmap) => source(exec::ExecAccessPlan::Edge(
                exec::ExecEdgeAccessPlan::Bitmap {
                    bitmap: bitmap.clone(),
                },
            )),
            C::Filter { input, predicate } => apply(
                input,
                exec::ExecOp::Filter {
                    predicate: predicate.clone(),
                },
            ),
            C::Expand { input, plan } => apply(input, exec::ExecOp::Expand { plan: plan.clone() }),
            C::Order { input, plan } => apply(input, exec::ExecOp::Order { plan: plan.clone() }),
            C::VectorSearch { input, plan } => {
                apply(input, exec::ExecOp::VectorSearch { plan: plan.clone() })
            }
            C::TextSearch { input, plan } => {
                apply(input, exec::ExecOp::TextSearch { plan: plan.clone() })
            }
            C::Variable { input, op } => apply(
                input,
                exec::ExecOp::Variable {
                    op: exec::ExecVariableOp::Stream(op.to_stream_op()),
                },
            ),
            C::Distinct {
                input,
                plan: exec::ExecCountDistinctPlan::HashRows,
            } => apply(input, exec::ExecOp::Distinct),
            C::Distinct {
                input,
                plan: exec::ExecCountDistinctPlan::OrderedRows,
            } => Self::OrderedDistinct(Box::new(Self::new(input))),
            C::Window { input, window } => Self::Window {
                input: Box::new(Self::new(input)),
                window,
            },
            C::Union { driver, rest } | C::Intersect { driver, rest } => Self::Set {
                inputs: std::iter::once(driver.as_ref())
                    .chain(rest.as_ref())
                    .map(Self::new)
                    .collect(),
                intersect: matches!(plan, C::Intersect { .. }),
            },
            C::NodeRange(plan) => source(exec::ExecAccessPlan::Node(
                exec::ExecNodeAccessPlan::RangeIndex {
                    index: plan.index.clone(),
                    key: plan.key.clone(),
                    range: plan.range.clone(),
                    iteration: ir::RangeScanIteration::Forward,
                },
            )),
            C::EdgeRange(plan) => source(exec::ExecAccessPlan::Edge(
                exec::ExecEdgeAccessPlan::RangeIndex {
                    index: plan.index.clone(),
                    key: plan.key.clone(),
                    range: plan.range.clone(),
                    iteration: ir::RangeScanIteration::Forward,
                },
            )),
            C::EmptyRows
            | C::NodeUnique { .. }
            | C::NodePointReads(_)
            | C::EdgePointReads(_)
            | C::NodeRuntimeInput(_)
            | C::EdgeRuntimeInput(_)
            | C::RuntimeInput(_)
            | C::NodeLabelBitmap(_)
            | C::EdgeLabelBitmap(_)
            | C::NodeVectorSearch { .. }
            | C::EdgeVectorSearch { .. }
            | C::NodeTextSearch { .. }
            | C::EdgeTextSearch { .. }
            | C::NodeDynamicEquality { .. }
            | C::EdgeDynamicEquality { .. }
            | C::NodeDynamicMembership { .. }
            | C::EdgeDynamicMembership { .. } => Self::Leaf(plan),
        }
    }

    fn validate_bounds(&self, ctx: &ExecutionContext<'_>) -> Result<()> {
        match self {
            Self::Window { input, window } => {
                input.validate_bounds(ctx)?;
                ctx.count_window(window)?;
            }
            Self::Apply { input, .. } | Self::OrderedDistinct(input) => {
                input.validate_bounds(ctx)?
            }
            Self::Set { inputs, .. } => {
                for input in inputs {
                    input.validate_bounds(ctx)?;
                }
            }
            Self::Empty | Self::Input | Self::Leaf(_) => {}
        }
        Ok(())
    }

    fn cursor<'b>(
        &'b self,
        ctx: &ExecutionContext<'_>,
        dependency: &mut Option<Cursor<'b>>,
    ) -> Result<Cursor<'b>> {
        let node = match self {
            Self::Empty => return Cursor::materialized(ExecutionValue::Stream(Vec::new())),
            Self::Input => {
                let input = dependency.take().ok_or_else(|| {
                    HelixDbError::InvariantViolation(
                        "count cursor consumed its row dependency more than once".into(),
                    )
                })?;
                if input.shape != Shape::Rows {
                    return Err(HelixDbError::InvariantViolation(
                        "count plan expected rows".into(),
                    ));
                }
                return Ok(input);
            }
            Self::Leaf(plan) => Node::CountLeaf {
                plan,
                dependency: None,
                pending: None,
            },
            Self::Apply { input, op } => {
                if let exec::ExecOp::Access { plan } = op.as_ref() {
                    match plan.as_ref() {
                        exec::ExecAccessPlan::Node(exec::ExecNodeAccessPlan::RangeIndex {
                            index,
                            key,
                            ..
                        }) => count::validate_range_index("node_range:", &index.index_id, key)?,
                        exec::ExecAccessPlan::Edge(exec::ExecEdgeAccessPlan::RangeIndex {
                            index,
                            key,
                            ..
                        }) => count::validate_range_index("edge_range:", &index.index_id, key)?,
                        exec::ExecAccessPlan::Node(_)
                        | exec::ExecAccessPlan::Edge(_)
                        | exec::ExecAccessPlan::Limited(_) => {}
                    }
                }
                return input.cursor(ctx, dependency)?.wrap(ctx, op);
            }
            Self::Window { input, window } => {
                let input = input.cursor(ctx, dependency)?;
                let window = ctx.count_window(window)?;
                Node::Window {
                    input: Box::new(input),
                    skip: window.skip,
                    remaining: window.take.map_or(Demand::All, Demand::take),
                }
            }
            Self::OrderedDistinct(input) => Node::OrderedDistinct {
                input: Box::new(input.cursor(ctx, dependency)?),
                previous: None,
            },
            Self::Set { inputs, intersect } => {
                let mut inputs = inputs
                    .iter()
                    .map(|input| input.cursor(ctx, dependency))
                    .collect::<Result<Vec<_>>>()?;
                let driver = Box::new(inputs.remove(0));
                Node::CountSet {
                    driver,
                    inputs: inputs.into_iter(),
                    current: None,
                    intersect: *intersect,
                    sets: None,
                    seen: BTreeSet::new(),
                    in_driver: true,
                }
            }
        };
        Ok(Cursor {
            shape: Shape::Rows,
            node,
            produced_rows: 0,
            row_mode: false,
            name: "count cursor",
        })
    }
}

impl ExecutionContext<'_> {
    pub(in crate::execution::interpreter) async fn pull_count_rows(
        &mut self,
        plan: &exec::ExecCountCursorPlan,
        dependency: &mut Option<ExecutionValue>,
    ) -> Result<Vec<ExecutionRow>> {
        let program = Program::new(plan);
        let mut input = dependency.take().map(Cursor::materialized).transpose()?;
        let mut cursor = program.cursor(self, &mut input)?;
        let value = cursor.drain(self).await?;
        self.stream_rows(value, "count cursor")
    }

    pub(in crate::execution::interpreter) async fn pull_count_cardinality(
        &mut self,
        plan: &exec::ExecCountCursorPlan,
        dependency: &mut Option<ExecutionValue>,
        skip: usize,
        take: Option<usize>,
    ) -> Result<usize> {
        let program = Program::new(plan);
        let mut dependency = dependency.take().map(Cursor::materialized).transpose()?;
        let input = program.cursor(self, &mut dependency)?;
        let mut cursor = Cursor {
            shape: Shape::Rows,
            node: Node::Window {
                input: Box::new(input),
                skip,
                remaining: take.map_or(Demand::All, Demand::take),
            },
            produced_rows: 0,
            row_mode: false,
            name: "count cursor",
        };
        let mut count = 0usize;
        while cursor.next(self).await?.is_some() {
            count = count
                .checked_add(1)
                .ok_or_else(|| HelixDbError::Query("count overflow".into()))?;
        }
        Ok(count)
    }
}

pub(super) async fn stream<'a>(
    ctx: &mut ExecutionContext<'_>,
    plan: &'a exec::ExecCountStreamPlan,
    input: Cursor<'a>,
) -> Result<ExecutionValue> {
    let program = Program::new(&plan.cursor);
    let mut dependency = Some(input);
    let input = program.cursor(ctx, &mut dependency)?;
    let window = ctx.count_window(&plan.window)?;
    let mut cursor = Cursor {
        shape: Shape::Rows,
        node: Node::Window {
            input: Box::new(input),
            skip: window.skip,
            remaining: window.take.map_or(Demand::All, Demand::take),
        },
        produced_rows: 0,
        row_mode: false,
        name: "count cursor",
    };
    let mut count = 0usize;
    while cursor.next(ctx).await?.is_some() {
        count = count
            .checked_add(1)
            .ok_or_else(|| HelixDbError::Query("count overflow".into()))?;
    }
    Ok(ExecutionValue::Count(count))
}

pub(in crate::execution::interpreter) fn validate_bounds(
    ctx: &ExecutionContext<'_>,
    plan: &exec::ExecCountPlan,
) -> Result<()> {
    if let exec::ExecCountPlan::Stream(plan) = plan {
        Program::new(&plan.cursor).validate_bounds(ctx)?;
    }
    if let Some(window) = count::count_plan_window(plan) {
        ctx.count_window(window)?;
    }
    Ok(())
}
