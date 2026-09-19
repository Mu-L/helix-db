//! Validated pull regions. These describe execution, never access-path selection.
use std::collections::{BTreeMap, BTreeSet};

use super::{ExecCondition, ExecExecutionOrder, ExecOp, ExecStep, ExecStepId, ExecVariableOp};
use crate::ir;

/// Whether an operation can participate in a demand-driven region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecPullCapability {
    /// Output can be produced incrementally.
    Incremental,
    /// Some ordered or membership state must be prepared first.
    Prepared,
    /// Any demanded output needs the complete input.
    FullInput,
    /// Observable effects or scope require ordinary execution.
    Boundary,
}

impl ExecPullCapability {
    /// Exhaustive operation contract; new executable operations require a decision.
    pub fn of(op: &ExecOp) -> Self {
        match op {
            ExecOp::Access { .. }
            | ExecOp::KvRead(_)
            | ExecOp::Expand { .. }
            | ExecOp::Merge { .. } => Self::Prepared,
            ExecOp::Filter { .. }
            | ExecOp::Limit { .. }
            | ExecOp::Skip { .. }
            | ExecOp::Range { .. }
            | ExecOp::Distinct
            | ExecOp::Noop => Self::Incremental,
            ExecOp::Project { projection } => match projection {
                ir::ProjectionPlan::Exists
                | ir::ProjectionPlan::Id
                | ir::ProjectionPlan::Values(_)
                | ir::ProjectionPlan::ValueMap(_)
                | ir::ProjectionPlan::Project(_)
                | ir::ProjectionPlan::ProjectBindings { .. }
                | ir::ProjectionPlan::Label
                | ir::ProjectionPlan::EdgeProperties => Self::Incremental,
            },
            ExecOp::Count { .. }
            | ExecOp::Order { .. }
            | ExecOp::Aggregate { .. }
            | ExecOp::VectorSearch { .. }
            | ExecOp::TextSearch { .. }
            | ExecOp::ShortestPath { .. } => Self::FullInput,
            ExecOp::Variable { op } => match op {
                ExecVariableOp::SourceInject { .. } => Self::Incremental,
                ExecVariableOp::Stream(op) => match op {
                    ir::StreamVariableOp::As(_) | ir::StreamVariableOp::Store(_) => Self::Boundary,
                    ir::StreamVariableOp::Bind(_)
                    | ir::StreamVariableOp::Within(_)
                    | ir::StreamVariableOp::Without(_)
                    | ir::StreamVariableOp::Select(_)
                    | ir::StreamVariableOp::Inject(_) => Self::Incremental,
                },
            },
            ExecOp::Reserved { op } => match op {
                ir::ReservedOp::Fold | ir::ReservedOp::Unfold => Self::FullInput,
                ir::ReservedOp::Path
                | ir::ReservedOp::SimplePath
                | ir::ReservedOp::WithSack(_)
                | ir::ReservedOp::SackSet(_)
                | ir::ReservedOp::SackAdd(_)
                | ir::ReservedOp::SackGet => Self::Incremental,
            },
            ExecOp::Branch { plan } => {
                let pure = match plan {
                    super::ExecBranchPlan::Union(branches) => {
                        branches.as_ref().iter().all(Self::pure_subplan)
                    }
                    super::ExecBranchPlan::Coalesce(branches) => {
                        branches.as_ref().iter().all(Self::pure_subplan)
                    }
                    super::ExecBranchPlan::Optional(branch) => Self::pure_subplan(branch),
                    super::ExecBranchPlan::Choose { then_plan, .. } => {
                        Self::pure_subplan(then_plan)
                    }
                    super::ExecBranchPlan::ChooseElse {
                        then_plan,
                        else_plan,
                        ..
                    } => Self::pure_subplan(then_plan) && Self::pure_subplan(else_plan),
                };
                if pure
                    && matches!(
                        plan,
                        super::ExecBranchPlan::Choose { .. }
                            | super::ExecBranchPlan::ChooseElse { .. }
                    )
                {
                    Self::FullInput
                } else if pure {
                    Self::Prepared
                } else {
                    Self::Boundary
                }
            }
            ExecOp::Repeat { plan } => {
                if Self::pure_subplan(&plan.body) {
                    Self::Prepared
                } else {
                    Self::Boundary
                }
            }
            ExecOp::ForEach { .. }
            | ExecOp::Mutation { .. }
            | ExecOp::IndexDdl { .. }
            | ExecOp::Barrier { .. } => Self::Boundary,
        }
    }
    /// A pure, exclusively consumed tree can suspend without exposing a frame's
    /// partial results or skipping effects. Other subplans remain boundaries.
    pub fn pure_subplan(plan: &super::ExecutableSubplan) -> bool {
        let mut uses = BTreeMap::<ExecStepId, usize>::new();
        for step in plan.steps() {
            for dependency in &step.dependencies {
                *uses.entry(*dependency).or_default() += 1;
            }
        }
        *uses.entry(plan.root()).or_default() += 1;
        // Purity is independent of whether this subplan itself has a window:
        // an enclosing branch can supply demand to an exclusive child tree.
        plan.steps().iter().all(|step| {
            uses.get(&step.id) == Some(&1)
                && matches!(step.condition, ExecCondition::Always)
                && matches!(step.output, ir::BatchOutputPlan::Discard)
                && Self::of(&step.op) != Self::Boundary
        })
    }
}

