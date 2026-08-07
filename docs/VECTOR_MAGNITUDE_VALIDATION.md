# Vector component magnitude validation

Current vector indexes retain their existing `f32` payloads and score
semantics. Euclidean and Manhattan vectors are accepted only when every
component's absolute value is at or below the metric- and dimension-specific
inclusive maximum:

```text
Euclidean = sqrt(f32::MAX / (8 × dimension))
Manhattan = f32::MAX / (4 × dimension)
```

The calculation uses checked `f64` arithmetic and rounds downward when the
nearest representable `f32` would exceed the exact result. A component exactly
equal to the resulting `f32` maximum is valid; the next greater representable
value is invalid. The factors leave 2× score headroom for worst-case
opposite-sign pairs. Cosine has no component-magnitude limit and retains its
existing finite-input, nonzero-norm policy.

Validation order is dimension, component finiteness, cosine zero norm, then
component magnitude. Requests fail with
`HelixDbError::VectorComponentMagnitudeExceeded`, including the metric,
dimension, component index, observed absolute magnitude, and inclusive maximum.
Invalid persisted current rows fail closed as
`VectorItemDecodeError::ComponentMagnitudeExceeded`. Authoritative build input
blocks the operation as `InvalidSourceData` with the exact entity identity;
invalid legacy physical data blocks adoption as `InvalidLegacyPhysical`.

This is validation-only. It changes no key, value, metadata, descriptor, score,
or `$distance` encoding, adds no configuration, and requires no migration.
Existing compliant indexes reopen unchanged.

For a blocked build, correct the authoritative graph property and retry the
operation. For an invalid active physical row, first correct the authoritative
graph property, then explicitly drop and recreate the index. Source correction
does not rewrite the invalid physical row, and the database never starts an
automatic rebuild or in-place row repair.
