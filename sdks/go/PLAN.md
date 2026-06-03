# HelixDB Go SDK Implementation Plan

## Goal

Build a Go SDK for HelixDB with complete wire-format and runtime parity with the existing Rust and TypeScript SDKs, while exposing an API that feels natural to Go developers.

The Go SDK module path is:

```text
github.com/helixdb/helix-db/sdks/go
```

The package name should be:

```go
package helix
```

## Scope

The Go SDK must include:

- Query DSL builders for read and write batches.
- Exact JSON AST serialization parity with Rust serde output and TypeScript custom serialization.
- Dynamic `/v1/query` request generation.
- Stored query bundle generation.
- Parameter schema and conversion support.
- HTTP client support for dynamic and stored query execution.
- Unit tests covering Go API behavior and JSON shape.
- Full integration into the existing Rust/TypeScript SDK parity suite.
- Documentation and examples.

## Current Repository Context

- `sdks/go/` exists but is empty.
- Rust SDK source of truth:
  - `sdks/rust/src/dsl.rs`
  - `sdks/rust/src/lib.rs`
  - `sdks/rust/src/query_generator.rs`
  - `sdks/rust/examples/generate_parity_fixtures.rs`
- TypeScript SDK source of truth:
  - `sdks/typescript/src/dsl.ts`
  - `sdks/typescript/src/index.ts`
  - `sdks/typescript/scripts/parity/generate-fixtures.ts`
  - `sdks/typescript/scripts/parity/compare-json.ts`
  - `sdks/typescript/scripts/parity/run-helix.ts`
- Existing parity suite currently compares Rust and TypeScript only.
- Current parity fixture counts:
  - Runtime fixtures: 224
  - JSON-only fixtures: 8

## Design Principles

1. Preserve wire compatibility above all else.
   - JSON enum tags, field names, omitted fields, explicit null fields, and request payload shapes must match Rust and TypeScript.

2. Make the Go API idiomatic.
   - Prefer explicit constructors, fluent pointer/value builders, `context.Context`, `error`, and `io`/`net/http` conventions.
   - Do not attempt to fully port Rust typestate into Go unless it improves usability.

3. Keep dependencies minimal.
   - Prefer the Go standard library.
   - Add third-party dependencies only for a clear need.

4. Make serialization deterministic.
   - Go map iteration is randomized, so any output containing maps must use sorted-key custom encoding where deterministic fixture parity matters.

5. Use Rust and TypeScript as parity references.
   - Rust defines the serde wire shape.
   - TypeScript demonstrates explicit registration and dynamic query ergonomics that translate better to Go than Rust macros.

## Proposed Package Layout

```text
sdks/go/
  go.mod
  README.md
  PLAN.md
  values.go
  refs.go
  expr.go
  predicate.go
  projection.go
  index.go
  step.go
  traversal.go
  batch.go
  params.go
  dynamic.go
  bundle.go
  client.go
  errors.go
  json.go
  values_test.go
  dsl_test.go
  dynamic_test.go
  bundle_test.go
  client_test.go
  cmd/
    generate-parity-fixtures/
      main.go
```

## Public API Shape

The SDK should feel natural in Go while staying close enough to Rust/TypeScript that users can translate examples easily.

Example read query:

```go
req := helix.DynamicRead(
    helix.Read().
        VarAs("users",
            helix.G().
                NWithLabel("User").
                Where(helix.PredEq("status", "active")).
                Limit(25).
                ValueMap("$id", "name", "status"),
        ).
        Returning("users"),
)
```

Example write query:

```go
req := helix.DynamicWrite(
    helix.Write().
        VarAs("alice",
            helix.G().AddN("User", helix.Props{
                helix.Prop("name", "Alice"),
                helix.Prop("tier", "pro"),
            }),
        ).
        Returning("alice"),
)
```

Example client usage:

```go
client, err := helix.NewClient("http://localhost:6969")
if err != nil {
    return err
}

var out map[string]any
err = client.Query().
    Dynamic(req).
    Send(ctx, &out)
```

## Module And Package Setup

Add `sdks/go/go.mod`:

```go
module github.com/helixdb/helix-db/sdks/go

go 1.22
```

Initial package should be `helix`.

Use `go test ./...` as the primary local validation command.

## JSON Encoding Requirements

Implement custom `MarshalJSON` where needed to match Rust serde output.

Required enum shapes:

- Unit enum:
  ```json
  "Count"
  ```

- Newtype enum:
  ```json
  { "N": { "Var": "user" } }
  ```

- Tuple enum:
  ```json
  { "Eq": ["name", { "String": "Alice" }] }
  ```

- Struct variant:
  ```json
  { "CreateIndex": { "spec": { "NodeVector": { "label": "Doc", "property": "embedding" } }, "if_not_exists": true } }
  ```

Important null/omit rules:

- `DynamicQueryRequest.query_name` must always serialize.
- Unnamed dynamic requests must emit `"query_name": null`.
- `NamedQuery.condition` must serialize as `null` when absent.
- `parameters` and `parameter_types` must be omitted when absent.
- Optional tenant fields must be omitted when absent.
- `ValueMap(nil)` must serialize as `{ "ValueMap": null }`.

Large integer handling:

- `int64` values must serialize as JSON numbers, not strings.
- Avoid converting large integers to `float64`.
- Client response decoding into `any` should use `json.Decoder.UseNumber()`.

Deterministic encoding:

- Query bundle route maps must serialize with sorted keys.
- Dynamic parameter maps should serialize with sorted keys for stable fixture output.
- Object property values should use sorted keys when represented as maps.
- Property pairs in `AddN`/`AddE` must preserve caller-provided order by using slices, not maps.

## Core Data Types

Implement parity for these Rust/TypeScript concepts.

### Property Values

Go types/builders should support:

- `Null`
- `Bool`
- `I64`
- `DateTime` as UTC epoch milliseconds in stored property values
- `F64`
- `F32`
- `String`
- `Bytes`
- `I64Array`
- `F64Array`
- `F32Array`
- `StringArray`
- Heterogeneous `Array`
- `Object`

Suggested API:

```go
helix.Null()
helix.Bool(true)
helix.I64(42)
helix.DateTimeMillis(1776000000000)
helix.F64(1.5)
helix.F32(1.25)
helix.String("Alice")
helix.Bytes([]byte{1, 2})
helix.I64Array(1, 2, 3)
helix.F64Array(1.5, 2.5)
helix.F32Array(1.0, 0.0)
helix.StringArray("a", "b")
helix.Array(helix.String("a"), helix.I64(7))
helix.Object(map[string]helix.PropertyValue{...})
```

Also support ergonomic conversion from common Go types where safe:

- `string` to `String`
- `bool` to `Bool`
- signed integer types to `I64`
- `float64` to `F64`
- `float32` to `F32`
- `[]byte` to `Bytes`
- `[]string` to `StringArray`
- `[]int64` to `I64Array`
- `[]float64` to `F64Array`
- `[]float32` to `F32Array`

Avoid ambiguous conversions that could hide precision loss.

### DateTime

Implement a `DateTime` type storing UTC epoch milliseconds.

Required methods:

- `DateTimeFromMillis(millis int64) DateTime`
- `ParseDateTimeRFC3339(input string) (DateTime, error)`
- `Millis() int64`
- `RFC3339() (string, error)`

Stored property serialization:

```json
{ "DateTime": 1776000000000 }
```

Dynamic parameter serialization:

```json
"2026-04-05T10:34:56.789Z"
```

### PropertyInput

Represent mutation/search inputs as either:

- `Value(PropertyValue)`
- `Expr(Expr)`

Required helper:

```go
helix.ParamInput("name")
```

Expected JSON:

```json
{ "Expr": { "Param": "name" } }
```

### NodeRef And EdgeRef

Node refs:

- `All`
- `Ids([]uint64)`
- `Var(string)`
- `Param(string)`

Edge refs:

- `Ids([]uint64)`
- `Var(string)`
- `Param(string)`

Suggested API:

```go
helix.AllNodes()
helix.NodeID(1)
helix.NodeIDs(1, 2)
helix.NodeVar("users")
helix.NodeParam("node_ids")
helix.EdgeID(1)
helix.EdgeIDs(1, 2)
helix.EdgeVar("edges")
helix.EdgeParam("edge_ids")
```

## Expressions

Support:

- Property reference
- ID
- Timestamp
- DateTimeNow
- Constant
- Param
- Add
- Sub
- Mul
- Div
- Mod
- Neg
- Case

Suggested API:

```go
helix.ExprProp("score")
helix.ExprID()
helix.ExprTimestamp()
helix.ExprDateTime()
helix.ExprVal(1)
helix.ExprParam("limit")
helix.ExprProp("score").Add(helix.ExprVal(1))
helix.ExprCase(branches, elseExpr)
```

## Predicates

Support normal predicates and source predicates.

Normal predicate variants:

- `Eq`, `Neq`, `Gt`, `Gte`, `Lt`, `Lte`, `Between`
- `EqExpr`, `NeqExpr`, `GtExpr`, `GteExpr`, `LtExpr`, `LteExpr`, `BetweenExpr`
- `HasKey`
- `IsNull`
- `IsNotNull`
- `StartsWith`
- `EndsWith`
- `Contains`
- `ContainsExpr`
- `IsIn`
- `IsInExpr`
- `And`
- `Or`
- `Not`
- `Compare`

Source predicate variants:

- `Eq`, `Neq`, `Gt`, `Gte`, `Lt`, `Lte`, `Between`
- `HasKey`
- `StartsWith`
- `And`
- `Or`
- expression variants for comparisons and between

Critical parity rule:

- Literal inputs must emit literal variants like `Eq`.
- `Expr` or parameter inputs must emit expression variants like `EqExpr`.
- If either `Between` bound is an expression, both bounds must emit under `BetweenExpr`, with literal bounds promoted to `Expr.Constant`.

Example:

```json
{ "BetweenExpr": ["age", { "Param": "min_age" }, { "Constant": { "I64": 65 } }] }
```

## Projections

Support:

- Property projection: `{ "source": "name", "alias": "display_name" }`
- Expression projection: `{ "alias": "age2", "expr": { ... } }`

Suggested API:

```go
helix.ProjectProp("name")
helix.ProjectPropAs("name", "display_name")
helix.ProjectExpr("age2", helix.ExprProp("age").Add(helix.ExprVal(1)))
```

## Index Specs

Support:

- Node equality
- Unique node equality
- Node range
- Edge equality
- Edge range
- Node vector
- Node text
- Edge vector
- Edge text

Optional tenant property fields must be omitted when unset.

Suggested API:

```go
helix.NodeEqualityIndex("User", "email")
helix.NodeUniqueEqualityIndex("User", "email")
helix.NodeRangeIndex("User", "createdAt")
helix.EdgeEqualityIndex("FOLLOWS", "weight")
helix.NodeVectorIndex("Doc", "embedding", "tenantId")
helix.NodeTextIndex("Doc", "body", "tenantId")
```

## Steps

Implement all current step variants from Rust `Step`.

Source steps:

- `N`
- `NWhere`
- `E`
- `EWhere`
- `VectorSearchNodes`
- `TextSearchNodes`
- `VectorSearchEdges`
- `TextSearchEdges`

Navigation steps:

- `Out`
- `In`
- `Both`
- `OutE`
- `InE`
- `BothE`
- `OutN`
- `InN`
- `OtherN`

Filter steps:

- `Has`
- `HasLabel`
- `HasKey`
- `Where`
- `Dedup`
- `Within`
- `Without`
- `EdgeHas`
- `EdgeHasLabel`

Bound steps:

- `Limit`
- `LimitBy`
- `Skip`
- `SkipBy`
- `Range`
- `RangeBy`

Variable steps:

- `As`
- `Store`
- `Select`
- `Inject`

Terminal steps:

- `Count`
- `Exists`
- `Id`
- `Label`
- `Values`
- `ValueMap`
- `Project`
- `EdgeProperties`

Index steps:

- `CreateIndex`
- `DropIndex`
- legacy vector/text index step variants if still present in Rust AST

Mutation steps:

- `AddN`
- `AddE`
- `SetProperty`
- `RemoveProperty`
- `Drop`
- `DropEdge`
- `DropEdgeLabeled`
- `DropEdgeById`

Control and aggregation steps:

- `OrderBy`
- `OrderByMultiple`
- `Repeat`
- `Union`
- `Choose`
- `Coalesce`
- `Optional`
- `Group`
- `GroupCount`
- `AggregateBy`

Reserved/no-op steps:

- `Fold`
- `Unfold`
- `Path`
- `SimplePath`
- `WithSack`
- `SackSet`
- `SackAdd`
- `SackGet`

## Traversal API

Use a fluent traversal builder.

Entry points:

```go
helix.G()
helix.Sub()
```

Traversal should track internally:

- Steps
- Whether it contains mutation steps
- Whether it is terminal, if needed for validation

Read batches must reject mutation traversals at runtime with a clear error or panic-free validation API. Preferred Go approach:

- Provide `VarAs(name string, traversal Traversal) *ReadBatch` for convenience.
- Also provide `Err() error` or make batch construction return errors where invalid read/write separation is possible.

Because fluent builders returning errors at every step are awkward in Go, a practical design is:

- Builders collect errors internally.
- Serialization or `Validate()` returns the first error.
- Tests verify invalid read batches fail before request generation.

## Batch API

Support:

- Read batch
- Write batch
- `VarAs`
- `VarAsIf`
- `ForEachParam`
- `Returning`

Batch entries must serialize as externally tagged variants:

```json
{ "Query": { "name": "users", "steps": [...], "condition": null } }
```

```json
{ "ForEach": { "param": "items", "body": [...] } }
```

## Dynamic Requests

Implement:

- `DynamicRead(batch *ReadBatch) *DynamicQueryRequest`
- `DynamicWrite(batch *WriteBatch) *DynamicQueryRequest`
- `SetQueryName(name string)`
- `ClearQueryName()`
- `WithQueryName(name string)`
- `InsertParameterValue(name string, value DynamicValue)`
- `InsertParameterType(name string, ty QueryParamType)`
- `JSON() ([]byte, error)`
- `JSONString() (string, error)`

Request shape:

```json
{
  "request_type": "read",
  "query_name": null,
  "query": {
    "queries": [],
    "returns": []
  }
}
```

With parameters:

```json
{
  "request_type": "read",
  "query_name": "find_users",
  "query": { "queries": [], "returns": [] },
  "parameters": { "limit": 25 },
  "parameter_types": { "limit": "I64" }
}
```

## Parameter Schemas

Support query parameter types:

- `Bool`
- `I64`
- `F64`
- `F32`
- `String`
- `DateTime`
- `Bytes`
- `Value`
- `Object`
- `Array(inner)`

Wire shape examples:

```json
"String"
```

```json
{ "Array": "Object" }
```

Dynamic conversion rules:

- `DateTime` becomes RFC3339 UTC string with millisecond precision.
- `Bytes` must return an unsupported bytes error for dynamic JSON.
- `Value` converts `PropertyValue` into untagged JSON-compatible dynamic values.
- Object schemas should validate and convert nested values where possible.
- Unknown parameters should be rejected.
- Missing required parameters should be rejected.

## Query Registration And Bundles

Rust uses `#[register]`; Go should use explicit registration similar to TypeScript.

Suggested API:

```go
params := helix.DefineParams(
    helix.Param("tenant_id", helix.ParamString()),
    helix.Param("limit", helix.ParamI64()),
)

queries, err := helix.DefineQueries(helix.QueryDefinitions{
    Read: map[string]helix.RegisteredReadQuery{
        "find_users": helix.RegisterRead(func(p helix.Params) *helix.ReadBatch {
            return helix.Read().
                VarAs("users",
                    helix.G().
                        NWithLabel("User").
                        Where(helix.PredEq("tenantId", p.Expr("tenant_id"))).
                        Limit(p.Expr("limit")).
                        ValueMap("$id", "name"),
                ).
                Returning("users")
        }, params),
    },
})
```

