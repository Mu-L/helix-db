//! Fixed-shape secondary-equality write-path benchmark support.
//!
//! Index creation and physical planning finish before the measured phase. The
//! fixture deliberately maps every indexed field to one shared value so V3
//! creates the worst-case 500,000 entity-suffixed rows and bitmap formats can
//! collapse the same logical state to 50 rows.

use std::sync::Arc;
use std::time::{Duration, Instant};

use helix_ast::prelude::*;
use helix_ast::value::PropertyValue as PlannerPropertyValue;
use helix_planner::{catalog, context, cost, exec, ir, properties, trace};
use serde::Serialize;

use crate::config::SecondaryIndexDefinition;
use crate::encoding::v2::keys::{ScopedKey, RecordKind};
use crate::encoding::v1::keys::tenant::DataScope;
use crate::encoding::v1::keys::Key;
use crate::execution::interpreter::ExecutionValue;
use crate::index_v2::ValidatedDynamicIndexDefinition;
use crate::{HelixDB, HelixDbSource, HelixStorage, Result};

const INDEX_COUNT: usize = 50;
const ENTITY_COUNT: usize = 10_000;
const CONCURRENT_WRITERS: usize = 32;
const LABEL: &str = "SecondaryEqualityHotPathNode";
const SHARED_VALUE: &str = "shared";

/// One insertion scheduling mode in the fixed hot-path workload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SecondaryEqualityInsertMode {
    /// One writer executes every transaction in order.
    Sequential,
    /// Thirty-two writers execute disjoint logical insert counts concurrently.
    Concurrent,
}

/// Measured insertion outcome, excluding database and index setup.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SecondaryEqualityInsertSample {
    pub mode: SecondaryEqualityInsertMode,
    pub indexes: usize,
    pub entities: usize,
    pub writers: usize,
    pub elapsed_nanos: u128,
    pub throughput_per_second: f64,
    pub median_latency_nanos: u64,
    pub p95_latency_nanos: u64,
    pub conflicts: u64,
    pub retries: u64,
    pub allocations: u64,
    pub allocated_bytes: u64,
}

impl SecondaryEqualityInsertSample {
    /// Attaches process-global allocator observations captured by the harness.
    pub fn with_allocations(mut self, allocations: u64, allocated_bytes: u64) -> Self {
        self.allocations = allocations;
        self.allocated_bytes = allocated_bytes;
        self
    }
}

/// Post-write structural and read-path observations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SecondaryEqualityInspection {
    pub physical_secondary_rows: u64,
    pub equality_result_count: usize,
    pub equality_lookup_nanos: u128,
}

/// Prepared empty database with 50 Active nonunique equality indexes.
pub struct SecondaryEqualityHotPathFixture {
    db: Arc<HelixDB>,
    insert_plan: Arc<exec::ExecutablePlan>,
    lookup_plan: exec::ExecutablePlan,
}

impl SecondaryEqualityHotPathFixture {
    /// Creates the fixed fixture without including setup in benchmark timings.
    pub async fn open(database: impl Into<String>) -> Result<Self> {
        assert_eq!(INDEX_COUNT, 50, "hot-path index shape is frozen");
        assert_eq!(ENTITY_COUNT, 10_000, "hot-path entity shape is frozen");
        assert_eq!(
            CONCURRENT_WRITERS, 32,
            "hot-path concurrency shape is frozen"
        );

        let db = Arc::new(
            HelixDB::open(HelixDbSource::InMemory {
                database: database.into(),
            })
            .await?,
        );
        db.wait_for_startup_cache_warm().await;

        for ordinal in 0..INDEX_COUNT {
            let definition =
                SecondaryIndexDefinition::node_equality(LABEL, property_name(ordinal))?;
            db.install_index_for_tests(ValidatedDynamicIndexDefinition::try_from(definition)?)
                .await?;
        }

        let properties = (0..INDEX_COUNT)
            .map(|ordinal| (property_name(ordinal), PropertyInput::from(SHARED_VALUE)))
            .collect();
        let insert = write_batch().var_as("node", g().add_n(LABEL, properties));
        let insert_plan = helix_planner::planning::plan_write_batch(
            &insert,
            &db.planner_context(context::ParamBindings::default()),
        )
        .map_err(|error| crate::HelixDbError::Query(error.to_string()))?;

        Ok(Self {
            db,
            insert_plan: Arc::new(insert_plan),
            lookup_plan: equality_search_plan(),
        })
    }

