//! Transaction-local coalescing for current-format graph topology rows.
//!
//! Bitmap and adjacency rows are staged as associative membership deltas. The
//! transaction conflict metadata names each logical member, so mutations to
//! disjoint members of one physical row can commit concurrently without a
//! read/modify/write cycle.

use std::collections::{BTreeMap, BTreeSet};

use bytes::Bytes;
use slatedb::DbTransaction;

use crate::encoding::indexes::label::{EdgeLabelKey, EdgeLabelNeighborKey};
use crate::encoding::indexes::{
    hash_property_name, hash_property_value, EdgeDirection, PropertyIndexKey,
};
use crate::encoding::v2::keys::scope::DataScope;
use crate::encoding::v2::keys::{AdjacencyKey, DataKey, DataKeyKind, EdgePairIndexKey};
use crate::encoding::v2::values::{adjacency as edges, indexes as secondary};
use crate::{HelixDbError, Result};

/// A final membership operation for one ID within the current epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MembershipMutation {
    Present,
    Absent,
}

/// Direction local to adjacency rows; `Both` is represented by two entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum AdjacencyDirection {
    Out,
    In,
}

#[derive(Debug, Default)]
struct TopologyMutationBatch {
    bitmaps: BTreeMap<Bytes, BTreeMap<u64, MembershipMutation>>,
    adjacency: BTreeMap<Bytes, BTreeMap<(AdjacencyDirection, u64), MembershipMutation>>,
}

/// Transaction-owned topology mutations with a terminal prepared state.
#[derive(Debug, Default)]
pub(in crate::execution::interpreter) struct TopologyMutationRuntime {
    state: TopologyMutationRuntimeState,
    staged_keys: BTreeSet<Bytes>,
}

#[derive(Debug, Default)]
enum TopologyMutationRuntimeState {
    #[default]
    Collecting,
    Pending(TopologyMutationBatch),
    Prepared,
}

impl TopologyMutationRuntime {
    /// Reads topology rows while preserving transaction-local staged overlays.
    pub(in crate::execution::interpreter) async fn observe(
        &self,
        transaction: &DbTransaction,
        keys: &[Bytes],
    ) -> Result<Vec<Option<Bytes>>> {
        let mut values = vec![None; keys.len()];
        let mut snapshot_positions = Vec::new();
        let mut snapshot_keys = Vec::new();
        for (position, key) in keys.iter().enumerate() {
            if self.staged_keys.contains(key) {
                values[position] = transaction.get(key).await?;
            } else {
                snapshot_positions.push(position);
                snapshot_keys.push(key.clone());
            }
        }
        if !snapshot_keys.is_empty() {
            for (position, value) in snapshot_positions
                .into_iter()
                .zip(transaction.multi_get(&snapshot_keys).await?)
            {
                values[position] = value;
            }
        }
        Ok(values)
    }

    /// Adds one node-label membership through the canonical typed key.
    pub(in crate::execution::interpreter) fn add_node_label(
        &mut self,
        scope: DataScope,
        label: &str,
        node_id: u64,
    ) -> Result<()> {
        self.record_bitmap(
            node_label_key(scope, label),
            node_id,
            MembershipMutation::Present,
        )
    }

    /// Removes one node-label membership through the canonical typed key.
    pub(in crate::execution::interpreter) fn remove_node_label(
        &mut self,
        scope: DataScope,
        label: &str,
        node_id: u64,
    ) -> Result<()> {
        self.record_bitmap(
            node_label_key(scope, label),
            node_id,
            MembershipMutation::Absent,
        )
    }

    /// Adds one exact multigraph pair membership.
    pub(in crate::execution::interpreter) fn add_edge_pair(
        &mut self,
        scope: DataScope,
        from: u64,
        to: u64,
        edge_id: u64,
    ) -> Result<()> {
        self.record_bitmap(
            edge_pair_key(scope, from, to),
            edge_id,
            MembershipMutation::Present,
        )
    }

    /// Removes one exact multigraph pair membership.
    pub(in crate::execution::interpreter) fn remove_edge_pair(
        &mut self,
        scope: DataScope,
        from: u64,
        to: u64,
        edge_id: u64,
    ) -> Result<()> {
        self.record_bitmap(
            edge_pair_key(scope, from, to),
            edge_id,
            MembershipMutation::Absent,
        )
    }

    /// Adds outgoing, incoming, and global edge-label memberships.
    pub(in crate::execution::interpreter) fn add_edge_label(
        &mut self,
        scope: DataScope,
        from: u64,
        to: u64,
        label: &str,
        edge_id: u64,
    ) -> Result<()> {
        self.record_bitmap(
            edge_label_neighbor_key(scope, EdgeDirection::Out, from, label),
            to,
            MembershipMutation::Present,
        )?;
        self.record_bitmap(
            edge_label_neighbor_key(scope, EdgeDirection::In, to, label),
            from,
            MembershipMutation::Present,
        )?;
        self.record_bitmap(
            global_edge_label_key(scope, label),
            edge_id,
            MembershipMutation::Present,
        )
    }

    /// Removes the global edge-label membership for one physical edge.
    pub(in crate::execution::interpreter) fn remove_global_edge_label(
        &mut self,
        scope: DataScope,
        label: &str,
        edge_id: u64,
    ) -> Result<()> {
        self.record_bitmap(
            global_edge_label_key(scope, label),
            edge_id,
            MembershipMutation::Absent,
        )
    }

    /// Removes pair-level outgoing and incoming edge-label memberships.
    pub(in crate::execution::interpreter) fn remove_edge_label_neighbors(
        &mut self,
        scope: DataScope,
        from: u64,
        to: u64,
        label: &str,
    ) -> Result<()> {
        self.record_bitmap(
            edge_label_neighbor_key(scope, EdgeDirection::Out, from, label),
            to,
            MembershipMutation::Absent,
        )?;
        self.record_bitmap(
            edge_label_neighbor_key(scope, EdgeDirection::In, to, label),
            from,
            MembershipMutation::Absent,
        )
    }

