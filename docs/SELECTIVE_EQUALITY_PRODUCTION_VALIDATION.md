# HEL-850 production evidence and exact-query replay

## Recovered source and rollout identity

The hyperscale checkout was fetched and fast-forwarded to `dev` revision
`26c64acaf921a1b938d983db86a3d5668074c0ad` before production inspection.
Read-only CloudWatch inspection located the customer cluster's application log
group in `us-east-1`. The approved insights reads then recovered nine canonical
query ASTs and their planner diagnostics from narrow intervals within
2026-09-09 12:20–13:20 UTC. Credentials stayed in process memory and no production
writes or query replays were performed.

The rollout record was created at **12:44:00.805175 UTC** and completed at
**12:51:43.017738 UTC**. Its successful result records requested and observed
hyperscale image identity:

```text
sha256:70b49b435a0960330bbd6097f2c57571b77c8c448ec5ded4c02903ec75309e8b
```

ECR identifies its build tag as the same hyperscale revision `26c64aca`; that
revision's Cargo manifest and lockfile pin the engine to
`57877a76b307b6838ebd950c5f05dab4b7767d7e`. This connects observed rollout
identity to source, rather than inferring deployment from a repository pin.
CloudWatch still contains old-container messages, including the old writer;
the operator's observed-image success record is the rollout evidence.

## Production query and planner evidence

The four probes match the customer's report:

| Query | CloudWatch latency ms | Equality lookups | Label scans | SlateDB block-cache hits / misses |
| --- | ---: | ---: | ---: | ---: |
| `ab1_tenant_type` | 5.629 | 2 | 0 | 147 / 0 |
| `ab2_tenant_type_deleted` | 2140.314 | 0 | 1 | 143547 / 0 |
| `ab3_type_deleted` | 3.076 | 2 | 0 | 31 / 0 |
| `ab4_deleted_only` | 5.227 | 1 | 0 | 176 / 0 |

The three-equality probe and `find_deleted_resource_ids_by_type` have identical
ASTs: parameterized tenant/type/deleted equalities, a parameterized limit, and
`value_map(["id"])`. Their diagnostics identify one unbounded `Resource` label
scan with those three residual properties. The simpler probes demonstrate the
availability of all three equality indexes in the actual execution scope.

The full `covered_workloads` AST is identical in the recovered before/after
calls. It contains five type alternatives AND tenant, binds the workload,
traverses `HAS_VULNERABILITY`, filters target tenant and severity, and returns
distinct workload IDs. At 12:29:29 it used ten equality lookups, five
intersections and a union; it recorded 1,957 block-cache hits. At 13:14:03 it
used one label scan, zero equality lookups and 349,023 block-cache hits.
CloudWatch latency changed from 246.399 ms to 8,092.157 ms for these two calls.
Cache hits are observed work evidence, not counts of unique rows or decoded
properties. Both selected calls recorded zero block-cache misses.

The full `service_workload_map` traverses `ROUTES_TO`, incoming `MANAGES`, and
conditionally incoming `CREATES`; it projects distinct bound service/workload
properties. Both ordered queries use a named `candidates` binding followed by
injection, timestamp filtering, descending ordering, limit, and projection.
Those full ASTs are preserved in
[`hel850-production-queries.json`](../docker-image/tests/fixtures/hel850-production-queries.json).
Only synthetic parameter values are included. The metrics protocol stores the
AST separately from runtime parameters; it cannot recover those original values
or an exhaustive tenant catalog snapshot.

## Exact AST plan comparison

[`selective_equality_production_plans.json`](selective_equality_production_plans.json)
contains executable operators and estimated cost vectors for all nine ASTs,
using empty statistics and active Resource tenant/type/deleted equality indexes.
Baseline source is `cb893050`; its planner and query-service source are identical
to the verified deployed engine pin `57877a76`. Fixed production source is
`34f32c03`. The isolated
#1081 trigger and complete competing-candidate costs remain documented in
[the costing report](SELECTIVE_EQUALITY_COSTING_VALIDATION.md).

The deleted query changes from label scan + residual to a three-bitmap
intersection. The five-alternative covered query changes from label scan to
one tenant lookup intersected with the existing batched five-type lookup.
`service_workload_map`, both ordered queries and the three simpler probes
retain identical executable operators after excluding cost annotations.

The integration test covers every AST with limits 0, 1 and 1,000, asserting no
unbounded scans or guardrail stops, expected equality lookups, and retained
traversals. Separate Cargo build artifacts, or explicitly cleaning the affected
planner package, are necessary when comparing archived source revisions:
archive timestamps can otherwise make a shared target directory reuse the
wrong revision. Both archived baseline plan capture and image compilation were
verified after cleaning the affected artifacts.

## Paired built-image validation

Both disposable localhost images use the canonical build, stable Rust 1.97.1,
`linux/arm64`, and identical in-memory data: **10,700 nodes**, 4 KiB raw data per
node, and **900 edges**. Fifty service subgraphs include repeated paths,
foreign-tenant sources/intermediates/targets, direct workloads, replica-set
parents, multiple matching vulnerabilities, and nonmatching severity controls.
The oracle checks every projected result, distinctness, tenant filters, cutoff,
limit and ordered output. Every response passed.

There are 100 measured calls per case/image, three warmups, alternating image
order, and no concurrent local compilation or writers during timing.

| Query | Result rows | Baseline p50 / p95 ms | Candidate p50 / p95 ms |
| --- | ---: | ---: | ---: |
| deleted-resource lookup | 3 | 46.609 / 48.672 | 1.816 / 2.317 |
| covered workloads, full traversal | 100 | 57.919 / 60.637 | 3.753 / 4.283 |
| service workload map, full traversal | 150 | 8.811 / 9.573 | 8.809 / 9.658 |
| resource read, broad | 1000 | 22.149 / 23.343 | 22.175 / 23.204 |
| resource read, narrow | 14 | 8.825 / 9.433 | 8.741 / 9.235 |
| dedup read, broad | 1000 | 19.637 / 20.979 | 19.812 / 20.854 |
| dedup read, narrow | 14 | 8.670 / 9.084 | 8.598 / 9.176 |

Full results, including every probe, maximum latency and image identities, are
in [`selective_equality_production_benchmark_results.json`](selective_equality_production_benchmark_results.json).
The prior gains are preserved within approximately 1% median variation in these
full-query samples. These are local measurements with synthetic values and
storage, not claims of measured post-fix production latency.

Reproduction:

```sh
cargo test -p helix-planner --test production_equality
python3 -m unittest discover -s docker-image/tests -p 'test_*equality_benchmark.py'
python3 docker-image/tests/production_equality_benchmark.py --samples 100
```

The original 1,262 planner unit tests, 200 doctests, tenant/snapshot/churn tests,
ordered-storage suite and image smoke validation remain applicable. The added
planner integration test and complete image replay pass; all 26 Python tests and
planner all-target Clippy pass. The coverage-gate line-position correction passes
the server production coverage script with the original fingerprint and
thresholds unchanged. The fix has not been deployed to production.
