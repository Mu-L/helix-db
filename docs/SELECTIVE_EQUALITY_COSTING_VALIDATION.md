# Selective equality costing regression (HEL-850)

## Reproduction and scope

The serial-child-cost change in [#1081](https://github.com/HelixDB/helix-db/pull/1081)
reproduces the plan-selection flip with empty statistics. Both access candidates
survive exploration; the regression is their cost comparison, not pruning or a
missing index. This is a controlled reproduction of the suspected trigger in
[HEL-850](https://linear.app/helix-db/issue/HEL-850), not a capture of the production request.

GitHub delivery evidence is [Hyperscale #313](https://github.com/HelixDB/helix-hyperscale/pull/313):
the new engine pin is `57877a76b307b6838ebd950c5f05dab4b7767d7e`, and its previous
pin is `1402bd27980a722d8d740cfe1aa87a5358a0523c`. Searching the linked changes and
the Hyperscale source did not locate the exact affected request envelope, tenant
catalog snapshot, or running image digest. Image builder code records digests at
runtime; the GitHub source pin alone does not establish what the tenant ran.

The planner fixture uses `Resource` with active, non-unique equality indexes on
`tenant`, `type`, and `deleted`, and this traversal:

```text
Resource WHERE tenant = "one" AND type = "pod" AND deleted = true
  VALUES ["id"]
```

Statistics are `StatsSnapshot::default()`: 1,000 estimated scan rows and 10
estimated rows for each equality. Parameters and tenant catalog are held constant.
The HTTP benchmark generates a complete equivalent envelope in
`docker-image/tests/equality_benchmark.py::lookup` and binds all three predicates.

## Captured plans and complete comparison

The initial reproducer is commit `a25c2d57`. At that commit, the empty-statistics
plan assertion fails. Replacing only
`crates/planner/src/rules/physical_contracts/access/sets.rs` with the version from
`ad79300bb359b0728e30d0966e9d0d02fa524305^` makes it pass. Everything else remains
at the same checkout. This isolates the costing trigger rather than presenting a
mixed checkout as a historical deployed binary. The replacement was reverted
before implementing the fix.

Compact executable plan captures:

```text
Previous set costing:
  Intersect(Bitmap(tenant), Bitmap(type), Bitmap(deleted)) -> Values(id)
Current main before this fix:
  LabelScan(Resource) -> Filter(tenant AND type AND deleted) -> Values(id)
Fixed costing:
  Intersect(Bitmap(tenant), Bitmap(type), Bitmap(deleted)) -> Values(id)
```

All numbers below are estimated microseconds, not measured latency.

| Cost | Previous set costing | Current main | Fixed |
| --- | ---: | ---: | ---: |
| Equality child reads/decode | 5,135 (parallel, including 75 scheduling) | 15,180 (serial) | 15,180 (serial) |
| Intersection membership work | 10 | 10 | 30 |
| Final access-row construction | 10 | 10 | 10 |
| Complete indexed access candidate | 5,155 | 15,200 | 15,220 |
| Complete label access + residual candidate | 13,000 | 13,000 | 18,050 |
| Projection, either candidate | 1,000 | 1,000 | 1,000 |
| Selected full plan | 6,155 indexed | 14,000 scan | 16,220 indexed |

The fixed label candidate is 5,050 bitmap setup + 1,000 bitmap decode + 10,000
graph-row read/decode + 1,000 row construction + 1,000 residual evaluation. The
complete alternative with projection is 19,050. Projection retains the existing
conservative unknown-row estimate; changing it is not necessary for this fix.

Captured access-candidate work vectors:

| Candidate | Object reads | Graph reads | Range seeks / nexts | CPU units | Bytes | Peak bytes | Parallel width |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Original label + filter | 0 | 0 | 1 / 1,000 | 1,000 | 256,000 | 0 | 1 |
| Original serial intersection | 3 | 0 | 0 / 0 | 50 | 2,800 | 2,560 | 1 |
| Fixed label + filter | 1,001 | 1,000 | 0 / 0 | 4,000 | 520,000 | 256,000 | 1 |
| Fixed serial intersection | 3 | 0 | 0 / 0 | 70 | 2,800 | 2,560 | 1 |

`selective_equality_retains_full_cost_competition -- --nocapture` prints the two
retained alternatives and asserts their complete costs and selected winner.

## Cost-model contracts

Label access reads a label bitmap, then reads graph rows before emitting them.
It now includes this work rather than pricing itself as a bare range scan.
All-element access likewise includes graph-row reads and row construction.
These formulas use the existing tunable verification/read/decode budget; they do
not introduce a new measured latency constant. Residual evaluation stays separate.

Unordered intersections consume all input memberships, even when their estimated
output is zero. Input cardinalities determine set-work cost; output cardinality
determines the single final row-construction cost. Saturating sums preserve the
cost-domain invariant. Physical candidate costing and executable lowering use
the same accounting. Small populated label statistics can still make a scan win.

Ordered range drivers keep their existing visited-entry cost, membership-first
verification, reverse iteration, dynamic-limit handling, and tie semantics.
No executor behavior, index identity, snapshot contract, or encoding changes.

## Matched built-image results

Measurements taken 2026-09-09 with canonical `Dockerfile`, Rust 1.97.1,
`linux/arm64`, disposable localhost containers, and default in-memory storage:

| Image | Source revision | Local image ID |
| --- | --- | --- |
| Baseline (already includes ordered-read optimization) | `cb893050927a193444c9cbfb93209f8efee47ec8` | `sha256:7c71a70a2e43dfef1a771d94683b8448a0dc21fc89958d6ac92e0fa04e408860` |
| Candidate | `a1fbe5d54a1d4395f6b63d247be4dffb030804ef` | `sha256:5f8fbfbedebf0294e4da6b5d5febee303b4dd2ae827a6db5e07a15dc7d58cb96` |

These images capture the initial three-equality fix. The customer follow-up below
extends it for OR/AND sources. These local image IDs are not deployed digests.

The fixture has 10,000 resources, 4 KiB raw data per row, interleaved tenant/type
values, three rare deleted matches and an empty control. Each request's complete
response is compared to independently computed fixture results, including order
for ordered reads. There are three unmeasured warmups and alternating image order.
No concurrent local compilation or smoke suite ran during these final timings.

| Case | Samples per image | Baseline p50 / p95 / max ms | Candidate p50 / p95 / max ms |
| --- | ---: | --- | --- |
| Rare equality (3 rows) | 50 | 43.131 / 45.117 / 50.242 | 1.684 / 2.131 / 3.599 |
| Empty equality | 50 | 42.329 / 44.285 / 45.361 | 1.394 / 1.787 / 2.016 |
| Rare equality, 4 readers + churn | 120 | 72.612 / 79.183 / 81.168 | 2.943 / 6.081 / 11.929 |
| Empty equality, 4 readers + churn | 120 | 70.485 / 76.250 / 85.586 | 2.537 / 3.917 / 5.670 |
| Wide ordered projection, broad window (1,000 rows) | 100 | 17.305 / 18.142 / 19.905 | 16.175 / 17.549 / 19.645 |
| Wide ordered projection, narrow window (20 rows) | 100 | 2.965 / 3.567 / 4.070 | 1.780 / 2.072 / 2.234 |
| Narrow ordered projection, broad window (1,000 rows) | 100 | 15.807 / 16.744 / 17.130 | 14.927 / 15.791 / 17.701 |
| Narrow ordered projection, narrow window (20 rows) | 100 | 2.935 / 3.475 / 4.435 | 1.702 / 2.591 / 3.649 |

Churn completed 17 insert/update/delete cycles of 20 entities on each image.
The full precision results and image provenance are in
[`selective_equality_benchmark_results.json`](selective_equality_benchmark_results.json).
This demonstrates recovery to millisecond reads and retained ordered-read gains
for the controlled workload; it does not reproduce production storage latency.

Measured DB-test secondary counters are three bitmap point reads, zero secondary
scans and zero secondary verification reads for both node and edge intersections.
These counters exclude projection property reads. Global visited-row and property
decode counters are not exposed by the stock HTTP image, so the image measurements
above do not claim those counters. The plan's estimated work vectors are separate
evidence, not substitutes for measured I/O. Complete production work counters and
the exact affected request remain a follow-up diagnostic requirement.

## Verification and reproduction

- 1,256 planner unit tests and 200 planner doctests pass.
- Empty/populated statistics, literal/bound predicates, nested intersections,
  zero/rare estimates, node/edge symmetry, and a cheaper small-label control pass.
- A real DB test creates two tenant catalogs, retains a prepared read snapshot,
  changes deletion-state memberships, checks every original match in that
  snapshot, then checks that a fresh read is empty. All 32 query-service tests pass.
- 45 DB production-contract tests and the independent graph semantic oracle pass.
- 22 Python image/benchmark tests pass.
- Workspace Clippy with `-D warnings`, planner all-target Clippy, formatting and
  diff whitespace checks pass on stable Rust 1.97.1.
- LLVM coverage: shared physical set contracts 132/132 lines; shared access leaves
  142/142 lines; cost formulas 251/255 lines; executable costing 387/440 lines.
  New executable bitmap/node/edge paths are exercised by a dedicated lowering test.
- The final image passes packaging checks, memory, native volume, restart,
  concurrent membership, MinIO, and persisted Compose replacement smoke checks.

Local commands (use stable Rust 1.97.1):

```sh
cargo test -p helix-planner
cargo test -p db --lib query_service
cargo test -p db --test production_contracts
cargo test -p helix-db-testkit --test planner_semantic_oracle
cargo llvm-cov -p helix-planner --lib
cargo clippy --workspace -- -D warnings
cargo clippy -p helix-planner --all-targets -- -D warnings
cargo fmt --all -- --check
docker-image/test.sh --platform linux/arm64 --image helixdb:hel-850-final
```

For paired images, build the baseline revision separately, run it on localhost
18250 and the candidate on 18251, then use disposable empty databases:

```sh
python3 docker-image/tests/equality_benchmark.py --samples 50
python3 docker-image/tests/equality_benchmark.py --no-seed --ordered --samples 100
python3 docker-image/tests/equality_benchmark.py --no-seed --churn --workers 4 --samples 30
```

Production acceptance still requires the exact request/catalog/image identity,
explicitly authorized deployment, and follow-up p50/p95/max and work measurements.
This change has not been deployed to a live tenant.

## Customer follow-up: covered_workloads

The customer's ab1–ab4 probes report that `tenant+type`, `type+deleted`, and
`deleted` alone remain fast, while combining all three takes 2.1 seconds with
`unbounded_scan`. This agrees with the three-equality reproduction above.

The additional `OR(type=...) AND tenant` source exposed a second issue in the
initial fix. The access extractor distributed shared predicates into each OR
branch, producing `(tenant AND type=A) OR (tenant AND type=B)`. Serial costing
then correctly charged the repeated tenant lookup: 20,320 estimated microseconds
for indexed access versus 18,050 for label scan plus filter. Both candidates
survived exploration; the scan won even with the corrected scan costs.

The extension preserves `tenant AND (type=A OR type=B)` explicitly. A typed plan
variant requires a nonempty shared conjunction and at least two nonempty union
branches; the existing union branch limit still applies. Shared lookup work is
performed once, and the same-property type union can use the existing batch read.
The selected access costs 6,020 microseconds: 5,060 for tenant, 920 for the batched
type union, 30 membership work and 10 final row construction. The scan alternative
stays at 18,050. Measured latency is recorded separately below.

The reproducer is committed in `6231944f` and fails before this extension. Tests
cover 2, 3 and 8 type alternatives, constants and bound parameters, both conjunct
orders, node and edge paths, and complete DB results from two tenant catalogs
with an absent OR branch. Missing-index diagnostics include both the shared and
branch properties. Unsupported predicates and branch limits retain safe fallbacks.

The previous ordered-read fix is retained. No changes were made to the executor,
reverse traversal calibration, ordered range rules, dynamic limits, tie handling,
write path or API-key handling.

Local validation after the extension passes 1,262 planner tests, 200 doctests,
all 32 query-service tests, 23 Python tests, workspace and planner all-target
Clippy with warnings denied, and formatting. Coverage includes missing shared
and branch indexes, retained residual filters, absent shared conjuncts, branch
limits and unsupported shared predicates.
