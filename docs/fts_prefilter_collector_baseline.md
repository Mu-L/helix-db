# FTS prefilter collector baseline

Captured from commit `48b55ba157fc06ad3bc7cfc73c9f1981735ef6f9` on a 100,000-document,
10-split fixture. Each case used three independent runs, three warmups, and 25 measured
samples for rare, medium, and common terms, `k = 10` and `k = 100`, before and after
compaction.

## Term-set removal decision

The generic collector did not win every comparison in every tiny bucket. The production
term-set path must remain until the specialized collector passes this gate.

| candidates | comparisons | won every comparison | worst p95 advantage |
| ---: | ---: | :---: | ---: |
| 1 | 36 | no | -135.28% |
| 10 | 36 | no | -63.30% |
| 25 | 36 | no | -8.93% |
| 50 | 36 | yes | 7.33% |
| 100 | 36 | no | -50.48% |

Every measured result passed the exhaustive filtered BM25 oracle assertion. Result digests
matched across term-set and collector strategies for all 180 equivalent case/run groups.

## Optimization baselines

- At 1% density with the common query, collector p95 advantage over term-set ranged from
  -9.25% to 96.19% across required cases and runs.
- At 100% density, the worst collector/unrestricted p95 ratio was 1.349; 13 of 36 required
  cases exceeded 1.1x.
- The worst 100%-density collector/unrestricted peak-allocation ratio was 1.075; all cases
  passed the 2x allocation gate.

Raw evidence:

- `fts_prefilter_tiny_baseline.json`
- `fts_prefilter_sparse_baseline.json`
- `fts_prefilter_dense_baseline.json`
