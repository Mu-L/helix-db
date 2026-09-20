# Idle vector-cache refresh validation

The refresh guard retains a published cache only when its snapshot sequence,
committed dirty generation, and admission budget match. Empty and budget-limited
stores retain the same checks. No persisted data format or public API changed.

- `cargo test --workspace`: 3,972 tests and doc tests passed; 12 existing tests ignored.
- Workspace Clippy, production-contract Clippy with the production coverage feature
  set, formatting, and 24 Docker tooling tests passed.
- The counted-SST contract verifies unchanged store identity and no additional
  vector-data reads across three refreshes, including empty and bounded stores.
- The ARM64 server image passed the full packaging/runtime suite. The final MinIO
  regression uses a metadata-only query to identify catalog ranges, then observes
  17 seconds without queries: six catalog GETs and zero vector-data GETs. Search
  results remain correct before and after a write.
- The same regression fails against a rebuilt image from unmodified `main`
  (`2eaa844a96046808b887914679fbe4a49b7adc05`), detecting repeated non-catalog ranges.

Catalog polling remains enabled. Unrelated committed writes can still trigger
rehydration because cache reads require an exact snapshot sequence.
