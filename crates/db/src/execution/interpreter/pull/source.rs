//! Resumable graph sources. Stored bitmaps are prepared once; storage scans keep
//! their original iterator in the request's read view until demand ends.
use super::*;
use access::kv;
use bytes::Bytes;
use helix_planner::properties;

pub(super) enum Plan<'a> {
    Prepared,
    Access(&'a exec::ExecAccessPlan),
    Kv(&'a exec::KvReadPlan),
}

pub(super) struct Source<'a> {
    plan: Plan<'a>,
    state: State,
    remaining: Demand,
}

enum State {
    Unopened,
    Ids {
        ids: std::vec::IntoIter<u64>,
        keyspace: exec::ElementKeyspace,
        verified: bool,
    },
    Scan {
        iter: slatedb::DbIterator,
        keyspace: exec::ElementKeyspace,
    },
    Range {
        cursor: Box<crate::index_lifecycle::secondary::OrderedRangeCursor>,
        keyspace: exec::ElementKeyspace,
    },
    Rows(Items),
    Done,
}

impl<'a> Source<'a> {
    pub(super) fn ids(ids: Vec<u64>, keyspace: exec::ElementKeyspace, verified: bool) -> Self {
        Self {
            plan: Plan::Prepared,
            state: State::Ids {
                ids: ids.into_iter(),
                keyspace,
                verified,
            },
            remaining: Demand::All,
        }
    }

    pub(super) fn new(ctx: &ExecutionContext<'_>, mut plan: Plan<'a>) -> Result<Self> {
        let mut limit = None;
        if let Plan::Access(mut source) = plan {
            let mut bounds = Vec::new();
            while let exec::ExecAccessPlan::Limited(limited) = source {
                bounds.push(limited.limit());
                source = limited.source();
            }
            for bound in bounds.into_iter().rev() {
                let count = match bound {
                    exec::ExecAccessLimit::Zero => 0,
                    exec::ExecAccessLimit::Static(n) => n.get(),
                    exec::ExecAccessLimit::Dynamic(expr) => stream::eval_stream_bound(
                        &ir::StreamBoundPlan::Expr(expr.clone()),
                        &ctx.params,
                    )?,
                };
                limit = Some(limit.map_or(count, |n: usize| n.min(count)));
            }
            plan = Plan::Access(source);
        }
        if let Plan::Kv(
            exec::KvReadPlan::RangeScan { limit: n, .. }
            | exec::KvReadPlan::PrefixScan { limit: n, .. },
        ) = plan
        {
            limit = n.map(properties::PositiveUsize::get);
        }
        Ok(Self {
            plan,
            state: State::Unopened,
            remaining: limit.map_or(Demand::All, Demand::take),
        })
    }