Bundle shape:

```json
{
  "version": 4,
  "read_routes": {},
  "write_routes": {},
  "read_parameters": {},
  "write_parameters": {}
}
```

Required behavior:

- `QUERY_BUNDLE_VERSION = 4`
- Duplicate route names across read/write must fail.
- Bundle route maps must serialize with sorted keys.
- Bundle deserialization must reject unsupported versions.
- Generate `queries.json` by default.

Implement:

- `BuildQueryBundle`
- `SerializeQueryBundle`
- `DeserializeQueryBundle`
- `WriteQueryBundleToPath`
- `ReadQueryBundleFromPath`
- `Generate`
- `GenerateToPath`

## HTTP Client

Mirror Rust/TypeScript behavior.

Client construction:

```go
client, err := helix.NewClient("")
client, err := helix.NewClient("https://cluster.helix-db.com")
client = client.WithAPIKey("hx_secret")
```

Default base URL:

```text
http://localhost:6969
```

Routes:

- Dynamic query: `POST /v1/query`
- Stored query: `POST /v1/query/{name}`

Headers:

- Always set `Content-Type: application/json`.
- `WriterOnly()` sets `x-helix-require-writer: true`.
- `WarmOnly()` sets `x-helix-warm: true`.
- `ShouldAwaitDurability(bool)` sets `x-helix-await-durable: true|false`.
- API key sets `Authorization: Bearer <key>`.

Suggested API:

```go
err := client.Query().
    WriterOnly().
    WarmOnly().
    Dynamic(req).
    Send(ctx, &out)
```

```go
err := client.Query().
    ShouldAwaitDurability(false).
    Body(map[string]any{"name": "Alice"}).
    Stored("add_user").
    Send(ctx, &out)
```

Error behavior:

- Only HTTP 200 succeeds.
- Non-200 response returns remote error with response body as details.
- Invalid URL returns invalid URL error.
- Transport failure returns network error.
- Request/response JSON failure returns serialization error.

Typed errors:

```go
type ErrorKind string

const (
    ErrorNetwork       ErrorKind = "Network"
    ErrorRemote        ErrorKind = "Remote"
    ErrorSerialization ErrorKind = "Serialization"
    ErrorInvalidURL    ErrorKind = "InvalidUrl"
)
```

## Unit Test Plan

Port the existing TypeScript unit coverage into Go.

### Value And JSON Tests

Cover:

- All `PropertyValue` variants.
- `DateTime` parsing and formatting.
- `PropertyInput` value and param expression shapes.
- `NodeRef` and `EdgeRef` shapes.
- Large `int64` JSON numbers.
- Rejection of unsupported dynamic bytes parameters.

### Predicate And Expression Tests

Cover:

- Arithmetic expression serialization.
- `Case` expression serialization.
- Literal predicate variants.
- Expression predicate variants.
- Source predicate conversion to normal predicate.
- `BetweenExpr` literal-promotion behavior.

### Batch And Traversal Tests

Cover:

- Basic read batch JSON.
- Basic write batch JSON.
- Conditional query JSON.
- `ForEachParam` JSON.
- Search/index steps.
- Nested object properties.
- Dotted property paths like `metadata.externalID`.
- Generic edge filters preserving current TypeScript behavior.

### Dynamic Request Tests

Cover:

- `query_name: null` for unnamed requests.
- Named query requests.
- Parameter values and types.
- DateTime dynamic params as RFC3339.
- Omission of absent `parameters` and `parameter_types`.

### Bundle Tests

Cover:

- Bundle version equals 4.
- Read/write routes serialize correctly.
- Parameter metadata serializes correctly.
- Duplicate route names fail.
- Unsupported bundle version fails.
- Sorted deterministic output.

### Client Tests

Use `httptest.Server`.

Cover:

- Default URL behavior.
- Invalid URL behavior.
- Dynamic query route `/v1/query`.
- Stored query route `/v1/query/{name}`.
- API key header.
- Writer/warm/durability headers.
- Non-200 remote error.
- JSON response decoding with `UseNumber()`.