    /// Adds one directed adjacency membership.
    pub(in crate::execution::interpreter) fn add_adjacency(
        &mut self,
        scope: DataScope,
        node: u64,
        neighbor: u64,
        direction: helix_planner::ir::ExpandDirection,
    ) -> Result<()> {
        self.record_adjacency(
            scope,
            node,
            neighbor,
            direction,
            MembershipMutation::Present,
        )
    }

    /// Removes one directed adjacency membership.
    pub(in crate::execution::interpreter) fn remove_adjacency(
        &mut self,
        scope: DataScope,
        node: u64,
        neighbor: u64,
        direction: helix_planner::ir::ExpandDirection,
    ) -> Result<()> {
        self.record_adjacency(scope, node, neighbor, direction, MembershipMutation::Absent)
    }

    fn record_bitmap(&mut self, key: Bytes, id: u64, mutation: MembershipMutation) -> Result<()> {
        let batch = self.collecting_batch()?;
        batch.bitmaps.entry(key).or_default().insert(id, mutation);
        Ok(())
    }

    fn record_adjacency(
        &mut self,
        scope: DataScope,
        node: u64,
        neighbor: u64,
        direction: helix_planner::ir::ExpandDirection,
        mutation: MembershipMutation,
    ) -> Result<()> {
        let key = DataKey::Data {
            scope,
            kind: DataKeyKind::Adjacency(AdjacencyKey::new(node)),
        }
        .to_bytes();
        let row = self.collecting_batch()?.adjacency.entry(key).or_default();
        match direction {
            helix_planner::ir::ExpandDirection::Out => {
                row.insert((AdjacencyDirection::Out, neighbor), mutation);
            }
            helix_planner::ir::ExpandDirection::In => {
                row.insert((AdjacencyDirection::In, neighbor), mutation);
            }
            helix_planner::ir::ExpandDirection::Both => {
                row.insert((AdjacencyDirection::Out, neighbor), mutation);
                row.insert((AdjacencyDirection::In, neighbor), mutation);
            }
        }
        Ok(())
    }

    fn collecting_batch(&mut self) -> Result<&mut TopologyMutationBatch> {
        if matches!(self.state, TopologyMutationRuntimeState::Collecting) {
            self.state = TopologyMutationRuntimeState::Pending(TopologyMutationBatch::default());
        }
        match &mut self.state {
            TopologyMutationRuntimeState::Pending(batch) => Ok(batch),
            TopologyMutationRuntimeState::Collecting => {
                unreachable!("collecting state transitions to a pending batch")
            }
            TopologyMutationRuntimeState::Prepared => Err(HelixDbError::InvariantViolation(
                "prepared topology mutation runtime cannot collect another mutation".to_string(),
            )),
        }
    }

    /// Flushes one coalesced epoch and returns to the collecting state.
    pub(in crate::execution::interpreter) async fn flush(
        &mut self,
        transaction: &DbTransaction,
    ) -> Result<()> {
        let state = std::mem::take(&mut self.state);
        let batch = match state {
            TopologyMutationRuntimeState::Collecting => return Ok(()),
            TopologyMutationRuntimeState::Pending(batch) => batch,
            TopologyMutationRuntimeState::Prepared => {
                self.state = TopologyMutationRuntimeState::Prepared;
                return Err(HelixDbError::InvariantViolation(
                    "prepared topology mutation runtime cannot flush another epoch".to_string(),
                ));
            }
        };

        for (key, mutations) in batch.bitmaps {
            let mut delta = secondary::BitmapMembershipDelta::default();
            let mut discriminators = Vec::with_capacity(mutations.len());
            for (id, mutation) in &mutations {
                discriminators.push(Bytes::copy_from_slice(&id.to_be_bytes()));
                match mutation {
                    MembershipMutation::Present => {
                        delta.add(*id);
                    }
                    MembershipMutation::Absent => {
                        delta.remove(*id);
                    }
                }
            }
            transaction.merge_disjoint(&key, discriminators, delta.encode())?;
            self.staged_keys.insert(key);
        }

        for (key, mutations) in batch.adjacency {
            let mut delta = edges::AdjacencyMembershipDelta::default();
            let mut discriminators = Vec::with_capacity(mutations.len());
            for ((direction, neighbor), mutation) in &mutations {
                let mut discriminator =
                    Vec::with_capacity(core::mem::size_of::<u8>() + core::mem::size_of::<u64>());
                discriminator.push(match direction {
                    AdjacencyDirection::Out => 0,
                    AdjacencyDirection::In => 1,
                });
                discriminator.extend_from_slice(&neighbor.to_be_bytes());
                discriminators.push(Bytes::from(discriminator));
                match (direction, mutation) {
                    (AdjacencyDirection::Out, MembershipMutation::Present) => {
                        delta.add_out(*neighbor);
                    }
                    (AdjacencyDirection::In, MembershipMutation::Present) => {
                        delta.add_in(*neighbor);
                    }
                    (AdjacencyDirection::Out, MembershipMutation::Absent) => {
                        delta.remove_out(*neighbor);
                    }
                    (AdjacencyDirection::In, MembershipMutation::Absent) => {
                        delta.remove_in(*neighbor);
                    }
                }
            }
            transaction.merge_disjoint(&key, discriminators, delta.encode())?;
            self.staged_keys.insert(key);
        }
        Ok(())
    }