    /// Executes the fixed insertion workload, retrying only transaction conflicts.
    pub async fn insert(
        &self,
        mode: SecondaryEqualityInsertMode,
    ) -> Result<SecondaryEqualityInsertSample> {
        match mode {
            SecondaryEqualityInsertMode::Sequential => self.insert_sequential().await,
            SecondaryEqualityInsertMode::Concurrent => self.insert_concurrent().await,
        }
    }

    /// Counts physical rows and times one full-cardinality equality lookup.
    pub async fn inspect(&self) -> Result<SecondaryEqualityInspection> {
        let HelixStorage::Writer(writer) = self.db.storage() else {
            unreachable!("hot-path benchmark opens a writer")
        };
        let prefix = Key::data_prefix(
            DataScope::LegacyUnscoped,
            ScopedKey::logical_prefix(RecordKind::SecondaryEntry),
        );
        let mut rows = writer.db().scan_prefix(&prefix, ..).await?;
        let mut physical_secondary_rows = 0_u64;
        while rows.next().await?.is_some() {
            physical_secondary_rows = physical_secondary_rows.saturating_add(1);
        }

        let lookup_started = Instant::now();
        let result = self
            .db
            .execute(&self.lookup_plan, context::ParamBindings::default())
            .await?;
        let equality_lookup_nanos = lookup_started.elapsed().as_nanos();
        let Some(ExecutionValue::Scalars(values)) = result.last else {
            return Err(crate::HelixDbError::InvariantViolation(
                "hot-path equality lookup did not return projected scalar IDs".to_string(),
            ));
        };

        Ok(SecondaryEqualityInspection {
            physical_secondary_rows,
            equality_result_count: values.len(),
            equality_lookup_nanos,
        })
    }

    /// Closes this fixture and its background workers.
    pub async fn close(&self) -> Result<()> {
        self.db.close().await
    }

    async fn insert_sequential(&self) -> Result<SecondaryEqualityInsertSample> {
        let started = Instant::now();
        let mut latencies = Vec::with_capacity(ENTITY_COUNT);
        let mut conflicts = 0_u64;
        let mut retries = 0_u64;
        for _ in 0..ENTITY_COUNT {
            loop {
                let operation_started = Instant::now();
                match self
                    .db
                    .execute(&self.insert_plan, context::ParamBindings::default())
                    .await
                {
                    Ok(_) => {
                        latencies.push(duration_nanos(operation_started.elapsed()));
                        break;
                    }
                    Err(error) if error.is_transaction_conflict() => {
                        conflicts = conflicts.saturating_add(1);
                        retries = retries.saturating_add(1);
                    }
                    Err(error) => return Err(error),
                }
            }
        }
        Ok(insert_sample(
            SecondaryEqualityInsertMode::Sequential,
            1,
            started.elapsed(),
            latencies,
            conflicts,
            retries,
        ))
    }

    async fn insert_concurrent(&self) -> Result<SecondaryEqualityInsertSample> {
        let started = Instant::now();
        let mut tasks = tokio::task::JoinSet::new();
        for worker in 0..CONCURRENT_WRITERS {
            let db = Arc::clone(&self.db);
            let plan = Arc::clone(&self.insert_plan);
            tasks.spawn(async move {
                let mut latencies = Vec::with_capacity(ENTITY_COUNT.div_ceil(CONCURRENT_WRITERS));
                let mut conflicts = 0_u64;
                let mut retries = 0_u64;
                for _ in (worker..ENTITY_COUNT).step_by(CONCURRENT_WRITERS) {
                    loop {
                        let operation_started = Instant::now();
                        match db.execute(&plan, context::ParamBindings::default()).await {
                            Ok(_) => {
                                latencies.push(duration_nanos(operation_started.elapsed()));
                                break;
                            }
                            Err(error) if error.is_transaction_conflict() => {
                                conflicts = conflicts.saturating_add(1);
                                retries = retries.saturating_add(1);
                            }
                            Err(error) => return Err(error),
                        }
                    }
                }
                Result::<_>::Ok((latencies, conflicts, retries))
            });
        }

        let mut latencies = Vec::with_capacity(ENTITY_COUNT);
        let mut conflicts = 0_u64;
        let mut retries = 0_u64;
        while let Some(result) = tasks.join_next().await {
            let (mut task_latencies, task_conflicts, task_retries) =
                result.map_err(|error| {
                    crate::HelixDbError::InvariantViolation(format!(
                        "hot-path benchmark writer task failed: {error}"
                    ))
                })??;
            latencies.append(&mut task_latencies);
            conflicts = conflicts.saturating_add(task_conflicts);
            retries = retries.saturating_add(task_retries);
        }
        assert_eq!(
            latencies.len(),
            ENTITY_COUNT,
            "every concurrent insertion must complete exactly once"
        );
        Ok(insert_sample(
            SecondaryEqualityInsertMode::Concurrent,
            CONCURRENT_WRITERS,
            started.elapsed(),
            latencies,
            conflicts,
            retries,
        ))
    }
}

