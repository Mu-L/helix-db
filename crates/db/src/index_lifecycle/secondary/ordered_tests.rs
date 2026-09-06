use std::sync::atomic::{AtomicUsize, Ordering};

use helix_planner::ir::RangeScanIteration::{Forward, Reverse};

use super::tests::{active_read_handle, put_read_entry, test_db};
use super::*;
use crate::config::SecondaryIndexDefinition;
use crate::encoding::v2::values::property::encode_properties;

#[tokio::test]
async fn reverse_range_preserves_value_order_and_ascending_id_ties() {
    for direction in [RangeIndexDirection::Asc, RangeIndexDirection::Desc] {
        for definition in [
            SecondaryIndexDefinition::node_range_with_direction("User", "value", direction)
                .unwrap(),
            SecondaryIndexDefinition::edge_range_with_direction("LINK", "value", direction)
                .unwrap(),
        ] {
            let db = test_db("reverse-range-order").await;
            let handle = active_read_handle(&db, definition).await;
            for (id, value) in [(1, "a"), (5, "b"), (2, "b"), (9, "c")] {
                put_read_entry(&db, &handle, value, id).await;
            }
            for iteration in [Forward, Reverse] {
                let expected = match (direction, iteration) {
                    (RangeIndexDirection::Asc, Forward) | (RangeIndexDirection::Desc, Reverse) => {
                        vec![1, 2, 5, 9]
                    }
                    _ => vec![9, 2, 5, 1],
                };
                for limit in [None, Some(0), Some(1), Some(3), Some(10)] {
                    let counters = RangeScanCounters::default();
                    let actual = scan_active_range_generation_ordered(
                        &db,
                        &handle,
                        None,
                        iteration,
                        limit,
                        &[],
                        &counters,
                    )
                    .await
                    .unwrap();
                    assert_eq!(
                        actual,
                        expected
                            .iter()
                            .copied()
                            .take(limit.unwrap_or(usize::MAX))
                            .collect::<Vec<_>>()
                    );
                }
            }
            let lower = SecondaryRangeQuery::Lower {
                value: PropertyValue::String("a".into()),
                inclusive: false,
            };
            let actual = scan_active_range_generation_ordered(
                &db,
                &handle,
                Some(&lower),
                Reverse,
                None,
                &[],
                &RangeScanCounters::default(),
            )
            .await
            .unwrap();
            assert_eq!(
                actual,
                if direction == RangeIndexDirection::Asc {
                    vec![9, 2, 5]
                } else {
                    vec![2, 5, 9]
                }
            );
            db.close().await.unwrap();
        }
    }
}

#[tokio::test]
async fn reverse_range_stale_tie_recovery_publishes_once_and_exhausts() {
    let db = test_db("reverse-range-stale-ties").await;
    let handle = active_read_handle(
        &db,
        SecondaryIndexDefinition::node_range_with_direction(
            "User",
            "value",
            RangeIndexDirection::Asc,
        )
        .unwrap(),
    )
    .await;
    for id in 1..=5 {
        put_read_entry(&db, &handle, "same", id).await;
    }
    let property_key = |id| {
        authoritative_property_key(
            handle.scope(),
            IndexEntity {
                kind: IndexElementKind::Node,
                id: IndexEntityId::new(id),
            },
        )
    };
    db.delete(property_key(2)).await.unwrap();
    let counters = RangeScanCounters::default();
    assert_eq!(
        scan_active_range_generation_ordered(&db, &handle, None, Reverse, Some(2), &[], &counters)
            .await
            .unwrap(),
        vec![1, 3]
    );
    assert_eq!(counters.fallbacks.load(Ordering::Relaxed), 1);
    assert_eq!(counters.peak.load(Ordering::Relaxed), 2);
    for id in [1, 3, 4] {
        db.delete(property_key(id)).await.unwrap();
    }
    assert_eq!(
        scan_active_range_generation_ordered(
            &db,
            &handle,
            None,
            Reverse,
            Some(2),
            &[],
            &RangeScanCounters::default()
        )
        .await
        .unwrap(),
        vec![5]
    );
    db.delete(property_key(5)).await.unwrap();
    assert!(scan_active_range_generation_ordered(
        &db,
        &handle,
        None,
        Reverse,
        Some(2),
        &[],
        &RangeScanCounters::default()
    )
    .await
    .unwrap()
    .is_empty());
    db.close().await.unwrap();
}

