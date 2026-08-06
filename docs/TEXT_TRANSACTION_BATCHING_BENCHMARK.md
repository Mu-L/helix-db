# Transaction-batched Active V3 text deltas benchmark

Measured 2026-07-31. The acceptance target passed: for an unpartitioned Active V3 text index, insert transactions changed from `N` FTS uploads and `N` manifest entries to one upload and one manifest entry for batches of 1, 10, 100, and 500. Search-result digests were identical before and after compaction and across all three revisions.

## Revisions and protocol

| Revision | Source SHA | Harness SHA |
| --- | --- | --- |
| PR #46 baseline | `1017c02e` | `d44b2b938f15c3bbb3adc93eef0baa117f0f003d` |
| PR #47 baseline | `9fda2b5352667167cb6a1200de9223b462d42691` | `ee97f6d5224887b06f610848122aab42d8fb6d25` |
| Transaction batching | `0321a5f10d8b2d4037d88adc453af1522080e039` | same as source |

- Three independent runs per case, three warmups per run, and 25 measured samples per run: 75 measured samples per revision, batch size, and latency mode.
- Release builds with `rustc 1.97.1 (8bab26f4f 2026-07-14)`, LLVM 22.1.6, `aarch64-apple-darwin`.
- Apple M4 Pro Mac16,7, 24 GiB RAM, Darwin 25.5.0. No benchmark revisions ran concurrently.
- Fresh local-filesystem database, identical deterministic documents, configuration, queries, and cache state for every sample.
- Modes were local storage and local storage with a synthetic 1 ms delay applied only to FTS blob uploads.
- Compaction was held through the immediate search measurement. The database was then reopened with automatic compaction and observed until quiescent before the post-compaction search.
- Median and p50 are the same nearest-rank observation. p95 uses index `ceil((n - 1) * 0.95)` over the 75 aggregate samples.

The complete aggregate metrics and all 24 independent-run summaries per revision are in [text_transaction_batching_benchmark_results.json](text_transaction_batching_benchmark_results.json).

## Transaction latency and throughput

Times are aggregate p50 / p95 milliseconds. Throughput is aggregate p50 inserts/second.

| Upload mode | Batch | PR #46 ms | PR #47 ms | After ms | PR #47 throughput | After throughput |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| local | 1 | 203.5 / 319.1 | 202.8 / 304.0 | 202.7 / 204.9 | 4.93 | 4.93 |
| local | 10 | 1081.8 / 1108.6 | 1064.6 / 1087.5 | 303.4 / 307.0 | 9.39 | 32.96 |
| local | 100 | 8682.8 / 8953.6 | 8702.1 / 8848.8 | 305.2 / 307.3 | 11.49 | 327.66 |
| local | 500 | 42936.0 / 43240.1 | 43036.8 / 44293.5 | 507.8 / 509.6 | 11.62 | 984.56 |
| synthetic 1 ms | 1 | 202.9 / 307.0 | 202.6 / 205.1 | 202.4 / 206.0 | 4.94 | 4.94 |
| synthetic 1 ms | 10 | 1060.3 / 1152.3 | 1079.6 / 1109.4 | 304.9 / 306.5 | 9.26 | 32.80 |
| synthetic 1 ms | 100 | 9348.1 / 9478.1 | 9003.4 / 9173.5 | 305.1 / 307.3 | 11.11 | 327.78 |
| synthetic 1 ms | 500 | 44760.5 / 46851.6 | 44435.3 / 44798.8 | 508.1 / 509.6 | 11.25 | 984.15 |

Against the primary PR #47 baseline, local p50 transaction latency improved by 3.5x, 28.5x, and 84.7x for batches 10, 100, and 500. Under synthetic upload latency, the improvements were 3.5x, 29.5x, and 87.5x.

## Primary structural and search comparison

Counts and bytes are p50; their p95 values were identical in every case. Search times are p50 / p95 milliseconds.

| Upload mode | Batch | Uploads | Upload bytes | Manifest growth | Immediate search ms | Post-compaction search ms | Post-compaction splits |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| local | 1 | 1 -> 1 | 6,691 -> 6,691 | 1 -> 1 | 1.034 / 1.387 -> 1.065 / 1.395 | 3.772 / 4.378 -> 3.748 / 4.338 | 1 -> 1 |
| local | 10 | 10 -> 1 | 66,730 -> 6,915 | 10 -> 1 | 3.680 / 4.439 -> 1.341 / 1.928 | 8.041 / 8.897 -> 5.786 / 6.416 | 10 -> 1 |
| local | 100 | 100 -> 1 | 667,570 -> 8,991 | 100 -> 1 | 23.758 / 25.338 -> 1.159 / 1.254 | 24.658 / 25.896 -> 22.016 / 23.165 | 7 -> 1 |
| local | 500 | 500 -> 1 | 3,338,244 -> 12,570 | 500 -> 1 | 78.181 / 80.225 -> 2.894 / 3.037 | 101.430 / 103.374 -> 95.208 / 97.532 | 35 -> 1 |
| synthetic 1 ms | 1 | 1 -> 1 | 6,691 -> 6,691 | 1 -> 1 | 1.132 / 1.441 -> 1.122 / 1.495 | 3.977 / 4.705 -> 3.899 / 4.544 | 1 -> 1 |
| synthetic 1 ms | 10 | 10 -> 1 | 66,754 -> 6,915 | 10 -> 1 | 3.988 / 4.764 -> 1.415 / 1.999 | 8.500 / 9.759 -> 5.932 / 6.867 | 10 -> 1 |
| synthetic 1 ms | 100 | 100 -> 1 | 667,498 -> 9,015 | 100 -> 1 | 23.289 / 25.113 -> 1.156 / 1.222 | 24.685 / 25.839 -> 22.315 / 23.553 | 7 -> 1 |
| synthetic 1 ms | 500 | 500 -> 1 | 3,338,340 -> 12,570 | 500 -> 1 | 78.534 / 80.674 -> 2.824 / 2.999 | 101.759 / 103.389 -> 95.475 / 97.665 | 35 -> 1 |

The only apparent regression was local batch-1 immediate search: p50 increased by 31 microseconds (3.0%) and p95 by 8 microseconds (0.5%). Transaction latency and post-compaction search improved for that case, and the change is small relative to run dispersion, so it is classified as measurement noise rather than a batching regression.

## Correctness and delete-only acceptance

Each batch had one digest across every measured sample, latency mode, revision, and immediate/post-compaction pair:

| Batch | Search digest |
| ---: | --- |
| 1 | `2c0a8a99dc147d5445c3b49d035665b2` |
| 10 | `de5c02a2a7364a243afefc27e2b2388d` |
| 100 | `d2c89607ccb62de412374102cf825b1f` |
| 500 | `302c0633976aec6aa04550dbb7ecc242` |

The insert benchmark does not manufacture a delete-only batch. The production-linked `interpreter_active_text_transactions_batch_by_destination` contract separately asserts that delete-only destinations update root/state while producing zero uploads and zero manifest growth.
