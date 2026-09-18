# Unique equality membership regression

The serial cost of a unique equality union exceeds the default label-scan cost
at four values. Production has no label cardinality statistics, so selective
membership queries scan large labels.

## Scope and contract

- Lower same-index, finite, indexed unique literals into one typed owner batch.
- Resolve the active generation and tenant scope from the request snapshot.
- Read owner keys with `multi_get`; verify every returned owner against the
  authoritative graph in that same snapshot. Missing owners are misses; stale
  or malformed owners fail closed. Do not alter persisted formats or writes.
- Charge batch reads and authoritative verification in both cost paths.
- Preserve null/NaN classification, numeric equality, set deduplication, range
  ordering, and configured union limits. Keep non-unique batching unchanged.

## Validation

1. Reproduce the failing four-value plan before implementation.
2. Test empty/populated statistics, literal/bound parameters, duplicate values,
   nested labels, union bounds, and small labels where a scan is appropriate.
3. Test batch results, missing/corrupt owners, snapshot/tenant boundaries,
   canonical numeric equality, and cancellation using local fixtures.
4. Run workspace tests, doc tests, coverage, formatting, and strict Clippy.
5. Build and test the full local image, compare synthetic membership queries
   with the released image, and inspect plans as well as timings.

No production access, push, deployment, or image publication is part of this work.