#[tokio::test]
async fn reverse_range_skips_unneeded_blobs_but_propagates_attempted_decode_errors() {
    let db = test_db("reverse-range-corruption").await;
    let handle = active_read_handle(
        &db,
        SecondaryIndexDefinition::node_range_with_direction(
            "User",
            "value",
            RangeIndexDirection::Asc,
        )
        .unwrap(),
    )
    .await;
    for id in 1..=4 {
        put_read_entry(&db, &handle, "same", id).await;
    }
    db.put(
        authoritative_property_key(
            handle.scope(),
            IndexEntity {
                kind: IndexElementKind::Node,
                id: IndexEntityId::new(4),
            },
        ),
        Bytes::from_static(b"corrupt"),
    )
    .await
    .unwrap();
    // A discarded member is outside the bounded verification set.
    let counters = RangeScanCounters::default();
    assert_eq!(
        scan_active_range_generation_ordered(&db, &handle, None, Reverse, Some(2), &[], &counters)
            .await
            .unwrap(),
        vec![1, 2]
    );
    assert_eq!(counters.reads.load(Ordering::Relaxed), 2);
    // Non-members are skipped in both storage directions.
    let members = [roaring::RoaringTreemap::from_iter([1, 2])];
    for iteration in [Forward, Reverse] {
        assert_eq!(
            scan_active_range_generation_ordered(
                &db,
                &handle,
                None,
                iteration,
                None,
                &members,
                &RangeScanCounters::default()
            )
            .await
            .unwrap(),
            vec![1, 2]
        );
    }
    // Reading the corrupt member must fail, never trigger stale recovery.
    let counters = RangeScanCounters::default();
    assert!(scan_active_range_generation_ordered(
        &db,
        &handle,
        None,
        Reverse,
        None,
        &[],
        &counters
    )
    .await
    .is_err());
    assert_eq!(counters.fallbacks.load(Ordering::Relaxed), 0);
    db.close().await.unwrap();
}

#[tokio::test]
async fn reverse_range_large_tie_has_bounded_retention_and_authoritative_reads() {
    let db = test_db("reverse-range-large-tie").await;
    let handle = active_read_handle(
        &db,
        SecondaryIndexDefinition::node_range_with_direction(
            "User",
            "value",
            RangeIndexDirection::Asc,
        )
        .unwrap(),
    )
    .await;
    let definition = handle.secondary_definition().unwrap();
    for chunk in 0..100 {
        let mut batch = slatedb::WriteBatch::new();
        for offset in 1..=1_000 {
            let id = IndexEntityId::new(chunk * 1_000 + offset);
            batch.put(
                secondary_entry_key(
                    handle.scope(),
                    handle.index_id(),
                    handle.generation(),
                    definition,
                    CanonicalSecondaryValue::range_string(StorageRangeIndexDirection::Asc, "same"),
                    id,
                )
                .unwrap(),
                encode_secondary_entry(&SecondaryEntryValue {
                    index_id: handle.index_id(),
                    generation: handle.generation(),
                    lane: definition_lane(definition),
                    entity_id: id,
                }),
            );
            batch.put(
                authoritative_property_key(
                    handle.scope(),
                    IndexEntity {
                        kind: definition.element_kind(),
                        id,
                    },
                ),
                encode_properties(&[
                    Property::string("$label", "User"),
                    Property::string("value", "same"),
                ]),
            );
        }
        db.write(batch).await.unwrap();
    }
    let counters = RangeScanCounters::default();
    assert_eq!(
        scan_active_range_generation_ordered(&db, &handle, None, Reverse, Some(10), &[], &counters)
            .await
            .unwrap(),
        (1..=10).collect::<Vec<_>>()
    );
    assert_eq!(counters.entries.load(Ordering::Relaxed), 100_000);
    assert_eq!(counters.reads.load(Ordering::Relaxed), 10);
    assert_eq!(counters.decodes.load(Ordering::Relaxed), 10);
    assert_eq!(counters.peak.load(Ordering::Relaxed), 10);
    db.close().await.unwrap();
}

#[tokio::test]
async fn reverse_range_cancels_inside_rejected_membership_and_tie_verification() {
    struct CancelAfter(AtomicUsize);
    impl ExactRangeScanProgress for CancelAfter {
        fn checkpoint(&self) -> Result<()> {
            self.0
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |left| {
                    left.checked_sub(1)
                })
                .map(|_| ())
                .map_err(|_| HelixDbError::QueryDeadlineExceeded)
        }
    }
    let db = test_db("reverse-range-cancellation").await;
    let handle = active_read_handle(
        &db,
        SecondaryIndexDefinition::node_range_with_direction(
            "User",
            "value",
            RangeIndexDirection::Asc,
        )
        .unwrap(),
    )
    .await;
    for id in 1..=100 {
        put_read_entry(&db, &handle, "same", id).await;
    }
    for (members, checkpoints) in [
        (vec![roaring::RoaringTreemap::from_iter([1000])], 20),
        (vec![], 105),
    ] {
        assert!(matches!(
            scan_active_range_generation_ordered(
                &db,
                &handle,
                None,
                Reverse,
                Some(10),
                &members,
                &CancelAfter(AtomicUsize::new(checkpoints))
            )
            .await,
            Err(HelixDbError::QueryDeadlineExceeded)
        ));
    }
    db.close().await.unwrap();
}
