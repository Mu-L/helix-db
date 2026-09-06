# Dalton type-read optimization

The planner can serve the two checked-in Dalton `/v2/query` requests through the existing ascending `Resource.last_seen` index, without changing the request, index identity, catalog, or stored bytes.

## Execution contract

- Physical index direction and traversal iteration are separate typed values.
- Logical `ORDER BY` remains in the optimizer. Each physical alternative uses the executable secondary-set lowering contract to prove its actual driver order. Nested intersections are flattened consistently. Selected lowering and executable validation reject false ordering claims.
- Ordered access builds its membership bitmap before checking range candidates. It checks each visited index entry before membership and reads properties only for eligible candidates.
- Eligible range access resolves nested bounds from inner to outer and validates every runtime expression before taking the minimum. A resolved zero skips access validation and scanning. Residual filters, projections, distinct, offsets, branches, and unrelated sources remain limit barriers.
- Reverse traversal normalizes each timestamp group to ascending entity IDs. Its ring holds at most the remaining result count. A stale retained candidate triggers an exact-value forward rescan only when candidates were discarded. Provisional results are discarded before recovery; errors propagate.
- Cancellation is checked before iterator reads, property reads, and recovery. Every phase shares one request snapshot. Projection is unchanged and can decode winning rows again.
- The limit bounds retained candidates and returned rows, not visited entries. Equality bitmap memory is separate. Large ties and stale-heavy recovery can still require substantial scanning.

Costing charges sequential membership construction, estimated visited range entries, authoritative verification, materialization, and property-key sorting. It does not use an unknown runtime limit as a cardinality estimate. A paired 50,000-entry arm64 image measurement found 0.1917 microseconds of additional reverse work per visited entry; the configurable default is 192 microseconds per 1,000 entries. This is a measured cost, not a plan-selection penalty.

Plan statistics expose reverse scans and dynamic bounded accesses. Internal selected operations also record satisfied ordering and the bound expression. Request-local test counters record range entries, verification reads/decodes, stale candidates, recovery scans, and peak ring size.

## Checked-in fixtures and harnesses

- `docker-image/tests/fixtures/dalton-find-resources-by-type.json`: original 56-property request.
- `docker-image/tests/fixtures/dalton-find-resource-dedup-keys-by-type.json`: original five-property request.
- `docker-image/tests/dalton_benchmark.py`: deterministic fixture generation, independent result oracle, paired warm reads, concurrent readers and insert/update/delete churn, calibration, and enforceable performance gates.
- `docker-image/tests/dalton_cold_benchmark.py`: native-volume process restarts, paired first reads, and process peak RSS. The kernel page cache is retained; this is not an S3-cold measurement.

All fixtures use the fixed cutoff `1788000000000000` (2026-08-29 10:40 UTC). Synthetic resource IDs are inserted in ascending order, so the oracle can check internal-ID ties independently. Every measured response is compared completely, including projection and ordering.

Example warm reference command after seeding disposable local containers:

```bash
python3 docker-image/tests/dalton_benchmark.py \
  --baseline 18196 --candidate 18197 --size 2000 --raw-bytes 4096 \
  --samples 100 --no-seed --require-gates --output results.json
```

Use fresh containers for a different fixture size, tie distribution, or blob size. Use `--calibrate` on a seeded candidate to pair forward and reverse full-range reads. Use `--workers 4 --churn --no-seed` for concurrent load. Churn creates, updates, and deletes other-tenant resources; matched resources remain fixed so each concurrent result has an independent oracle. Snapshot tests separately cover updates and deletes of matching entities.

## Correctness and review

Tests cover nodes and edges, both physical directions and iteration orders, inclusive/exclusive bounds, empty intervals, ties, stale recovery, malformed entry lookahead, skipped malformed blobs, attempted decode errors, cancellation, dynamic bound errors and precedence, empty/populated statistics, nested driver selection, iteration-independent range simplification, and false ordering claims.

A storage integration test reads across six SST flushes, overlapping property versions, entry tombstones, stale entries, an old writer snapshot, and a new reader that must replay WAL. The 100,000-equal-timestamp fixture checks a limit of 10 against an explicit-sort reference and asserts a maximum ring size of 10 and 10 authoritative reads/decodes for the optimized access.

No equality-only residual alternative, general top-N operator, shared decoded-row cache, dual-direction catalog change, migration, or deployment is included. Production acceptance still requires measurements on Dalton's actual cardinalities, timestamp ties, readers, and object storage.

## Validation evidence (2026-09-06)

Baseline: `b2d9ed30528b6bd19fb441af6fb377c1324141f1`, image `helixdb:dalton-baseline-b2d9ed3`. Delivery image: `helixdb:dalton-delivery`, local image ID `sha256:3b86392df287f51e545a937aea82d885922d4bc18cb0ac82d6d98e0623eba455` (linux/arm64).

