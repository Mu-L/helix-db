# Planner and stateful workload fuzzing

These Cargo Fuzz targets exercise separate semantic boundaries:

- `query_json`: public `QueryRequest` JSON parsing and round-trip stability.
- `planner_context_ast`: arbitrary serialized AST/context pairs plus the finite
  normalized planner domain.
- `planner_interpreter`: public planner-to-interpreter execution against one
  process-local empty database.
- `stateful_action_trace`: replay-trace parsing, lifecycle validation, and
  serialization stability.

Run a target with, for example:

```bash
cargo fuzz run --manifest-path crates/db-testkit/fuzz/Cargo.toml query_json
```

Persist every minimized failure beneath the matching `corpus/` directory and
keep it as a deterministic regression seed.