    async fn open(&self, ctx: &mut ExecutionContext<'_>) -> Result<State> {
        use exec::{
            ElementKeyspace as K, ExecAccessPlan as A, ExecEdgeAccessPlan as E,
            ExecNodeAccessPlan as N,
        };
        let ids = |ids: Vec<u64>, keyspace, verified| State::Ids {
            ids: ids.into_iter(),
            keyspace,
            verified,
        };
        match self.plan {
            Plan::Prepared => unreachable!("prepared IDs already have source state"),
            Plan::Access(A::Node(N::Empty)) | Plan::Access(A::Edge(E::Empty)) => Ok(State::Done),
            Plan::Access(A::Node(N::FromParam { param })) => {
                Ok(ids(ctx.param_ids(param)?, K::NodeProperty, false))
            }
            Plan::Access(A::Edge(E::FromParam { param })) => {
                Ok(ids(ctx.param_ids(param)?, K::EdgeEndpoints, false))
            }
            Plan::Access(A::Node(N::FromVar { variable })) => Ok(ids(
                ctx.access_variable_nodes(variable)?,
                K::NodeProperty,
                false,
            )),
            Plan::Access(A::Edge(E::FromVar { variable })) => Ok(ids(
                ctx.access_variable_edges(variable)?,
                K::EdgeEndpoints,
                false,
            )),
            Plan::Access(A::Node(
                N::AllScan
                | N::AuthoritativeScan { .. }
                | N::SecondarySet {
                    set: exec::ExecNodeSecondarySetPlan::AuthoritativeScan(_),
                },
            )) => Self::scan(ctx, K::NodeProperty).await,
            Plan::Access(A::Edge(
                E::AllScan
                | E::AuthoritativeScan { .. }
                | E::SecondarySet {
                    set: exec::ExecEdgeSecondarySetPlan::AuthoritativeScan(_),
                },
            )) => Self::scan(ctx, K::EdgeEndpoints).await,
            Plan::Access(A::Node(N::LabelScan { label })) => Ok(ids(
                ctx.lookup_equality_index_set(
                    "$label",
                    &DbPropertyValue::String(label.to_string()),
                )
                .await?
                .into_iter()
                .collect(),
                K::NodeProperty,
                false,
            )),
            Plan::Access(A::Edge(E::LabelScan { label })) => Ok(ids(
                ctx.lookup_global_edge_label_index(label.as_ref())
                    .await?
                    .into_iter()
                    .collect(),
                K::EdgeEndpoints,
                false,
            )),
            Plan::Access(A::Node(N::Bitmap { bitmap })) => Ok(ids(
                ctx.node_bitmap(bitmap).await?.into_iter().collect(),
                K::NodeProperty,
                true,
            )),
            Plan::Access(A::Edge(E::Bitmap { bitmap })) => Ok(ids(
                ctx.edge_bitmap(bitmap).await?.into_iter().collect(),
                K::EdgeEndpoints,
                true,
            )),
            Plan::Access(A::Node(N::Unique {
                lookup,
                verification,
            })) => Ok(ids(
                ctx.verified_node_unique_owner(lookup, verification)
                    .await?
                    .into_iter()
                    .collect(),
                K::NodeProperty,
                true,
            )),
            Plan::Access(A::Node(N::DynamicEquality { index, key, param })) => {
                count::validate_node_equality_index(&index.index_id, key)?;
                let value = ctx.index_value(&ir::IndexValue::Param(param.clone()))?;
                Ok(ids(
                    ctx.lookup_managed_equality_union(
                        crate::index_lifecycle::IndexElementKind::Node,
                        key,
                        &[value],
                    )
                    .await?
                    .into_iter()
                    .collect(),
                    K::NodeProperty,
                    true,
                ))
            }
            Plan::Access(A::Edge(E::DynamicEquality { index, key, param })) => {
                count::validate_edge_equality_index(&index.index_id, key)?;
                let value = ctx.index_value(&ir::IndexValue::Param(param.clone()))?;
                Ok(ids(
                    ctx.lookup_managed_equality_union(
                        crate::index_lifecycle::IndexElementKind::Edge,
                        key,
                        &[value],
                    )
                    .await?
                    .into_iter()
                    .collect(),
                    K::EdgeEndpoints,
                    true,
                ))
            }
            Plan::Access(A::Node(N::DynamicMembership { index, key, values })) => {
                count::validate_node_equality_index(&index.index_id, key)?;
                Ok(ids(
                    ctx.dynamic_membership_ids(
                        crate::index_lifecycle::IndexElementKind::Node,
                        key,
                        values,
                    )
                    .await?
                    .into_iter()
                    .collect(),
                    K::NodeProperty,
                    true,
                ))
            }
            Plan::Access(A::Edge(E::DynamicMembership { index, key, values })) => {
                count::validate_edge_equality_index(&index.index_id, key)?;
                Ok(ids(
                    ctx.dynamic_membership_ids(
                        crate::index_lifecycle::IndexElementKind::Edge,
                        key,
                        values,
                    )
                    .await?
                    .into_iter()
                    .collect(),
                    K::EdgeEndpoints,
                    true,
                ))
            }
            Plan::Access(A::Node(N::SecondarySet {
                set: exec::ExecNodeSecondarySetPlan::Range(driver),
            })) => {
                self.open_range(
                    ctx,
                    K::NodeProperty,
                    &driver.key,
                    &driver.range,
                    driver.iteration,
                    Vec::new(),
                )
                .await
            }
            Plan::Access(A::Edge(E::SecondarySet {
                set: exec::ExecEdgeSecondarySetPlan::Range(driver),
            })) => {
                self.open_range(
                    ctx,
                    K::EdgeEndpoints,
                    &driver.key,
                    &driver.range,
                    driver.iteration,
                    Vec::new(),
                )
                .await
            }
            Plan::Access(A::Node(N::SecondarySet {
                set: exec::ExecNodeSecondarySetPlan::OrderedIntersect { driver, filters },
            })) => {
                let mut membership = Vec::new();
                for filter in filters {
                    membership.push(
                        ctx.node_secondary_set_ids(filter, None)
                            .await?
                            .into_iter()
                            .collect(),
                    );
                }
                self.open_range(
                    ctx,
                    K::NodeProperty,
                    &driver.key,
                    &driver.range,
                    driver.iteration,
                    membership,
                )
                .await
            }
            Plan::Access(A::Edge(E::SecondarySet {
                set: exec::ExecEdgeSecondarySetPlan::OrderedIntersect { driver, filters },
            })) => {
                let mut membership = Vec::new();
                for filter in filters {
                    membership.push(
                        ctx.edge_secondary_set_ids(filter, None)
                            .await?
                            .into_iter()
                            .collect(),
                    );
                }
                self.open_range(
                    ctx,
                    K::EdgeEndpoints,
                    &driver.key,
                    &driver.range,
                    driver.iteration,
                    membership,
                )
                .await
            }
            Plan::Access(A::Node(
                N::SecondarySet { .. } | N::VectorSearch { .. } | N::TextSearch { .. },
            ))
            | Plan::Access(A::Edge(
                E::SecondarySet { .. } | E::VectorSearch { .. } | E::TextSearch { .. },
            )) => {
                let Plan::Access(plan) = self.plan else {
                    unreachable!()
                };
                Ok(State::Rows(Items::new(
                    Box::pin(ctx.execute_prepared_access(
                        plan,
                        match self.remaining {
                            Demand::All => None,
                            Demand::Take(n) => properties::PositiveUsize::new(n.get()),
                            Demand::Done => unreachable!("zero demand does not open a source"),
                        },
                    ))
                    .await?,
                )))
            }
            Plan::Access(A::Node(N::RangeIndex {
                key,
                range,
                iteration,
                ..
            })) => {
                self.open_range(ctx, K::NodeProperty, key, range, *iteration, Vec::new())
                    .await
            }
            Plan::Access(A::Edge(E::RangeIndex {
                key,
                range,
                iteration,
                ..
            })) => {
                self.open_range(ctx, K::EdgeEndpoints, key, range, *iteration, Vec::new())
                    .await
            }
            Plan::Access(A::Limited(_)) => unreachable!("bounds resolved when source is activated"),
            Plan::Kv(exec::KvReadPlan::RangeScan {
                keyspace,
                start,
                end,
                ..
            }) => {
                let (start, end) = kv::element_range_bounds(*keyspace, start, end);
                Ok(State::Scan {
                    iter: ctx.open_raw_range(start, end).await?,
                    keyspace: *keyspace,
                })
            }
            Plan::Kv(exec::KvReadPlan::PrefixScan {
                keyspace, prefix, ..
            }) => {
                let mut bytes = kv::element_prefix(*keyspace);
                bytes.extend_from_slice(prefix.as_ref());
                Ok(State::Scan {
                    iter: ctx.open_raw_prefix(Bytes::from(bytes)).await?,
                    keyspace: *keyspace,
                })
            }
            // Keep the planner-selected point and multi-get primitives intact.
            Plan::Kv(plan @ (exec::KvReadPlan::Get { .. } | exec::KvReadPlan::MultiGet(_))) => Ok(
                State::Rows(Items::new(Box::pin(ctx.execute_kv_read(plan)).await?)),
            ),
        }
    }