The final native-volume reference has 2,000 matching resources among 10,000 interleaved resources, limit 1,000, and 4 KiB unprojected `raw_data`. Each case uses 100 paired samples after three warmups, with alternating image order.

| Query/window | Baseline p50/p95 ms | Delivery p50/p95 ms | Gate |
|---|---:|---:|---|
| find_resources_by_type / broad | 156.27 / 176.65 | 40.67 / 43.46 | pass |
| find_resources_by_type / narrow | 2.79 / 3.29 | 1.78 / 2.33 | pass |
| find_resource_dedup_keys_by_type / broad | 151.46 / 169.30 | 36.96 / 40.09 | pass |
| find_resource_dedup_keys_by_type / narrow | 2.96 / 3.52 | 1.81 / 2.24 | pass |

Both broad queries pass the 2x p50 and p95 gate. Both narrow controls pass the 10% regression budget. These are controlled synthetic measurements, not Dalton deployment acceptance.

Additional exploratory runs used 500, 1,000, 2,000, and 10,000 matching resources; no blob, 1 KiB, 4 KiB, and 32 KiB blobs; and distinct or tied timestamps. The below-limit/no-blob case did not reach 2x, which is not the reference acceptance workload. Short exploratory runs had isolated narrow-window p95 failures; the final reference above used 100 samples and passed. Earlier 50,000-resource image runs improved broad p95 by about 39x, but the final reference table is the delivery gate.

Four concurrent readers with other-tenant insert/update/delete churn returned the complete expected results for both projections on the delivery image (30 paired samples, native-volume reference fixture). Broad p50/p95 improved from 244.03/260.86 to 59.74/66.52 ms for the full projection, and from 246.48/260.64 to 59.62/65.09 ms for the five-property projection. Narrow controls passed. Each writer completed 73 cycles of 20 inserted, updated, then deleted entities.

- `cargo test --workspace`: 3845 passing test results across unit, integration, and documentation groups; no failures. The CLI TypeScript fixture required `npm ci` in `sdks/typescript` before its successful run.
- Additional explicit-sort counter comparison: 100,000 visited entries in each path; reference 100,000 verification reads/decodes and sorting inputs; optimized 10 verification reads/decodes, no sorting input, peak ring 10.
- `cargo clippy --workspace -- -D warnings`: passed. The broader optional `--all-targets` check found four unchanged pre-existing test lints in `execution/interpreter/mutation/topology.rs`; these were not folded into this change.
- Rust formatting and Python image-harness tests passed.
- LLVM coverage over engine/planner library suites: 94.33% of lines overall, 90.76% of the new reverse scanner, and 100% of the ordering-alternative implementation. These are line coverage figures, not a claim of exhaustive state-space coverage.
- Delivery image archive inspection, secret scan, memory-mode round trip, native-volume persistence, and MinIO/S3-compatible Compose smoke tests passed.

### Process-cold native-volume reference

Thirty paired first-read samples per query on the delivery image, restarting each process between samples. Builds and Rust test runs were complete before this run. Kernel page cache was retained. An earlier run during compilation had a 2.99-second candidate p95 outlier; the quiet delivery run below supersedes that contaminated measurement.

| Query | Baseline p50/p95 ms | Delivery p50/p95 ms | Baseline/delivery maximum RSS MiB |
|---|---:|---:|---:|
| find_resources_by_type | 674.76 / 731.64 | 118.81 / 138.20 | 82.97 / 45.48 |
| find_resource_dedup_keys_by_type | 666.00 / 764.15 | 107.42 / 135.77 | 83.20 / 45.20 |

Both queries also exceed 2x improvement at p50 and p95 in this process-cold reference. RSS includes the server, caches, and query allocations; it is not an isolated allocator measurement.

### Full projection and empty-result controls

A separate delivery-image fixture populates all 56 projected properties, with 2,000 matches among 10,000 resources and 4 KiB unprojected blobs (100 paired samples). Full-projection broad p95 improved from 237.11 to 66.96 ms; five-property broad p95 improved from 210.84 to 40.86 ms. Both p50 gates and both narrow controls passed. Reproduce with `--full-properties`.

The all-tied empty-result control was repeated with 1,000 paired samples after a marginal p95 failure in the 100-sample run. Full-projection p95 was 0.911 ms baseline versus 0.900 ms delivery; five-property p95 was 0.857 ms versus 0.860 ms. Both medians were within 3% of baseline. All controls passed the 10% budget in that longer run. Reproduce the focused control with `--tied --window narrow --samples 1000` on a fresh tied fixture.
