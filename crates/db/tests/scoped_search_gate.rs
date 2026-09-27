//! Release-only regression gate for traversal-scoped search on a synthetic
//! `Group <- Item -> Attribute` graph shaped like a production product graph.
//!
//! Every scoped vector shape must stay inside its traversal scope and keep
//! recall@k against a client-side exact scan, and a stored-property filter
//! after expansion must count exactly the rows a client-side filter keeps.
//!
//! ```text
//! cargo test --release -p db --features production-scale --test scoped_search_gate -- --ignored --nocapture
//! ```
//!
//! `HELIX_SCOPED_GATE_SCALE` (default 0.02) and `HELIX_SCOPED_GATE_DIM`
//! (default 768) size the fixture.

#[path = "../examples/scoped_search_bench/fixture.rs"]
mod fixture;

use std::collections::HashSet;
use std::io::{Read, Write};
use std::sync::Arc;
use std::time::Duration;

use db::{DbConfig, HelixDB};
use helix_ast::prelude::*;
use slatedb::object_store::memory::InMemory;
use slatedb::object_store::ObjectStore;

const QUERIES: usize = 12;
const MIN_RECALL: f64 = 0.95;
/// Every index is created on the empty graph, so each build finishes at once.
const INDEX_DEADLINE: Duration = Duration::from_secs(5 * 60);

#[tokio::test(flavor = "multi_thread")]
#[ignore = "release-only scoped search recall and filter-semantics gate"]
async fn scoped_search_keeps_recall_and_exact_filter_semantics() {
    let scale = fixture::env_or("HELIX_SCOPED_GATE_SCALE", 0.02);
    let dimension = fixture::env_or("HELIX_SCOPED_GATE_DIM", 768);
    let store = Arc::new(fixture::CountingStore::new(
        Arc::new(InMemory::new()),
        Duration::ZERO,
    ));
    let db = HelixDB::open_with_object_store_and_config(
        "scoped-search-gate",
        Arc::clone(&store) as Arc<dyn ObjectStore>,
        DbConfig::new(),
    )
    .await
    .unwrap();
    let backend = fixture::Backend::Embedded { db, store };
    fixture::load(
        &backend,
        fixture::LoadOptions {
            scale,
            dimension,
            items_per_batch: 4,
            vector: fixture::VectorBuild::Before,
            index_deadline: INDEX_DEADLINE,
        },
    )
    .await;

    let vectors = fixture::query_vectors(QUERIES, dimension);
    let shape_count = fixture::shapes(&vectors[0]).len();
    for shape_index in 0..shape_count {
        let (name, scope, _) = fixture::shapes(&vectors[0]).swap_remove(shape_index);
        let Some((scope, k)) = scope else {
            continue;
        };
        let candidates = fixture::candidate_embeddings(&backend, scope).await;
        let candidate_ids = candidates.iter().map(|(id, _)| *id).collect::<HashSet<_>>();
        let recall = futures::future::join_all(vectors.iter().map(|vector| {
            let (_, _, request) = fixture::shapes(vector).swap_remove(shape_index);
            let backend = &backend;
            let candidates = &candidates;
            let candidate_ids = &candidate_ids;
            async move {
                let response = backend.query(request).await.unwrap();
                let returned = response["r"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|row| row["id"].as_u64().unwrap())
                    .collect::<HashSet<_>>();
                assert!(
                    returned.is_subset(candidate_ids),
                    "{name} returned ids outside its traversal scope"
                );
                let expected = fixture::exact_top_k(candidates, vector, k);
                expected.iter().filter(|id| returned.contains(id)).count() as f64
                    / expected.len().max(1) as f64
            }
        }))
        .await;
        let mean = recall.iter().sum::<f64>() / recall.len() as f64;
        println!("{name}: recall={mean:.3} candidates={}", candidates.len());
        assert!(mean >= MIN_RECALL, "{name} recall {mean:.3} < {MIN_RECALL}");
    }

    // The post-expansion filter may be answered from index bitmaps; it must
    // agree with filtering the expanded rows client-side.
    let filtered = backend
        .query(fixture::read(
            fixture::group_scope()
                .where_(Predicate::eq("kind", "B"))
                .count(),
        ))
        .await
        .unwrap();
    let expanded = backend
        .query(fixture::read(
            fixture::group_scope().project(vec![PropertyProjection::new("kind")]),
        ))
        .await
        .unwrap();
    let expected = expanded["r"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["kind"] == "B")
        .count() as u64;
    assert!(expected > 0, "fixture has kind-B attributes in scope");
    assert_eq!(filtered["r"].as_u64(), Some(expected));
}

/// A build that never leaves the queue fails the wait at its deadline with
/// the last status, instead of hanging the gate.
#[tokio::test(flavor = "multi_thread")]
#[should_panic(
    expected = r#"index vector did not succeed within 1s; last status: {"op":{"status":"queued"}}"#
)]
async fn index_wait_fails_at_its_deadline_with_the_last_status() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    // Answers every status poll with a build that stays queued.
    std::thread::spawn(move || {
        let body = r#"{"op":{"status":"queued"}}"#;
        for mut stream in listener.incoming().map_while(Result::ok) {
            let _ = stream.read(&mut [0; 4_096]);
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.shutdown(std::net::Shutdown::Write);
            let _ = std::io::copy(&mut stream, &mut std::io::sink());
        }
    });
    let backend = fixture::Backend::Http {
        client: reqwest::Client::builder().no_proxy().build().unwrap(),
        url,
    };
    fixture::wait_for_operations(
        &backend,
        &serde_json::json!({ "vector": { "operation_id": "00000000-0000-4000-8000-000000000000" } }),
        &["vector"],
        Duration::from_secs(1),
    )
    .await;
}