## Parity Fixture Generator

Add:

```text
sdks/go/cmd/generate-parity-fixtures/main.go
```

It must mirror both:

- `sdks/rust/examples/generate_parity_fixtures.rs`
- `sdks/typescript/scripts/parity/generate-fixtures.ts`

Output paths:

```text
sdks/tests/parity/generated/go/runtime
sdks/tests/parity/generated/go/json-only
```

Required fixture set:

- Runtime fixtures `001` through `032`.
- Node permutation fixtures `100` through `291`.
- JSON-only fixtures `900` through `907`.

Fixture names and bucket locations must match Rust and TypeScript exactly.

Generator behavior:

- Accept optional output directory argument.
- Default to `../tests/parity/generated/go` when run from `sdks/go`, or document exact expected working directory.
- Clear and recreate `runtime` and `json-only` output directories.
- Write compact JSON requests.
- Preserve all property pair and step ordering.

## Extend Existing Parity Harness

The existing parity harness is TypeScript-orchestrated. Keep that structure initially.

### Paths

Update `sdks/typescript/scripts/parity/paths.ts`:

```ts
export const goGeneratedRoot = resolve(generatedRoot, "go");
```

### Package Scripts

Update `sdks/typescript/package.json` scripts:

```json
{
  "parity:generate:go": "go run ../go/cmd/generate-parity-fixtures ../tests/parity/generated/go",
  "parity:generate": "npm run parity:generate:rust && npm run parity:generate:ts && npm run parity:generate:go"
}
```

### Structural Comparison

Update `compare-json.ts` to compare Rust, TypeScript, and Go.

Recommended approach:

- Keep Rust as baseline.
- Enforce file-set equality for TypeScript and Go against Rust.
- Compare canonical structural JSON for TypeScript and Go against Rust.
- Report missing, extra, and mismatched fixture paths per language.
- Keep unsafe integer protection through existing `parseJsonStructural`.

Add explicit expected counts:

- Runtime: 224
- JSON-only: 8
- Total: 232

### Runtime Comparison

Update `run-helix.ts` to add Go instance:

```ts
const go: Instance = {
  label: "go",
  generatedRoot: goGeneratedRoot,
  workspace: join(workspacesRoot, "go"),
  results: join(resultsRoot, "go"),
  port: 18082,
};
```

Run all three instances independently:

- Rust on port `18080`
- TypeScript on port `18081`
- Go on port `18082`

Compare results:

- Rust vs TypeScript
- Rust vs Go

Runtime execution must remain sequential per language because fixtures are stateful.

## CI Updates

Update `.github/workflows/parity_tests.yml`.

Add Go setup:

```yaml
- uses: actions/setup-go@v5
  with:
    go-version: '1.22'
```

Add Go test step before full parity:

```yaml
- name: Test Go SDK
  working-directory: sdks/go
  run: go test ./...
```

Keep final parity command:

```yaml
- name: Run parity suite
  run: npm run test:parity
```

The parity suite should now include Go fixture generation and Go runtime comparison.

## Documentation

Add `sdks/go/README.md` with:

- Install/import instructions.
- Query DSL quick start.
- Read batch example.
- Write batch example.
- Dynamic request example.
- Stored query client example.
- Parameter schema example.
- Query bundle generation example.
- Notes about parity with Rust/TypeScript.
- Notes about int64, datetime, bytes dynamic params, and deterministic output.

## Implementation Phases

### Phase 1: Scaffolding

- Add `go.mod`.
- Add package skeleton files.
- Add core error and JSON helper infrastructure.
- Add minimal tests proving module compiles.

Validation:

```sh
cd sdks/go && go test ./...
```

### Phase 2: Core AST And Serialization

- Implement values, refs, expressions, predicates, projections, index specs, and steps.
- Implement custom JSON encoding for all enum shapes.
- Add unit tests for exact JSON output.

Validation:

```sh
cd sdks/go && go test ./...
```

### Phase 3: Traversal And Batch Builders