fn insert_sample(
    mode: SecondaryEqualityInsertMode,
    writers: usize,
    elapsed: Duration,
    mut latencies: Vec<u64>,
    conflicts: u64,
    retries: u64,
) -> SecondaryEqualityInsertSample {
    latencies.sort_unstable();
    let median_latency_nanos = percentile(&latencies, 50);
    let p95_latency_nanos = percentile(&latencies, 95);
    SecondaryEqualityInsertSample {
        mode,
        indexes: INDEX_COUNT,
        entities: ENTITY_COUNT,
        writers,
        elapsed_nanos: elapsed.as_nanos(),
        throughput_per_second: f64::from(ENTITY_COUNT as u32) / elapsed.as_secs_f64(),
        median_latency_nanos,
        p95_latency_nanos,
        conflicts,
        retries,
        allocations: 0,
        allocated_bytes: 0,
    }
}

fn percentile(sorted: &[u64], percentile: usize) -> u64 {
    assert!(
        !sorted.is_empty(),
        "benchmark latency samples are non-empty"
    );
    let index = sorted.len().saturating_mul(percentile).div_ceil(100) - 1;
    sorted[index]
}

fn duration_nanos(duration: Duration) -> u64 {
    u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
}

fn property_name(ordinal: usize) -> String {
    format!("field_{ordinal:02}")
}

fn name(value: &str) -> ir::NonEmptyString {
    ir::NonEmptyString::new(value).expect("hot-path fixture identifiers are non-empty")
}

fn step(id: usize, dependencies: Vec<exec::ExecStepId>, op: exec::ExecOp) -> exec::ExecStep {
    exec::ExecStep {
        id: exec::ExecStepId::new(id).expect("hot-path step ids are positive"),
        dependencies,
        output: ir::BatchOutputPlan::Discard,
        condition: exec::ExecCondition::Always,
        op,
        schedule: exec::ExecSchedule::Pipeline,
        delivered: properties::DeliveredProperties::default(),
        cost: cost::CostVector::ZERO,
    }
}

fn equality_search_plan() -> exec::ExecutablePlan {
    let access_id = exec::ExecStepId::new(1).expect("hot-path access id is positive");
    exec::ExecutablePlan::new(
        ir::PlanKind::Read,
        ir::ReturnPlan::None,
        ir::AtLeast::<_, 1>::try_from_vec(vec![
            step(
                1,
                Vec::new(),
                exec::ExecOp::Access {
                    plan: Box::new(exec::ExecAccessPlan::Node(
                        exec::ExecNodeAccessPlan::EqualityIndex {
                            index: catalog::NodeEqualityIndexMeta::new(name(&format!(
                                "node_eq:{LABEL}:{}",
                                property_name(0)
                            ))),
                            key: catalog::ScopedPropertyKey::try_new(LABEL, property_name(0))
                                .expect("hot-path equality key is valid"),
                            value: ir::IndexValue::Literal(
                                ir::SecondaryIndexLiteral::new(PlannerPropertyValue::String(
                                    SHARED_VALUE.to_string(),
                                ))
                                .expect("hot-path equality value is indexable"),
                            ),
                        },
                    )),
                },
            ),
            step(
                2,
                vec![access_id],
                exec::ExecOp::Project {
                    projection: ir::ProjectionPlan::Id,
                },
            ),
        ])
        .expect("hot-path fixture plan is non-empty"),
        exec::ExecStepId::new(2).expect("hot-path root id is positive"),
        trace::PlanningTrace::default(),
        exec::PlannerMetrics::default(),
    )
    .expect("hot-path equality plan is valid")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentile_uses_nearest_rank() {
        let values = (1..=100).collect::<Vec<_>>();
        assert_eq!(percentile(&values, 50), 50);
        assert_eq!(percentile(&values, 95), 95);
    }
}