/// An exclusive producer tree, stored in dependency order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecPullRegion {
    steps: Vec<ExecStepId>,
}

impl ExecPullRegion {
    /// Original executable IDs, including the terminal step.
    pub fn steps(&self) -> &[ExecStepId] {
        &self.steps
    }
}

/// Derived, non-serialized execution regions for one validated DAG.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExecProgram {
    regions: BTreeMap<ExecStepId, ExecPullRegion>,
    absorbed: BTreeSet<ExecStepId>,
}

impl ExecProgram {
    pub(in crate::exec) fn derive(
        steps: &[ExecStep],
        order: &ExecExecutionOrder,
        root: ExecStepId,
    ) -> Self {
        let by_id = steps
            .iter()
            .map(|step| (step.id, step))
            .collect::<BTreeMap<_, _>>();
        let mut uses = BTreeMap::<ExecStepId, usize>::new();
        for step in steps {
            for id in &step.dependencies {
                *uses.entry(*id).or_default() += 1;
            }
            if let ExecCondition::PreviousStepNotEmpty { dependency } = step.condition {
                *uses.entry(dependency).or_default() += 1;
            }
        }
        *uses.entry(root).or_default() += 1;
        // Captures and effects separate epochs even if an unrelated ready step
        // appears between a producer and consumer in topological order.
        let mut epochs = BTreeMap::new();
        let mut epoch = 0usize;
        for id in order.step_ids() {
            let step = by_id[&id];
            if ExecPullCapability::of(&step.op) == ExecPullCapability::Boundary {
                epoch += 1;
            }
            epochs.insert(id, epoch);
            if !matches!(step.output, ir::BatchOutputPlan::Discard) {
                epoch += 1;
            }
        }
        let mut program = Self::default();
        let ids = order.step_ids().collect::<Vec<_>>();
        for id in ids.into_iter().rev() {
            if program.absorbed.contains(&id) {
                continue;
            }
            let terminal = by_id[&id];
            if ExecPullCapability::of(&terminal.op) == ExecPullCapability::Boundary {
                continue;
            }
            let mut members = BTreeSet::from([id]);
            let mut pending = vec![id];
            while let Some(current) = pending.pop() {
                for dependency in &by_id[&current].dependencies {
                    let parent = by_id[dependency];
                    if uses[dependency] != 1
                        || !matches!(parent.output, ir::BatchOutputPlan::Discard)
                        || parent.condition != terminal.condition
                        || epochs[dependency] != epochs[&id]
                        || ExecPullCapability::of(&parent.op) == ExecPullCapability::Boundary
                    {
                        continue;
                    }
                    if members.insert(*dependency) {
                        pending.push(*dependency);
                    }
                }
            }
            // Without a window or terminal cardinality consumer, ordinary
            // whole-value operators avoid per-row polling overhead. Their
            // existing implementations also serve effect/materialization edges.
            let needs_demand = members.iter().any(|id| {
                matches!(
                    by_id[id].op,
                    ExecOp::Limit { .. }
                        | ExecOp::Range { .. }
                        | ExecOp::Count { .. }
                        | ExecOp::Project {
                            projection: ir::ProjectionPlan::Exists
                        }
                )
            });
            if members.len() > 1 && needs_demand {
                let steps = order.step_ids().filter(|id| members.contains(id)).collect();
                members.remove(&id);
                program.absorbed.extend(members);
                program.regions.insert(id, ExecPullRegion { steps });
            }
        }
        program
    }

    /// Regions in deterministic terminal-ID order. Original step IDs can be
    /// joined to `ExecPullCapability::of` for preparation and barrier details.
    pub fn regions(&self) -> impl Iterator<Item = (ExecStepId, &ExecPullRegion)> {
        self.regions.iter().map(|(id, region)| (*id, region))
    }

    /// A source step executed only when its region requests rows.
    pub fn is_absorbed(&self, id: ExecStepId) -> bool {
        self.absorbed.contains(&id)
    }

    /// Region whose observable output belongs to this step.
    pub fn region(&self, id: ExecStepId) -> Option<&ExecPullRegion> {
        self.regions.get(&id)
    }
}