    /// Known reverse bounds use the native bounded tie-group preparation.
    /// Unknown demand (for example a residual filter) keeps the resumable
    /// cursor. Both paths use the same request view and authoritative checks.
    async fn open_range(
        &self,
        ctx: &ExecutionContext<'_>,
        keyspace: exec::ElementKeyspace,
        key: &helix_planner::catalog::ScopedPropertyDirectionKey,
        range: &ir::IndexRange,
        iteration: ir::RangeScanIteration,
        membership: Vec<roaring::RoaringTreemap>,
    ) -> Result<State> {
        let element = match keyspace {
            exec::ElementKeyspace::NodeProperty => crate::index_lifecycle::IndexElementKind::Node,
            exec::ElementKeyspace::EdgeEndpoints => crate::index_lifecycle::IndexElementKind::Edge,
        };
        if let Demand::Take(limit) = self.remaining
            && iteration == ir::RangeScanIteration::Reverse
        {
            let ids = ctx
                .range_index_ids(
                    element,
                    key,
                    range,
                    iteration,
                    &membership,
                    properties::PositiveUsize::new(limit.get()),
                )
                .await?;
            return Ok(State::Ids {
                ids: ids.into_iter(),
                keyspace,
                verified: true,
            });
        }
        Ok(State::Range {
            cursor: Box::new(
                ctx.open_range_cursor(element, key, range, iteration)
                    .await?
                    .with_membership(membership),
            ),
            keyspace,
        })
    }

