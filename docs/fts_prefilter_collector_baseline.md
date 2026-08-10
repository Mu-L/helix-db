# FTS prefilter collector baseline

Captured from commit `48b55ba157fc06ad3bc7cfc73c9f1981735ef6f9` on a 100,000-document,
10-split fixture. Each case used three independent runs, three warmups, and 25 measured
samples for rare, medium, and common terms, `k = 10` and `k = 100`, before and after
compaction.

## Collector-only decision

The generic collector won 30 of 36 comparisons in each of the 1, 10, and 25 candidate
buckets. Those losses were compacted common-term cases, and their largest absolute p95
penalty was 0.494 ms. It won 36 of 36 at 50 candidates and 35 of 36 at 100 candidates; the
single 100-candidate loss was 1.178 ms. The implementation therefore uses the generic
collector for every restricted search. This removes term-set query construction, adaptive
strategy selection, and the entity-ID postings warmup. The small absolute losses were
accepted in exchange for one exact, easier-to-maintain path.

| candidates | comparisons | won every comparison | worst p95 advantage |
| ---: | ---: | :---: | ---: |
| 1 | 36 | no | -135.28% |
| 10 | 36 | no | -63.30% |
| 25 | 36 | no | -8.93% |
| 50 | 36 | yes | 7.33% |
| 100 | 36 | no | -50.48% |

The table records the original removal gate; it is retained as historical evidence, not as
the production selection rule. Every measured result passed the exhaustive filtered BM25
oracle assertion. Result digests matched across term-set and collector strategies for all
180 equivalent case/run groups.

## Optimization baselines

- At 1% density with the common query, collector p95 advantage over term-set ranged from
  -9.25% to 96.19% across required cases and runs.
- At 100% density, the worst collector/unrestricted p95 ratio was 1.349; 13 of 36 required
  cases exceeded 1.1x.
- The worst 100%-density collector/unrestricted peak-allocation ratio was 1.075; all cases
  passed the 2x allocation gate.
- The collector scans matching BM25 documents before checking bitmap membership. Common-term
  cost can therefore grow with corpus size; a production-format 1M-document benchmark should
  be used to set an absolute latency SLO before revisiting specialization.

## One-million-candidate smoke

The separate release-mode smoke used one million documents, ten initial splits, the common
term, `k = 100`, and a full-density candidate bitmap. Collector results exactly matched the
unrestricted digest before and after compaction. Peak measured allocation was 1.90 MB across
ten splits and 0.40 MB after compaction, both below the smoke's 64 MiB absolute bound. The
collector took 47.58 ms across ten splits and 42.47 ms after compaction. This confirms bounded
execution at the candidate cap, while also recording the expected dense common-term latency
tradeoff; it does not replace the 100k relative-allocation gate.

Raw evidence:

- `fts_prefilter_tiny_baseline.json`
- `fts_prefilter_sparse_baseline.json`
- `fts_prefilter_dense_baseline.json`
- `fts_prefilter_million_smoke.json`