    /// Flushes the final epoch and seals the runtime for commit.
    pub(in crate::execution::interpreter) async fn prepare(
        &mut self,
        transaction: &DbTransaction,
    ) -> Result<()> {
        self.flush(transaction).await?;
        self.state = TopologyMutationRuntimeState::Prepared;
        Ok(())
    }

    /// Consumes only a runtime that completed final preparation.
    pub(in crate::execution::interpreter) fn consume_prepared(self) -> Result<()> {
        match self.state {
            TopologyMutationRuntimeState::Prepared => Ok(()),
            TopologyMutationRuntimeState::Collecting | TopologyMutationRuntimeState::Pending(_) => {
                Err(HelixDbError::InvariantViolation(
                    "topology mutation runtime reached commit before prepare".to_string(),
                ))
            }
        }
    }
}

fn node_label_key(scope: DataScope, label: &str) -> Bytes {
    DataKey::Data {
        scope,
        kind: DataKeyKind::PropertyIndex(PropertyIndexKey::Equality(
            crate::encoding::indexes::equality::EqualityIndexKey::new(
                hash_property_name("$label"),
                hash_property_value(label),
            ),
        )),
    }
    .to_bytes()
}

fn edge_pair_key(scope: DataScope, from: u64, to: u64) -> Bytes {
    DataKey::Data {
        scope,
        kind: DataKeyKind::EdgePairIndex(EdgePairIndexKey::new(from, to)),
    }
    .to_bytes()
}

fn edge_label_neighbor_key(
    scope: DataScope,
    direction: EdgeDirection,
    node: u64,
    label: &str,
) -> Bytes {
    DataKey::Data {
        scope,
        kind: DataKeyKind::PropertyIndex(PropertyIndexKey::EdgeLabelNeighbor(
            EdgeLabelNeighborKey::new(direction, node, hash_property_value(label)),
        )),
    }
    .to_bytes()
}