    async fn scan(ctx: &ExecutionContext<'_>, keyspace: exec::ElementKeyspace) -> Result<State> {
        Ok(State::Scan {
            iter: ctx
                .open_raw_range(
                    Bytes::from(kv::element_prefix(keyspace)),
                    Bytes::from(kv::element_prefix_end(keyspace)),
                )
                .await?,
            keyspace,
        })
    }

    pub(super) async fn next(
        &mut self,
        ctx: &mut ExecutionContext<'_>,
    ) -> Result<Option<ExecutionValue>> {
        loop {
            ctx.check_execution_deadline()?;
            if matches!(self.remaining, Demand::Done) {
                return Ok(None);
            }
            let row = match &mut self.state {
                State::Unopened => {
                    self.state = Box::pin(self.open(ctx)).await?;
                    continue;
                }
                State::Done => return Ok(None),
                State::Range { cursor, keyspace } => {
                    let Some(id) = ctx.next_range_cursor(cursor).await? else {
                        self.state = State::Done;
                        continue;
                    };
                    self.remaining.consume();
                    ExecutionRow::current(kv::element_ref(*keyspace, id))
                }
                State::Rows(items) => {
                    let item = items.next();
                    self.remaining.consume();
                    return Ok(item);
                }
                State::Ids {
                    ids,
                    keyspace,
                    verified,
                } => {
                    let Some(id) = ids.next() else {
                        self.state = State::Done;
                        continue;
                    };
                    #[cfg(test)]
                    ctx.pull_work
                        .source_visits
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    // An access-local limit applies at its original ID boundary.
                    self.remaining.consume();
                    if !*verified {
                        let key = exec::KvKey::from_id(*keyspace, id);
                        if ctx
                            .get_raw(&kv::physical_element_key(ctx.tenant_scope, &key).2)
                            .await?
                            .is_none()
                        {
                            continue;
                        }
                    }
                    ExecutionRow::current(kv::element_ref(*keyspace, id))
                }
                State::Scan { iter, keyspace } => {
                    let Some(entry) = iter.next().await? else {
                        self.state = State::Done;
                        continue;
                    };
                    #[cfg(test)]
                    ctx.pull_work
                        .source_visits
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some(key) = ctx.tenant_scope.strip_key(&entry.key) else {
                        return Err(HelixDbError::InvariantViolation(
                            "tenant-scoped scan returned key outside tenant prefix".into(),
                        ));
                    };
                    let Some(id) = kv::parse_element_id(*keyspace, key) else {
                        continue;
                    };
                    ExecutionRow::current(kv::element_ref(*keyspace, id))
                }
            };
            let accepted = match self.plan {
                Plan::Access(exec::ExecAccessPlan::Node(
                    exec::ExecNodeAccessPlan::AuthoritativeScan { predicate }
                    | exec::ExecNodeAccessPlan::SecondarySet {
                        set: exec::ExecNodeSecondarySetPlan::AuthoritativeScan(predicate),
                    },
                )) => match predicate {
                    exec::ExecNodeAuthoritativeScanPredicate::NullEquality { key } => {
                        ctx.scoped_null_matches(&row, key).await?
                    }
                    exec::ExecNodeAuthoritativeScanPredicate::Predicate(predicate) => {
                        ctx.eval_predicate(&row, predicate.predicate()).await?
                    }
                },
                Plan::Access(exec::ExecAccessPlan::Edge(
                    exec::ExecEdgeAccessPlan::AuthoritativeScan { predicate }
                    | exec::ExecEdgeAccessPlan::SecondarySet {
                        set: exec::ExecEdgeSecondarySetPlan::AuthoritativeScan(predicate),
                    },
                )) => match predicate {
                    exec::ExecEdgeAuthoritativeScanPredicate::NullEquality { key } => {
                        ctx.scoped_null_matches(&row, key).await?
                    }
                    exec::ExecEdgeAuthoritativeScanPredicate::Predicate(predicate) => {
                        ctx.eval_predicate(&row, predicate.predicate()).await?
                    }
                },
                Plan::Prepared | Plan::Access(_) | Plan::Kv(_) => true,
            };
            if !accepted {
                continue;
            }
            if matches!(self.state, State::Scan { .. }) {
                self.remaining.consume();
            }
            return Ok(Some(ExecutionValue::Stream(vec![row])));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    #[tokio::test]
    async fn bounded_reverse_ranges_preserve_retention_membership_and_stale_refill() {
        use crate::encoding::keys;
        use helix_ast::{index::RangeIndexDirection, value::PropertyValue};
        use helix_planner::catalog;
        for direction in [RangeIndexDirection::Asc, RangeIndexDirection::Desc] {
            let config = test_support::in_memory_config("bounded-reverse-range");
            let config = match direction {
                RangeIndexDirection::Asc => config
                    .with_range_index("User", "score")
                    .with_edge_range_index("LINK", "score"),
                RangeIndexDirection::Desc => config
                    .with_range_desc_index("User", "score")
                    .with_edge_range_desc_index("LINK", "score"),
            };
            let db = test_support::open_db_with_config(config).await;
            let mut nodes = Vec::new();
            let mut edges = Vec::new();
            for _ in 0..16 {
                nodes.push(
                    test_support::add_node_with_properties(
                        &db,
                        "User",
                        vec![("score", PropertyValue::I64(10))],
                    )
                    .await,
                );
            }
            for _ in 0..16 {
                edges.push(
                    test_support::add_edge_with_properties(
                        &db,
                        nodes[0],
                        nodes[1],
                        "LINK",
                        vec![("score", PropertyValue::I64(10))],
                    )
                    .await,
                );
            }
            for (keyspace, label, ids) in [
                (exec::ElementKeyspace::NodeProperty, "User", nodes),
                (exec::ElementKeyspace::EdgeEndpoints, "LINK", edges),
            ] {
                // Leave a stale index entry at the first ascending-ID tie.
                let key = keys::DataKey::Data {
                    scope: keys::scope::DataScope::LegacyUnscoped,
                    kind: match keyspace {
                        exec::ElementKeyspace::NodeProperty => {
                            keys::DataKeyKind::NodeProperty(keys::NodePropertyKey::new(ids[0]))
                        }
                        exec::ElementKeyspace::EdgeEndpoints => {
                            keys::DataKeyKind::EdgePropertyById(keys::EdgePropertyByIdKey::new(
                                ids[0],
                            ))
                        }
                    },
                }
                .to_bytes();
                db.inner_db().delete(key).await.unwrap();
                let key = catalog::ScopedPropertyDirectionKey::try_new(label, "score", direction)
                    .unwrap();
                for membership in [
                    Vec::new(),
                    vec![ids
                        .iter()
                        .step_by(2)
                        .copied()
                        .collect::<roaring::RoaringTreemap>()],
                    vec![roaring::RoaringTreemap::new()],
                ] {
                    for take in [1, 3, 20] {
                        let mut ctx = ExecutionContext::new(&db, context::ParamBindings::default());
                        let mut source = Source {
                            plan: Plan::Prepared,
                            state: State::Done,
                            remaining: Demand::take(take),
                        };
                        source.state = source
                            .open_range(
                                &ctx,
                                keyspace,
                                &key,
                                &ir::IndexRange::All,
                                ir::RangeScanIteration::Reverse,
                                membership.clone(),
                            )
                            .await
                            .unwrap();
                        let mut actual = Vec::new();
                        while let Some(value) = source.next(&mut ctx).await.unwrap() {
                            actual.extend(
                                ctx.stream_rows(value, "test")
                                    .unwrap()
                                    .into_iter()
                                    .map(|row| row.current.unwrap().id()),
                            );
                        }
                        let expected = ids
                            .iter()
                            .skip(1)
                            .copied()
                            .filter(|id| membership.iter().all(|set| set.contains(*id)))
                            .take(take)
                            .collect::<Vec<_>>();
                        assert_eq!(actual, expected);
                        assert!(ctx.range_reads.peak.load(Ordering::Relaxed) <= take);
                    }
                }
            }
            db.close().await.unwrap();
        }
    }
}
