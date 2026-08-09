//! Production-linked index V2 lifecycle acceptance contracts.
//!
//! This target imports the compiled `db` crate without `cfg(test)` and invokes
//! feature-gated harness code that drives the real canonical and outbox
//! repositories.

use std::process::Command;

/// Proves valid incomplete schemas are promotable only through writer open.
#[tokio::test]
async fn index_lifecycle_writer_migration_requirements_are_typed() {
    db::production_coverage::writer_migration_requirement_contracts().await;
}

/// Runs every stable operation/upload crash boundary twice from clean storage.
#[tokio::test]
async fn index_lifecycle_outbox_failpoints_leave_only_legal_recovery_states() {
    db::production_coverage::index_lifecycle_outbox_failpoint_contracts().await;
}

/// Proves the explicit crash action terminates at its configured boundary.
#[test]
fn index_lifecycle_failpoint_abort_action_terminates_process() {
    const CHILD_ENV: &str = "HELIX_INDEX_OUTBOX_ABORT_PROBE_CHILD";
    if std::env::var_os(CHILD_ENV).is_some() {
        db::production_coverage::index_lifecycle_failpoint_abort_probe();
    }

    let status = Command::new(std::env::current_exe().expect("test executable path resolves"))
        .args([
            "--exact",
            "index_lifecycle_failpoint_abort_action_terminates_process",
            "--nocapture",
        ])
        .env(CHILD_ENV, "1")
        .env("HELIX_INDEX_OUTBOX_FAILPOINT", "commit_before")
        .env("HELIX_INDEX_OUTBOX_FAIL_ACTION", "abort")
        .status()
        .expect("abort probe child starts");
    assert!(
        !status.success(),
        "abort probe child must not exit normally"
    );
}

/// Compares lifecycle, mutations, and indexed reads with one reference model.
#[tokio::test]
async fn index_lifecycle_secondary_state_machine_matches_reference_model() {
    db::production_coverage::index_lifecycle_secondary_state_machine_contracts().await;
}

/// Proves the global operation queue retains exact tenant ownership.
#[tokio::test]
async fn index_lifecycle_global_outbox_discovers_sixteen_isolated_scopes() {
    db::production_coverage::index_lifecycle_multi_scope_discovery_contracts().await;
}

/// Proves compact V2 model and resource gates retain their typed boundaries.
#[test]
fn index_lifecycle_typed_boundaries_fail_closed() {
    db::production_coverage::index_lifecycle_typed_boundary_contracts();
}

/// Proves Active text serving reads reject every cross-owned durable row.
#[tokio::test]
async fn index_lifecycle_text_serving_reads_fail_closed() {
    db::production_coverage::index_lifecycle_text_serving_contracts().await;
}

/// Proves state-only Active text retirement validates before atomic staging.
#[tokio::test]
async fn index_lifecycle_active_text_retirement_fails_closed() {
    db::production_coverage::index_lifecycle_active_text_retirement_contracts().await;
}

/// Proves bounded sequential and concurrent V4 writes produce identical
/// bitmaps and that every configured equality lookup is one point read.
#[tokio::test(flavor = "multi_thread", worker_threads = 32)]
async fn secondary_equality_v4_bitmap_shape_and_read_io_are_exact() {
    use db::production_coverage::{SecondaryEqualityHotPathFixture, SecondaryEqualityInsertMode};

    let sequential = SecondaryEqualityHotPathFixture::open_correctness(
        "secondary-equality-v4-correctness-sequential",
    )
    .await
    .expect("sequential V4 correctness fixture opens");
    sequential
        .insert(SecondaryEqualityInsertMode::Sequential)
        .await
        .expect("sequential V4 correctness inserts succeed");
    let sequential_inspection = sequential
        .inspect()
        .await
        .expect("sequential V4 correctness inspection succeeds");
    assert_eq!(sequential_inspection.physical_secondary_rows, 50);
    assert_eq!(sequential_inspection.v3_nonunique_rows, 0);
    assert_eq!(sequential_inspection.v4_bitmap_rows, 50);
    assert_eq!(sequential_inspection.minimum_bitmap_cardinality, 100);
    assert_eq!(sequential_inspection.maximum_bitmap_cardinality, 100);
    let lookup = sequential
        .inspect_all_lookups()
        .await
        .expect("all sequential V4 equality lookups succeed");
    assert_eq!(lookup.lookups, 50);
    assert_eq!(lookup.result_count, 100);
    assert_eq!(lookup.point_reads, 50);
    assert_eq!(lookup.scans, 0);
    assert_eq!(lookup.graph_reads, 0);
    let sequential_rows = sequential
        .decoded_bitmap_rows()
        .await
        .expect("sequential V4 bitmap rows decode");

    let concurrent = SecondaryEqualityHotPathFixture::open_correctness(
        "secondary-equality-v4-correctness-concurrent",
    )
    .await
    .expect("concurrent V4 correctness fixture opens");
    concurrent
        .insert(SecondaryEqualityInsertMode::Concurrent)
        .await
        .expect("concurrent V4 correctness inserts succeed");
    let concurrent_inspection = concurrent
        .inspect()
        .await
        .expect("concurrent V4 correctness inspection succeeds");
    assert_eq!(concurrent_inspection, sequential_inspection);
    let concurrent_rows = concurrent
        .decoded_bitmap_rows()
        .await
        .expect("concurrent V4 bitmap rows decode");
    let assert_one_entity_set_per_index = |rows: &[(Vec<u8>, Vec<u64>)]| {
        let Some((_, expected_ids)) = rows.first() else {
            panic!("V4 correctness fixture must contain bitmap rows");
        };
        assert_eq!(expected_ids.len(), 100);
        assert!(rows.iter().all(|(_, ids)| ids == expected_ids));
    };
    assert_one_entity_set_per_index(&sequential_rows);
    assert_one_entity_set_per_index(&concurrent_rows);
    assert_eq!(
        concurrent_rows
            .iter()
            .map(|(key, _)| key)
            .collect::<Vec<_>>(),
        sequential_rows
            .iter()
            .map(|(key, _)| key)
            .collect::<Vec<_>>()
    );

    sequential
        .close()
        .await
        .expect("sequential V4 correctness fixture closes");
    concurrent
        .close()
        .await
        .expect("concurrent V4 correctness fixture closes");
}