- Implement `G`, `Sub`, traversal methods, read/write batches, conditions, and foreach.
- Add mutation tracking for read-batch validation.
- Add nested property and dotted path tests.

Validation:

```sh
cd sdks/go && go test ./...
```

### Phase 4: Dynamic Requests And Params

- Implement dynamic request generation.
- Implement parameter schemas and conversion.
- Implement datetime dynamic conversion and bytes rejection.
- Add dynamic request tests.

Validation:

```sh
cd sdks/go && go test ./...
```

### Phase 5: Query Bundles

- Implement registration model.
- Implement bundle serialization/deserialization/generation.
- Add duplicate route and unsupported version tests.

Validation:

```sh
cd sdks/go && go test ./...
```

### Phase 6: HTTP Client

- Implement client, query builder, request sender, headers, body handling, and typed errors.
- Add `httptest.Server` tests.

Validation:

```sh
cd sdks/go && go test ./...
```

### Phase 7: Parity Fixture Generator

- Port all Rust/TypeScript parity fixtures into Go.
- Generate `sdks/tests/parity/generated/go` fixture tree.
- Validate fixture counts.

Validation:

```sh
cd sdks/go && go run ./cmd/generate-parity-fixtures ../tests/parity/generated/go
```

### Phase 8: Three-Way Parity Harness

- Extend TypeScript paths, scripts, JSON comparison, and runtime runner for Go.
- Run full parity suite.

Validation:

```sh
cd sdks/typescript && npm run test:parity
```

### Phase 9: CI And Docs

- Update GitHub Actions parity workflow.
- Add README.
- Finalize examples and acceptance checklist.

Validation:

```sh
cd sdks/go && go test ./...
cd ../typescript && npm run test:parity
```

## Acceptance Criteria

- `sdks/go` is a valid Go module at `github.com/helixdb/helix-db/sdks/go`.
- `go test ./...` passes in `sdks/go`.
- Go SDK exposes read/write DSL, dynamic request generation, query bundles, params, and HTTP client.
- Go SDK emits structurally identical request JSON to Rust and TypeScript for every parity fixture.
- Go fixture generator emits 224 runtime fixtures and 8 JSON-only fixtures.
- Existing Rust/TypeScript parity remains green.
- Three-way structural parity passes.
- Three-way runtime parity passes through the Helix runner.
- CI runs Go tests and the full parity suite.
- README documents common SDK workflows.

## Key Risks And Mitigations

### Risk: Go map ordering breaks fixture parity

Mitigation:

- Use sorted-key encoders for maps that appear in generated JSON.
- Use slices for ordered properties, projections, steps, returns, and parameters where ordering matters.

### Risk: JSON enum shapes drift from Rust serde output

Mitigation:

- Write targeted unit tests for every enum representation.
- Use parity fixtures as integration tests.
- Treat Rust `dsl.rs` as source of truth for tags and field names.

### Risk: Large integer precision loss

Mitigation:

- Keep `int64` as `int64` through serialization.
- Avoid `float64` for integer values.
- Decode client `any` responses with `json.Decoder.UseNumber()`.

### Risk: Dynamic parameter conversion differs from TypeScript/Rust

Mitigation:

- Port TypeScript parameter conversion tests.
- Explicitly test DateTime RFC3339 conversion and bytes rejection.

### Risk: Go API becomes a direct Rust clone instead of idiomatic Go

Mitigation:

- Use Go naming and error conventions.
- Avoid generic-heavy typestate.
- Keep builders clear, explicit, and testable.

### Risk: Parity generator diverges from Rust/TypeScript generators

Mitigation:

- Port fixture logic mechanically first.
- Add count assertions.
- Keep fixture names and buckets identical.

## Commands Summary

From `sdks/go`:

```sh
go test ./...
go run ./cmd/generate-parity-fixtures ../tests/parity/generated/go
```

From `sdks/typescript`:

```sh
npm run parity:generate
npm run parity:compare-json
npm run parity:helix
npm run test:parity
```

Final validation:

```sh
cd sdks/go && go test ./...
cd ../typescript && npm run test:parity
```
