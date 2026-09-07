# Ordered range review validation

Validated on 2026-09-07 against the implementation branch merged with main.

## Corrections

- Both production lifecycle range fixtures explicitly select forward iteration, fixing feature-enabled merge compilation.
- The production-linked exact-storage contract now exercises nodes and edges, both physical directions, forward/reverse iteration, bounded results, membership, stale tie recovery, exhausted recovery, and attempted property-decode failures.
- Calibration constructs independent ascending and descending order expectations, preserving ascending fixture/entity-ID order within ties. Regression tests reject reversed ties, shared tie-order errors, and changed results.

## Checks

- `cargo check -p db --features production-coverage,migration-parity,index-lifecycle-testing --tests`: passed.
- `cargo test --workspace`: passed (3,873 passing results across 57 result groups, including child test output).
- Workspace Clippy and Clippy for the production internal-contract target with production coverage features: passed with `-D warnings`.
- Rust formatting and all 19 Python image-harness tests: passed.
- Full production-linked integration suite from `scripts/db-production-coverage.sh`: all tests passed. The complete LLVM report passed every existing coverage floor. Its initial audit rejected only the old fingerprint; rerunning the same post-test audit against the refreshed fingerprint passed.
- Scanner production coverage: **190/211 lines (90.05%)**, **16/16 functions (100%)**, **306/334 regions (91.62%)**. This replaces the reported 54.98% line coverage. These are production-linked results, not unit-test-only coverage.
- Uncovered non-vector source lines decreased from **10,429 to 10,420**. Only the count and SHA-256 fingerprint changed; no floor was lowered and no exclusion was added. The checked-in coverage summary records the passing gate.

The source-gap review includes remaining malformed-entry/handle guards, recovery integrity checks, bound-dispatch branches, and uncovered await/error continuations. These remain in the fingerprinted production test backlog; this is not a claim of exhaustive production coverage.

## Built image

ARM64 image `helixdb:ordered-range-review`, ID `sha256:c018d1f14b735331deb44f73094e08c24977ca47ef27973f334216eeefaef09e`.

Archive inspection, secret scanning, memory-mode round trip, native-volume persistence, membership deletion/restart/reinsertion, and MinIO/S3-compatible Compose smoke tests passed. The actual corrected calibration function also passed against this image with 500 interleaved resources, 100 matching resources, and all timestamps equal.

No deployment is included. Hosted CI must validate the updated PR head separately.

## CI fingerprint correction

The Linux ARM production job for PR head `4c56a9cd6` passed all tests and every coverage floor, but its exact uncovered-line fingerprint differed from the local run above: **10,424** lines, SHA-256 `7343bd9430f31b4ac627ccf2c98aee62b62023270172b089c516718d5c1733b2`. The gate failed only on this mismatch. The baseline now uses that CI result; thresholds, exclusions, and runtime code are unchanged.

[CI evidence](https://github.com/HelixDB/helix-db/actions/runs/34118643886/job/101731282205). The checked-in coverage report above remains the historical local measurement. The CI job did not retain its full LLVM report, so the precise four-line difference has not been attributed to a specific platform or compiler behavior. A fresh CI run must confirm the updated fingerprint.