fn global_edge_label_key(scope: DataScope, label: &str) -> Bytes {
    DataKey::Data {
        scope,
        kind: DataKeyKind::PropertyIndex(PropertyIndexKey::EdgeLabel(EdgeLabelKey::new(
            hash_property_value(label),
        ))),
    }
    .to_bytes()
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use roaring::RoaringTreemap;
    use slatedb::object_store::memory::InMemory;
    use slatedb::{Db, IsolationLevel};

    use super::*;

    async fn database(name: &str) -> Db {
        Db::builder(
            format!("topology-mutation-runtime/{name}"),
            Arc::new(InMemory::new()),
        )
        .with_merge_operator(Arc::new(crate::merge_operator::HelixMergeOperator::new()))
        .build()
        .await
        .expect("topology test database opens")
    }

    async fn transaction(name: &str) -> DbTransaction {
        database(name)
            .await
            .begin(IsolationLevel::SerializableSnapshot)
            .await
            .expect("topology test transaction opens")
    }

    #[derive(Debug, Clone, Copy)]
    enum RaceRow {
        NodeLabel(&'static str),
        OutAdjacency(u64),
    }

    impl RaceRow {
        fn stage(
            self,
            runtime: &mut TopologyMutationRuntime,
            scope: DataScope,
            id: u64,
            mutation: MembershipMutation,
        ) {
            match (self, mutation) {
                (Self::NodeLabel(label), MembershipMutation::Present) => {
                    runtime.add_node_label(scope, label, id).unwrap();
                }
                (Self::NodeLabel(label), MembershipMutation::Absent) => {
                    runtime.remove_node_label(scope, label, id).unwrap();
                }
                (Self::OutAdjacency(node), MembershipMutation::Present) => {
                    runtime
                        .add_adjacency(scope, node, id, helix_planner::ir::ExpandDirection::Out)
                        .unwrap();
                }
                (Self::OutAdjacency(node), MembershipMutation::Absent) => {
                    runtime
                        .remove_adjacency(scope, node, id, helix_planner::ir::ExpandDirection::Out)
                        .unwrap();
                }
            }
        }

        async fn ids(self, db: &Db, scope: DataScope) -> Vec<u64> {
            match self {
                Self::NodeLabel(label) => db
                    .get(node_label_key(scope, label))
                    .await
                    .unwrap()
                    .map(|value| {
                        secondary::SecondaryEqualityValue::decode(&value)
                            .unwrap()
                            .into_ids()
                            .iter()
                            .collect()
                    })
                    .unwrap_or_default(),
                Self::OutAdjacency(node) => db
                    .get(
                        DataKey::Data {
                            scope,
                            kind: DataKeyKind::Adjacency(AdjacencyKey::new(node)),
                        }
                        .to_bytes(),
                    )
                    .await
                    .unwrap()
                    .map(|value| edges::decode_edges(&value).unwrap().iter_out().collect())
                    .unwrap_or_default(),
            }
        }
    }

    fn stage_linked_event(
        runtime: &mut TopologyMutationRuntime,
        scope: DataScope,
        parent_id: u64,
        event_id: u64,
        edge_id: u64,
        node_label: &str,
        edge_label: &str,
        mutation: MembershipMutation,
    ) {
        match mutation {
            MembershipMutation::Present => {
                runtime.add_node_label(scope, node_label, event_id).unwrap();
                runtime
                    .add_edge_pair(scope, parent_id, event_id, edge_id)
                    .unwrap();
                runtime
                    .add_edge_label(scope, parent_id, event_id, edge_label, edge_id)
                    .unwrap();
                runtime
                    .add_adjacency(
                        scope,
                        parent_id,
                        event_id,
                        helix_planner::ir::ExpandDirection::Out,
                    )
                    .unwrap();
                runtime
                    .add_adjacency(
                        scope,
                        event_id,
                        parent_id,
                        helix_planner::ir::ExpandDirection::In,
                    )
                    .unwrap();
            }
            MembershipMutation::Absent => {
                runtime
                    .remove_node_label(scope, node_label, event_id)
                    .unwrap();
                runtime
                    .remove_edge_pair(scope, parent_id, event_id, edge_id)
                    .unwrap();
                runtime
                    .remove_global_edge_label(scope, edge_label, edge_id)
                    .unwrap();
                runtime
                    .remove_edge_label_neighbors(scope, parent_id, event_id, edge_label)
                    .unwrap();
                runtime
                    .remove_adjacency(
                        scope,
                        parent_id,
                        event_id,
                        helix_planner::ir::ExpandDirection::Out,
                    )
                    .unwrap();
                runtime
                    .remove_adjacency(
                        scope,
                        event_id,
                        parent_id,
                        helix_planner::ir::ExpandDirection::In,
                    )
                    .unwrap();
            }
        }
    }

    #[tokio::test]
    async fn concurrent_add_only_rows_commit_and_preserve_every_topology_membership() {
        let db = database("concurrent-add-only").await;
        let scope = DataScope::LegacyUnscoped;
        let left = db
            .begin(IsolationLevel::SerializableSnapshot)
            .await
            .unwrap();
        let right = db
            .begin(IsolationLevel::SerializableSnapshot)
            .await
            .unwrap();
        let mut left_runtime = TopologyMutationRuntime::default();
        let mut right_runtime = TopologyMutationRuntime::default();

        left_runtime.add_node_label(scope, "User", 1).unwrap();
        right_runtime.add_node_label(scope, "User", 2).unwrap();
        left_runtime.add_edge_pair(scope, 10, 20, 100).unwrap();
        right_runtime.add_edge_pair(scope, 10, 20, 101).unwrap();
        left_runtime
            .add_edge_label(scope, 1, 9, "FOLLOWS", 100)
            .unwrap();
        right_runtime
            .add_edge_label(scope, 1, 10, "FOLLOWS", 101)
            .unwrap();
        left_runtime
            .add_edge_label(scope, 11, 2, "FOLLOWS", 102)
            .unwrap();
        right_runtime
            .add_edge_label(scope, 12, 2, "FOLLOWS", 103)
            .unwrap();
        left_runtime
            .add_adjacency(scope, 50, 60, helix_planner::ir::ExpandDirection::Both)
            .unwrap();
        right_runtime
            .add_adjacency(scope, 50, 61, helix_planner::ir::ExpandDirection::Both)
            .unwrap();

        left_runtime.prepare(&left).await.unwrap();
        right_runtime.prepare(&right).await.unwrap();
        left_runtime.consume_prepared().unwrap();
        right_runtime.consume_prepared().unwrap();
        left.commit().await.unwrap();
        right.commit().await.unwrap();

        let bitmap_ids = |key| async {
            secondary::SecondaryEqualityValue::decode(
                &db.get(key).await.unwrap().expect("topology bitmap exists"),
            )
            .unwrap()
            .into_ids()
            .iter()
            .collect::<Vec<_>>()
        };
        assert_eq!(bitmap_ids(node_label_key(scope, "User")).await, vec![1, 2]);
        assert_eq!(
            bitmap_ids(edge_pair_key(scope, 10, 20)).await,
            vec![100, 101]
        );
        assert_eq!(
            bitmap_ids(edge_label_neighbor_key(
                scope,
                EdgeDirection::Out,
                1,
                "FOLLOWS",
            ))
            .await,
            vec![9, 10]
        );
        assert_eq!(
            bitmap_ids(edge_label_neighbor_key(
                scope,
                EdgeDirection::In,
                2,
                "FOLLOWS",
            ))
            .await,
            vec![11, 12]
        );
        assert_eq!(
            bitmap_ids(global_edge_label_key(scope, "FOLLOWS")).await,
            vec![100, 101, 102, 103]
        );
        let adjacency = edges::decode_edges(
            &db.get(
                DataKey::Data {
                    scope,
                    kind: DataKeyKind::Adjacency(AdjacencyKey::new(50)),
                }
                .to_bytes(),
            )
            .await
            .unwrap()
            .expect("adjacency row exists"),
        )
        .unwrap();
        assert_eq!(adjacency.iter_out().collect::<Vec<_>>(), vec![60, 61]);
        assert_eq!(adjacency.iter_in().collect::<Vec<_>>(), vec![60, 61]);
        db.close().await.unwrap();
    }

    #[tokio::test]
    async fn topology_insert_remove_races_on_disjoint_members_commit_in_both_orders() {
        let db = database("insert-remove-races").await;
        let scope = DataScope::LegacyUnscoped;
        let cases = [
            (RaceRow::NodeLabel("insert-first"), true),
            (RaceRow::NodeLabel("remove-first"), false),
            (RaceRow::OutAdjacency(100), true),
            (RaceRow::OutAdjacency(101), false),
        ];

        for (row, insert_commits_first) in cases {
            let seed = db
                .begin(IsolationLevel::SerializableSnapshot)
                .await
                .unwrap();
            let mut seed_runtime = TopologyMutationRuntime::default();
            row.stage(&mut seed_runtime, scope, 1, MembershipMutation::Present);
            seed_runtime.prepare(&seed).await.unwrap();
            seed_runtime.consume_prepared().unwrap();
            seed.commit().await.unwrap();

            let insert = db
                .begin(IsolationLevel::SerializableSnapshot)
                .await
                .unwrap();
            let remove = db
                .begin(IsolationLevel::SerializableSnapshot)
                .await
                .unwrap();
            let mut insert_runtime = TopologyMutationRuntime::default();
            row.stage(&mut insert_runtime, scope, 2, MembershipMutation::Present);
            insert_runtime.prepare(&insert).await.unwrap();
            insert_runtime.consume_prepared().unwrap();
            let mut remove_runtime = TopologyMutationRuntime::default();
            row.stage(&mut remove_runtime, scope, 1, MembershipMutation::Absent);
            remove_runtime.prepare(&remove).await.unwrap();
            remove_runtime.consume_prepared().unwrap();

            if insert_commits_first {
                insert.commit().await.unwrap();
                remove.commit().await.unwrap();
            } else {
                remove.commit().await.unwrap();
                insert.commit().await.unwrap();
            }
            assert_eq!(row.ids(&db, scope).await, vec![2]);
        }

        db.close().await.unwrap();
    }

    #[tokio::test]
    async fn topology_insert_remove_races_on_same_member_conflict_in_both_orders() {
        let db = database("same-member-insert-remove-races").await;
        let scope = DataScope::LegacyUnscoped;
        let cases = [
            (RaceRow::NodeLabel("insert-first"), true),
            (RaceRow::NodeLabel("remove-first"), false),
            (RaceRow::OutAdjacency(200), true),
            (RaceRow::OutAdjacency(201), false),
        ];

        for (row, insert_commits_first) in cases {
            let seed = db
                .begin(IsolationLevel::SerializableSnapshot)
                .await
                .unwrap();
            let mut seed_runtime = TopologyMutationRuntime::default();
            row.stage(&mut seed_runtime, scope, 1, MembershipMutation::Present);
            seed_runtime.prepare(&seed).await.unwrap();
            seed_runtime.consume_prepared().unwrap();
            seed.commit().await.unwrap();

            let insert = db
                .begin(IsolationLevel::SerializableSnapshot)
                .await
                .unwrap();
            let remove = db
                .begin(IsolationLevel::SerializableSnapshot)
                .await
                .unwrap();
            let mut insert_runtime = TopologyMutationRuntime::default();
            row.stage(&mut insert_runtime, scope, 1, MembershipMutation::Present);
            insert_runtime.prepare(&insert).await.unwrap();
            insert_runtime.consume_prepared().unwrap();
            let mut remove_runtime = TopologyMutationRuntime::default();
            row.stage(&mut remove_runtime, scope, 1, MembershipMutation::Absent);
            remove_runtime.prepare(&remove).await.unwrap();
            remove_runtime.consume_prepared().unwrap();

            if insert_commits_first {
                insert.commit().await.unwrap();
                let error = remove.commit().await.expect_err("remove must conflict");
                assert_eq!(error.kind(), slatedb::ErrorKind::Transaction);
                assert_eq!(row.ids(&db, scope).await, vec![1]);
            } else {
                remove.commit().await.unwrap();
                let error = insert.commit().await.expect_err("insert must conflict");
                assert_eq!(error.kind(), slatedb::ErrorKind::Transaction);
                assert_eq!(row.ids(&db, scope).await, Vec::<u64>::new());
            }
        }

        db.close().await.unwrap();
    }

    #[tokio::test]
    async fn linked_event_insert_and_expiry_preserve_every_shared_topology_row() {
        let db = database("linked-event-insert-expiry-races").await;
        let scope = DataScope::LegacyUnscoped;

        for (case, insert_commits_first) in [(0_u64, false), (1, true)] {
            let parent_id = 1_000 + case * 100;
            let expired_event_id = parent_id + 1;
            let inserted_event_id = parent_id + 2;
            let expired_edge_id = parent_id + 10;
            let inserted_edge_id = parent_id + 11;
            let node_label = format!("Event-{case}");
            let edge_label = format!("HAS_EVENT-{case}");

            let seed = db
                .begin(IsolationLevel::SerializableSnapshot)
                .await
                .unwrap();
            let mut seed_runtime = TopologyMutationRuntime::default();
            stage_linked_event(
                &mut seed_runtime,
                scope,
                parent_id,
                expired_event_id,
                expired_edge_id,
                &node_label,
                &edge_label,
                MembershipMutation::Present,
            );
            seed_runtime.prepare(&seed).await.unwrap();
            seed_runtime.consume_prepared().unwrap();
            seed.commit().await.unwrap();

            let insert = db
                .begin(IsolationLevel::SerializableSnapshot)
                .await
                .unwrap();
            let remove = db
                .begin(IsolationLevel::SerializableSnapshot)
                .await
                .unwrap();
            let mut insert_runtime = TopologyMutationRuntime::default();
            stage_linked_event(
                &mut insert_runtime,
                scope,
                parent_id,
                inserted_event_id,
                inserted_edge_id,
                &node_label,
                &edge_label,
                MembershipMutation::Present,
            );
            insert_runtime.prepare(&insert).await.unwrap();
            insert_runtime.consume_prepared().unwrap();
            let mut remove_runtime = TopologyMutationRuntime::default();
            stage_linked_event(
                &mut remove_runtime,
                scope,
                parent_id,
                expired_event_id,
                expired_edge_id,
                &node_label,
                &edge_label,
                MembershipMutation::Absent,
            );
            remove_runtime.prepare(&remove).await.unwrap();
            remove_runtime.consume_prepared().unwrap();

            if insert_commits_first {
                insert.commit().await.unwrap();
                remove.commit().await.unwrap();
            } else {
                remove.commit().await.unwrap();
                insert.commit().await.unwrap();
            }

            let bitmap_ids = |key| async {
                db.get(key)
                    .await
                    .unwrap()
                    .map(|value| {
                        secondary::SecondaryEqualityValue::decode(&value)
                            .unwrap()
                            .into_ids()
                            .iter()
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default()
            };
            assert_eq!(
                bitmap_ids(node_label_key(scope, &node_label)).await,
                vec![inserted_event_id]
            );
            assert_eq!(
                bitmap_ids(edge_label_neighbor_key(
                    scope,
                    EdgeDirection::Out,
                    parent_id,
                    &edge_label,
                ))
                .await,
                vec![inserted_event_id]
            );
            assert_eq!(
                bitmap_ids(global_edge_label_key(scope, &edge_label)).await,
                vec![inserted_edge_id]
            );
            assert_eq!(
                bitmap_ids(edge_pair_key(scope, parent_id, expired_event_id)).await,
                Vec::<u64>::new()
            );
            assert_eq!(
                bitmap_ids(edge_pair_key(scope, parent_id, inserted_event_id)).await,
                vec![inserted_edge_id]
            );
            let parent_adjacency = edges::decode_edges(
                &db.get(
                    DataKey::Data {
                        scope,
                        kind: DataKeyKind::Adjacency(AdjacencyKey::new(parent_id)),
                    }
                    .to_bytes(),
                )
                .await
                .unwrap()
                .expect("parent adjacency remains"),
            )
            .unwrap();
            assert_eq!(
                parent_adjacency.iter_out().collect::<Vec<_>>(),
                vec![inserted_event_id]
            );
            let expired_adjacency = db
                .get(
                    DataKey::Data {
                        scope,
                        kind: DataKeyKind::Adjacency(AdjacencyKey::new(expired_event_id)),
                    }
                    .to_bytes(),
                )
                .await
                .unwrap()
                .map(|value| edges::decode_edges(&value).unwrap())
                .unwrap_or_default();
            assert!(expired_adjacency.is_empty());
        }

        db.close().await.unwrap();
    }

    #[tokio::test]
    async fn add_only_rows_merge_once_and_later_removals_observe_the_staged_overlay() {
        let transaction = transaction("add-remove-overlay").await;
        let scope = DataScope::LegacyUnscoped;
        let mut runtime = TopologyMutationRuntime::default();

        for node_id in [1, 2] {
            runtime.add_node_label(scope, "User", node_id).unwrap();
        }
        for edge_id in [10, 11] {
            runtime.add_edge_pair(scope, 1, 2, edge_id).unwrap();
        }
        runtime
            .add_adjacency(scope, 1, 2, helix_planner::ir::ExpandDirection::Out)
            .unwrap();
        runtime
            .add_adjacency(scope, 1, 3, helix_planner::ir::ExpandDirection::Out)
            .unwrap();
        runtime.add_edge_label(scope, 1, 2, "FOLLOWS", 10).unwrap();
        runtime.flush(&transaction).await.unwrap();

        runtime.remove_node_label(scope, "User", 1).unwrap();
        runtime.remove_edge_pair(scope, 1, 2, 10).unwrap();
        runtime
            .remove_adjacency(scope, 1, 2, helix_planner::ir::ExpandDirection::Out)
            .unwrap();
        runtime
            .remove_global_edge_label(scope, "FOLLOWS", 10)
            .unwrap();
        runtime.flush(&transaction).await.unwrap();

        let labels = secondary::SecondaryEqualityValue::decode(
            &transaction
                .get(node_label_key(scope, "User"))
                .await
                .unwrap()
                .expect("remaining node label row exists"),
        )
        .unwrap()
        .into_ids();
        assert_eq!(labels.iter().collect::<Vec<_>>(), vec![2]);
        let pairs = secondary::SecondaryEqualityValue::decode(
            &transaction
                .get(edge_pair_key(scope, 1, 2))
                .await
                .unwrap()
                .expect("remaining pair row exists"),
        )
        .unwrap()
        .into_ids();
        assert_eq!(pairs.iter().collect::<Vec<_>>(), vec![11]);
        let adjacency = edges::decode_edges(
            &transaction
                .get(
                    DataKey::Data {
                        scope,
                        kind: DataKeyKind::Adjacency(AdjacencyKey::new(1)),
                    }
                    .to_bytes(),
                )
                .await
                .unwrap()
                .expect("remaining adjacency row exists"),
        )
        .unwrap();
        assert_eq!(adjacency.iter_out().collect::<Vec<_>>(), vec![3]);

        runtime.prepare(&transaction).await.unwrap();
        runtime.consume_prepared().unwrap();
    }

    #[tokio::test]
    async fn prepared_runtime_rejects_further_collection() {
        let transaction = transaction("prepared-rejects-collection").await;
        let mut runtime = TopologyMutationRuntime::default();
        runtime.prepare(&transaction).await.unwrap();

        assert!(matches!(
            runtime.add_node_label(DataScope::LegacyUnscoped, "User", 1),
            Err(HelixDbError::InvariantViolation(_))
        ));
    }

    #[derive(Debug, Clone, Copy)]
    enum SweepBenchmarkStrategy {
        ExclusiveReadModifyWrite,
        DisjointDelta,
    }

    #[derive(Debug)]
    struct SweepBenchmarkResult {
        elapsed: Duration,
        writer_conflicts: usize,
        sweeper_conflicts: usize,
        writer_commits: usize,
        sweeper_commits: usize,
    }

    async fn stage_benchmark_memberships(
        db: &Db,
        strategy: SweepBenchmarkStrategy,
        ids: std::ops::Range<u64>,
        present: bool,
    ) -> DbTransaction {
        const PARENT_ID: u64 = u64::MAX - 1;
        const LABEL: &str = "KubernetesEvent";

        let transaction = db
            .begin(IsolationLevel::SerializableSnapshot)
            .await
            .expect("benchmark transaction begins");
        match strategy {
            SweepBenchmarkStrategy::ExclusiveReadModifyWrite if present => {
                let additions = ids.clone().collect::<RoaringTreemap>();
                transaction
                    .merge_commutative(
                        node_label_key(DataScope::LegacyUnscoped, LABEL),
                        secondary::SecondaryEqualityValue::encode_ids(&additions),
                    )
                    .expect("exclusive benchmark label additions stage");
                let mut adjacency = edges::Edges::new();
                for id in ids {
                    adjacency.add_out(id);
                }
                transaction
                    .merge_commutative(
                        DataKey::Data {
                            scope: DataScope::LegacyUnscoped,
                            kind: DataKeyKind::Adjacency(AdjacencyKey::new(PARENT_ID)),
                        }
                        .to_bytes(),
                        edges::encode_edges(&adjacency),
                    )
                    .expect("exclusive benchmark adjacency additions stage");
            }
            SweepBenchmarkStrategy::ExclusiveReadModifyWrite => {
                let label_key = node_label_key(DataScope::LegacyUnscoped, LABEL);
                let adjacency_key = DataKey::Data {
                    scope: DataScope::LegacyUnscoped,
                    kind: DataKeyKind::Adjacency(AdjacencyKey::new(PARENT_ID)),
                }
                .to_bytes();
                let values = transaction
                    .multi_get(&[label_key.clone(), adjacency_key.clone()])
                    .await
                    .expect("exclusive benchmark rows read");
                let mut labels = values[0]
                    .as_deref()
                    .map(secondary::SecondaryEqualityValue::decode)
                    .transpose()
                    .expect("exclusive benchmark labels decode")
                    .map(secondary::SecondaryEqualityValue::into_ids)
                    .unwrap_or_default();
                let mut adjacency = values[1]
                    .as_deref()
                    .map(edges::decode_edges)
                    .transpose()
                    .expect("exclusive benchmark adjacency decodes")
                    .unwrap_or_default();
                for id in ids {
                    labels.remove(id);
                    adjacency.remove_out(id);
                }
                if labels.is_empty() {
                    transaction
                        .delete(label_key)
                        .expect("exclusive benchmark empty label row deletes");
                } else {
                    transaction
                        .put(
                            label_key,
                            secondary::SecondaryEqualityValue::encode_ids(&labels),
                        )
                        .expect("exclusive benchmark label replacement stages");
                }
                if adjacency.is_empty() {
                    transaction
                        .delete(adjacency_key)
                        .expect("exclusive benchmark empty adjacency row deletes");
                } else {
                    transaction
                        .put(adjacency_key, edges::encode_edges(&adjacency))
                        .expect("exclusive benchmark adjacency replacement stages");
                }
            }
            SweepBenchmarkStrategy::DisjointDelta => {
                let mut runtime = TopologyMutationRuntime::default();
                for id in ids {
                    if present {
                        runtime
                            .add_node_label(DataScope::LegacyUnscoped, LABEL, id)
                            .unwrap();
                        runtime
                            .add_adjacency(
                                DataScope::LegacyUnscoped,
                                PARENT_ID,
                                id,
                                helix_planner::ir::ExpandDirection::Out,
                            )
                            .unwrap();
                    } else {
                        runtime
                            .remove_node_label(DataScope::LegacyUnscoped, LABEL, id)
                            .unwrap();
                        runtime
                            .remove_adjacency(
                                DataScope::LegacyUnscoped,
                                PARENT_ID,
                                id,
                                helix_planner::ir::ExpandDirection::Out,
                            )
                            .unwrap();
                    }
                }
                runtime.prepare(&transaction).await.unwrap();
                runtime.consume_prepared().unwrap();
            }
        }
        transaction
    }

    async fn run_sweep_benchmark(
        strategy: SweepBenchmarkStrategy,
        expired_members: u64,
        inserted_members: u64,
        sweep_chunk: u64,
        writer_chunk: u64,
    ) -> SweepBenchmarkResult {
        const PARENT_ID: u64 = u64::MAX - 1;
        const LABEL: &str = "KubernetesEvent";

        let db = Arc::new(
            database(match strategy {
                SweepBenchmarkStrategy::ExclusiveReadModifyWrite => "benchmark-exclusive",
                SweepBenchmarkStrategy::DisjointDelta => "benchmark-disjoint",
            })
            .await,
        );
        let label_key = node_label_key(DataScope::LegacyUnscoped, LABEL);
        db.put(
            &label_key,
            secondary::SecondaryEqualityValue::encode_ids(
                &(0..expired_members).collect::<RoaringTreemap>(),
            ),
        )
        .await
        .expect("benchmark label seed persists");
        let mut adjacency = edges::Edges::new();
        for id in 0..expired_members {
            adjacency.add_out(id);
        }
        db.put(
            DataKey::Data {
                scope: DataScope::LegacyUnscoped,
                kind: DataKeyKind::Adjacency(AdjacencyKey::new(PARENT_ID)),
            }
            .to_bytes(),
            edges::encode_edges(&adjacency),
        )
        .await
        .expect("benchmark adjacency seed persists");

        let writer_chunks = inserted_members.div_ceil(writer_chunk) as usize;
        let sweeper_chunks = expired_members.div_ceil(sweep_chunk) as usize;
        let synchronized_chunks = writer_chunks.min(sweeper_chunks);
        let barriers = (0..synchronized_chunks)
            .map(|_| Arc::new(tokio::sync::Barrier::new(2)))
            .collect::<Vec<_>>();
        let writer_progress = Arc::new(AtomicUsize::new(0));
        let writer_notify = Arc::new(tokio::sync::Notify::new());
        let started = Instant::now();

        let writer = {
            let db = Arc::clone(&db);
            let barriers = barriers.clone();
            let writer_progress = Arc::clone(&writer_progress);
            let writer_notify = Arc::clone(&writer_notify);
            tokio::spawn(async move {
                let mut conflicts = 0;
                for chunk in 0..writer_chunks {
                    let start = expired_members + chunk as u64 * writer_chunk;
                    let end = (start + writer_chunk).min(expired_members + inserted_members);
                    let mut first_attempt = true;
                    loop {
                        let transaction =
                            stage_benchmark_memberships(&db, strategy, start..end, true).await;
                        if first_attempt && chunk < synchronized_chunks {
                            barriers[chunk].wait().await;
                        }
                        match transaction.commit().await {
                            Ok(_) => break,
                            Err(error) if error.kind() == slatedb::ErrorKind::Transaction => {
                                conflicts += 1;
                                first_attempt = false;
                            }
                            Err(error) => panic!("benchmark writer commit failed: {error}"),
                        }
                    }
                    writer_progress.store(chunk + 1, Ordering::Release);
                    writer_notify.notify_waiters();
                }
                conflicts
            })
        };
        let sweeper = {
            let db = Arc::clone(&db);
            let barriers = barriers.clone();
            let writer_progress = Arc::clone(&writer_progress);
            let writer_notify = Arc::clone(&writer_notify);
            tokio::spawn(async move {
                let mut conflicts = 0;
                for chunk in 0..sweeper_chunks {
                    let start = chunk as u64 * sweep_chunk;
                    let end = (start + sweep_chunk).min(expired_members);
                    let mut first_attempt = true;
                    loop {
                        let transaction =
                            stage_benchmark_memberships(&db, strategy, start..end, false).await;
                        if first_attempt && chunk < synchronized_chunks {
                            barriers[chunk].wait().await;
                            loop {
                                let notified = writer_notify.notified();
                                if writer_progress.load(Ordering::Acquire) > chunk {
                                    break;
                                }
                                notified.await;
                            }
                        }
                        match transaction.commit().await {
                            Ok(_) => break,
                            Err(error) if error.kind() == slatedb::ErrorKind::Transaction => {
                                conflicts += 1;
                                first_attempt = false;
                            }
                            Err(error) => panic!("benchmark sweeper commit failed: {error}"),
                        }
                    }
                }
                conflicts
            })
        };
        let writer_conflicts = writer.await.expect("benchmark writer joins");
        let sweeper_conflicts = sweeper.await.expect("benchmark sweeper joins");
        let elapsed = started.elapsed();

        let labels = secondary::SecondaryEqualityValue::decode(
            &db.get(&label_key)
                .await
                .expect("benchmark result reads")
                .expect("inserted memberships remain"),
        )
        .expect("benchmark result decodes")
        .into_ids();
        assert_eq!(labels.len(), inserted_members);
        assert_eq!(labels.min(), Some(expired_members));
        assert_eq!(labels.max(), Some(expired_members + inserted_members - 1));
        db.close().await.expect("benchmark database closes");

        SweepBenchmarkResult {
            elapsed,
            writer_conflicts,
            sweeper_conflicts,
            writer_commits: writer_chunks,
            sweeper_commits: sweeper_chunks,
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore = "manual 100K-member conflict and throughput comparison"]
    async fn benchmark_exclusive_vs_disjoint_linked_event_sweep() {
        let expired_members = std::env::var("HELIX_SWEEP_BENCH_EXPIRED")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(100_000);
        let inserted_members = std::env::var("HELIX_SWEEP_BENCH_INSERTED")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(10_000);
        let sweep_chunk = std::env::var("HELIX_SWEEP_BENCH_SWEEP_CHUNK")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(1_000);
        let writer_chunk = std::env::var("HELIX_SWEEP_BENCH_WRITER_CHUNK")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(250);
        assert!(expired_members > 0);
        assert!(inserted_members > 0);
        assert!(sweep_chunk > 0);
        assert!(writer_chunk > 0);

        let before = run_sweep_benchmark(
            SweepBenchmarkStrategy::ExclusiveReadModifyWrite,
            expired_members,
            inserted_members,
            sweep_chunk,
            writer_chunk,
        )
        .await;
        let after = run_sweep_benchmark(
            SweepBenchmarkStrategy::DisjointDelta,
            expired_members,
            inserted_members,
            sweep_chunk,
            writer_chunk,
        )
        .await;
        let before_sweep_conflict_rate =
            before.sweeper_conflicts as f64 / before.sweeper_commits as f64;
        let after_sweep_conflict_rate =
            after.sweeper_conflicts as f64 / after.sweeper_commits as f64;
        let operations = expired_members + inserted_members;
        let before_throughput = operations as f64 / before.elapsed.as_secs_f64();
        let after_throughput = operations as f64 / after.elapsed.as_secs_f64();

        println!(
            "SWEEP_BENCH before elapsed_ms={} sweep_commits={} sweep_conflicts={} sweep_conflict_rate={:.4} writer_commits={} writer_conflicts={} throughput_members_per_s={:.0}",
            before.elapsed.as_millis(),
            before.sweeper_commits,
            before.sweeper_conflicts,
            before_sweep_conflict_rate,
            before.writer_commits,
            before.writer_conflicts,
            before_throughput,
        );
        println!(
            "SWEEP_BENCH after elapsed_ms={} sweep_commits={} sweep_conflicts={} sweep_conflict_rate={:.4} writer_commits={} writer_conflicts={} throughput_members_per_s={:.0}",
            after.elapsed.as_millis(),
            after.sweeper_commits,
            after.sweeper_conflicts,
            after_sweep_conflict_rate,
            after.writer_commits,
            after.writer_conflicts,
            after_throughput,
        );
        println!(
            "SWEEP_BENCH delta conflict_rate_points={:.2} throughput_ratio={:.3}",
            (after_sweep_conflict_rate - before_sweep_conflict_rate) * 100.0,
            after_throughput / before_throughput,
        );

        assert!(before.sweeper_conflicts > 0);
        assert_eq!(after.sweeper_conflicts, 0);
        assert_eq!(after.writer_conflicts, 0);
    }
}
