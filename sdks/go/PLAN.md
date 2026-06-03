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
- Dynamic `/v1/query` request generation and execution.
- Inline dynamic parameter declaration and conversion support.
- HTTP client support optimized for dynamic query execution.
- Unit tests covering Go API behavior and JSON shape.
- Full integration into the existing Rust/TypeScript SDK parity suite.
- Documentation and examples.

Out of v1 primary scope:

- Stored query client routes.
- Query bundle generation.
- Rust-style registration APIs.

Those can be added later as advanced parity utilities if users ask for them, but the v1 Go SDK should optimize for the easiest dynamic-query developer experience.

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
    - TypeScript demonstrates dynamic request JSON shape and parity behavior.
    - Go should diverge from TypeScript registration/bundle ergonomics when a normal-function Go API is simpler.

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
  client.go
  errors.go
  json.go
  values_test.go
  dsl_test.go
  dynamic_test.go
  client_test.go
  cmd/
    generate-parity-fixtures/
      main.go
```

## Public API Shape

The SDK should feel natural in Go while staying close enough to Rust/TypeScript that users can translate examples easily.

Primary usage should be ordinary Go functions that return `helix.Request`. Query names and parameters live inside the function, not in call-site options or a separate `.With(...)` step.

Example read query:

```go
func FindUsers(tenantID string, limit int64) helix.Request {
    q := helix.ReadQuery("find_users")

    tenant := q.ParamString("tenant_id", tenantID)
    lim := q.ParamI64("limit", limit)

    return q.
        VarAs("users",
            helix.G().
                NWithLabel("User").
                Where(helix.PredEq("tenantId", tenant)).
                Limit(lim).
                ValueMap("$id", "name", "tenantId"),
        ).
        Returning("users")
}
```

Example write query:

```go
func CreateUser(name string, tier string) helix.Request {
    q := helix.WriteQuery("create_user")

    nameParam := q.ParamString("name", name)
    tierParam := q.ParamString("tier", tier)

    return q.
        VarAs("alice",
            helix.G().AddN("User", helix.Props{
                helix.Prop("name", nameParam),
                helix.Prop("tier", tierParam),
            }),
        ).
        Returning("alice")
}
```

Example client usage:

```go
client, err := helix.NewClient("http://localhost:6969")
if err != nil {
    return err
}

var out map[string]any
err = client.Exec(ctx, FindUsers("acme", 25), &out)
```

## Engineer-Facing Example

This is the kind of example to show application engineers. They write ordinary Go functions and pass the returned request to the client.

```go
package users

import (
    "context"

    helix "github.com/helixdb/helix-db/sdks/go"
)

type UserRow struct {
    ID       int64  `json:"$id"`
    Name     string `json:"name"`
    TenantID string `json:"tenantId"`
}

type FindUsersResponse struct {
    Users []UserRow `json:"users"`
}

func FindUsers(tenantID string, limit int64) helix.Request {
    q := helix.ReadQuery("find_users")

    tenant := q.ParamString("tenant_id", tenantID)
    maxRows := q.ParamI64("limit", limit)

    return q.
        VarAs("users",
            helix.G().
                NWithLabel("User").
                Where(helix.PredEq("tenantId", tenant)).
                Limit(maxRows).
                ValueMap("$id", "name", "tenantId"),
        ).
        Returning("users")
}

func ListUsers(ctx context.Context, client *helix.Client, tenantID string, limit int64) (FindUsersResponse, error) {
    var out FindUsersResponse
    err := client.Exec(ctx, FindUsers(tenantID, limit), &out)
    return out, err
}
```

Write example:

```go
type CreateUserResponse struct {
    User []UserRow `json:"user"`
}

func CreateUser(name string, tenantID string) helix.Request {
    q := helix.WriteQuery("create_user")

    nameParam := q.ParamString("name", name)
    tenant := q.ParamString("tenant_id", tenantID)

    return q.
        VarAs("user",
            helix.G().AddN("User", helix.Props{
                helix.Prop("name", nameParam),
                helix.Prop("tenantId", tenant),
            }),
        ).
        Returning("user")
}

func SaveUser(ctx context.Context, client *helix.Client, name string, tenantID string) (CreateUserResponse, error) {
    var out CreateUserResponse
    err := client.Exec(ctx, CreateUser(name, tenantID), &out,
        helix.WriterOnly(),
        helix.AwaitDurability(true),
    )
    return out, err
}
```

The important DX rule: users should not need to call `MarshalRequest`, `MarshalJSON`, `ToJSON`, or `ToJSONString` in normal application code.

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

### Encoding Architecture

Implement `json.go` first. All AST types should use a small shared encoder layer instead of each type hand-rolling JSON strings independently.

Required helper concepts:

```go
type jsonValue interface {
    appendJSON(dst []byte) ([]byte, error)
}

func jsonUnit(name string) jsonValue
func jsonNewtype(name string, value any) jsonValue
func jsonTuple(name string, values ...any) jsonValue
func jsonStruct(name string, fields orderedFields) jsonValue
func jsonObjectSorted(entries map[string]any) jsonValue
func jsonObjectOrdered(fields orderedFields) jsonValue
```

`orderedFields` should preserve field order for serde-like struct output while allowing fields to be omitted deliberately:

```go
type field struct {
    Name  string
    Value any
    Omit  bool
}

type orderedFields []field
```

Use these helpers to implement Rust serde-style enum shapes consistently:

- `jsonUnit("Count")` -> `"Count"`
- `jsonNewtype("N", NodeVar("user"))` -> `{ "N": { "Var": "user" } }`
- `jsonTuple("Eq", "name", String("Alice"))` -> `{ "Eq": ["name", { "String": "Alice" }] }`
- `jsonStruct("CreateIndex", fields)` -> `{ "CreateIndex": { ... } }`

The SDK should not make explicit JSON conversion part of the primary developer experience. Users should call `client.Exec(ctx, request, &out)`, and the client should serialize internally.

Public AST/request types that are passed to `encoding/json` must implement `MarshalJSON()` by delegating to the internal `jsonValue` encoder. For debugging and tests, expose `MarshalRequest(req Request) ([]byte, error)` as a secondary helper instead of `ToJSON()` / `ToJSONString()` methods.

Do not rely on anonymous structs with `omitempty` for wire-critical AST payloads unless the struct shape has no optional/null-sensitive fields.

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

- Dynamic parameter maps should serialize with sorted keys for stable fixture output.
- Object property values should use sorted keys when represented as maps.
- Property pairs in `AddN`/`AddE` must preserve caller-provided order by using slices, not maps.

Implementation rule:

- Use slices for ordered semantic data: steps, properties, projections, returns, parameters, fixture lists, and batch entries.
- Use sorted-object encoding for unordered semantic data: dynamic parameter objects and `PropertyValue.Object`.
- Tests should compare both parsed JSON shape and selected raw JSON strings where field omission or explicit `null` matters.

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

Required API:

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

Exact native property conversion rules:

- `nil` converts to `Null` only when passed as a property value, property input value, or explicit dynamic value; nil slices and nil maps should encode as empty arrays/objects only when explicitly constructed as typed arrays/objects.
- `string` converts to `String`.
- `bool` converts to `Bool`.
- Signed integer types convert to `I64`; reject overflow if conversion is not exact.
- Unsigned integer types convert to `I64` only when the value fits in `math.MaxInt64`; reject overflow.
- `float64` converts to `F64`; reject NaN and infinity.
- `float32` converts to `F32`; reject NaN and infinity.
- `time.Time` converts to `DateTime` using UTC epoch milliseconds.
- `DateTime` converts to `DateTime`.
- `[]byte` converts to `Bytes`.
- `[]int64`, `[]float64`, `[]float32`, and `[]string` convert to typed arrays.
- `[]any` and mixed slices convert to heterogeneous `Array` with recursive conversion.
- `map[string]any` converts to `Object` with recursive conversion and sorted-key JSON output.
- `map[string]PropertyValue` converts to `Object` with sorted-key JSON output.
- Unsupported map key types must fail validation.

Ordered property helpers:

```go
type PropPair struct {
    Name  string
    Value PropertyInput
}

type Props []PropPair

func Prop(name string, value any) PropPair
func PropInput(name string, value PropertyInput) PropPair
```

Use `Props` for `AddN` and `AddE` so property order is stable and matches Rust/TypeScript fixture output.

Object helper:

```go
type ObjectEntry struct {
    Key   string
    Value PropertyValue
}

type ObjectEntries []ObjectEntry

func ObjectFromEntries(entries ...ObjectEntry) PropertyValue
func Entry(key string, value any) ObjectEntry
```

`PropertyValue.Object` may accept maps for user convenience, but fixture generation should prefer `ObjectEntries` where exact source order is useful before sorted object encoding.

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

Required API:

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

Required API:

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

Required API:

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

Required API:

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
- `CreateVectorIndexNodes`
- `CreateVectorIndexEdges`
- `CreateTextIndexNodes`
- `CreateTextIndexEdges`

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

## Traversal Method Inventory

Implement public builder methods for every common Rust/TypeScript traversal helper, not only the raw `Step` constructors.

Source methods:

- `N(ref NodeRef)`
- `NWhere(predicate SourcePredicate)`
- `NWithLabel(label string)`
- `NWithLabelWhere(label string, predicate SourcePredicate)`
- `E(ref EdgeRef)`
- `EWhere(predicate SourcePredicate)`
- `EWithLabel(label string)`
- `EWithLabelWhere(label string, predicate SourcePredicate)`
- `VectorSearchNodes(label, property string, queryVector any, k any, tenantValue ...any)`
- `VectorSearchNodesWith(label, property string, queryVector PropertyInput, k StreamBound, tenantValue *PropertyInput)`
- `TextSearchNodes(label, property string, queryText any, k any, tenantValue ...any)`
- `TextSearchNodesWith(label, property string, queryText PropertyInput, k StreamBound, tenantValue *PropertyInput)`
- `VectorSearchEdges(label, property string, queryVector any, k any, tenantValue ...any)`
- `VectorSearchEdgesWith(label, property string, queryVector PropertyInput, k StreamBound, tenantValue *PropertyInput)`
- `TextSearchEdges(label, property string, queryText any, k any, tenantValue ...any)`
- `TextSearchEdgesWith(label, property string, queryText PropertyInput, k StreamBound, tenantValue *PropertyInput)`

Navigation methods:

- `Out(label ...string)`
- `In(label ...string)`
- `Both(label ...string)`
- `OutE(label ...string)`
- `InE(label ...string)`
- `BothE(label ...string)`
- `OutN()`
- `InN()`
- `OtherN()`

Filter and set methods:

- `Has(property string, value any)`
- `HasLabel(label string)`
- `HasKey(property string)`
- `Where(predicate Predicate)`
- `Dedup()`
- `Within(varName string)`
- `Without(varName string)`
- `EdgeHas(property string, value any)`
- `EdgeHasLabel(label string)`

Bound methods:

- `Limit(bound any)`
- `Skip(bound any)`
- `Range(start any, end any)`
- `LimitBy(expr Expr)`
- `SkipBy(expr Expr)`
- `RangeBy(start StreamBound, end StreamBound)`

Variable methods:

- `As(name string)`
- `Store(name string)`
- `Select(name string)`
- `Inject(name string)`

Terminal methods:

- `Count()`
- `Exists()`
- `ID()`
- `Label()`
- `Values(properties ...string)`
- `ValueMap(properties ...string)` for filtered maps
- `ValueMapAll()` for `{ "ValueMap": null }`
- `Project(projections ...Projection)`
- `EdgeProperties()`

Mutation methods:

- `AddN(label string, properties Props)`
- `AddE(label string, to NodeRef, properties Props)`
- `SetProperty(name string, value any)`
- `RemoveProperty(name string)`
- `Drop()`
- `DropEdge(to NodeRef)`
- `DropEdgeLabeled(to NodeRef, label string)`
- `DropEdgeByID(ref EdgeRef)`

Index methods:

- `CreateIndexIfNotExists(spec IndexSpec)`
- `DropIndex(spec IndexSpec)`
- `CreateVectorIndexNodes(label, property string, tenantProperty ...string)`
- `CreateTextIndexNodes(label, property string, tenantProperty ...string)`
- `CreateVectorIndexEdges(label, property string, tenantProperty ...string)`
- `CreateTextIndexEdges(label, property string, tenantProperty ...string)`

Ordering, branching, and aggregation methods:

- `OrderBy(property string, order Order)`
- `OrderByMultiple(orderings ...Ordering)`
- `Repeat(config RepeatConfig)`
- `Union(traversals ...SubTraversal)`
- `Choose(condition Predicate, thenTraversal SubTraversal, elseTraversal ...SubTraversal)`
- `Coalesce(traversals ...SubTraversal)`
- `Optional(traversal SubTraversal)`
- `Group(property string)`
- `GroupCount(property string)`
- `AggregateBy(fn AggregateFunction, property string)`

Reserved/no-op methods:

- `Fold()`
- `Unfold()`
- `Path()`
- `SimplePath()`
- `WithSack(value any)`
- `SackSet(property string)`
- `SackAdd(property string)`
- `SackGet()`

Raw construction escape hatches:

```go
func TraversalFromSteps(steps []Step) *Traversal
func SubTraversalFromSteps(steps []Step) SubTraversal
func (t *Traversal) Steps() []Step
```

Escape hatches must preserve JSON parity but do not need to validate semantic correctness beyond malformed local data.

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

### Validation And Error Model

Use one concrete validation model throughout the Go SDK:

- Fluent builder methods do not return `(value, error)`.
- Builders collect the first construction error internally.
- `Validate() error` returns the first construction error.
- `Err() error` is an alias for `Validate()`.
- `MarshalJSON()` and `MarshalRequest(req)` call `Validate()` before serialization and return an error.
- Client `Exec(ctx, req, out, opts...)` calls request serialization and returns any validation or serialization error.
- No builder method should panic for normal user input errors.

Read/write validation rules:

- `ReadBatch.VarAs` and `ReadBatch.VarAsIf` must record `ErrWriteTraversalInReadBatch` if the traversal contains mutation steps.
- `WriteBatch.VarAs` and `WriteBatch.VarAsIf` accept read-only and write traversals.
- `ReadQuery(name)` and `WriteQuery(name)` return request builders that collect both query-building and parameter-conversion errors.
- `Returning(...)` finalizes a request value; errors surface through `Validate`, `MarshalJSON`, `MarshalRequest`, or `client.Exec`.
- Parameter methods such as `ParamI64` and `ParamDateTime` must record conversion errors on the owning query builder.

Concrete API requirements:

```go
func (t *Traversal) Validate() error
func (t *Traversal) Err() error

func (b *ReadBatch) Validate() error
func (b *ReadBatch) Err() error
func (b *ReadBatch) MarshalJSON() ([]byte, error)

func (b *WriteBatch) Validate() error
func (b *WriteBatch) Err() error
func (b *WriteBatch) MarshalJSON() ([]byte, error)

type Request interface {
    json.Marshaler
    Validate() error
    isHelixRequest()
}

func MarshalRequest(req Request) ([]byte, error)
```

Required errors:

```go
var ErrWriteTraversalInReadBatch = errors.New("helix: read batch cannot contain write traversal")
var ErrUnsupportedBytesParameter = errors.New("helix: dynamic query JSON cannot represent bytes parameters")
var ErrMissingParameter = errors.New("helix: missing required parameter")
var ErrUnknownParameter = errors.New("helix: unknown parameter")
var ErrInvalidParameterType = errors.New("helix: invalid parameter type")
var ErrInvalidDateTimeParameter = errors.New("helix: invalid datetime parameter")
```

Use wrapper errors with parameter paths where relevant:

```go
type PathError struct {
    Path string
    Err  error
}
```

## Batch API

Support:

- Read batch
- Write batch
- `VarAs`
- `VarAsIf`
- `ForEachParam`
- `Returning`
- `Validate`
- `Err`
- `MarshalJSON`

Batch entries must serialize as externally tagged variants:

```json
{ "Query": { "name": "users", "steps": [...], "condition": null } }
```

```json
{ "ForEach": { "param": "items", "body": [...] } }
```

## Dynamic Request API

The primary request API is dynamic-first and function-oriented. Users should write normal Go functions that return `helix.Request`, then pass those requests directly to `client.Exec`.

Required entry points:

```go
func ReadQuery(name string) *ReadQueryBuilder
func WriteQuery(name string) *WriteQueryBuilder
func MarshalRequest(req Request) ([]byte, error)
```

Required request-builder behavior:

- `ReadQuery("find_users")` creates a read dynamic request builder with `query_name: "find_users"`.
- `ReadQuery("")` creates an unnamed read dynamic request builder with `query_name: null`.
- `WriteQuery` follows the same rules for write dynamic requests.
- Query builders expose `VarAs`, `VarAsIf`, `ForEachParam`, and `Returning` methods matching read/write batch behavior.
- `Returning(vars ...string)` finalizes and returns `helix.Request`.
- Query builders expose inline parameter methods that both record runtime values and return refs usable in the DSL.
- There is no `.With(...)` request construction step.
- There is no `WithQueryName(...)` option in the primary API; the query name is declared once in `ReadQuery` / `WriteQuery`.
- There are no primary `ToJSON()` or `ToJSONString()` methods.

Concrete dynamic value model:

```go
type DynamicValue any

func DynamicNull() DynamicValue
func DynamicBool(value bool) DynamicValue
func DynamicI64(value int64) DynamicValue
func DynamicF64(value float64) DynamicValue
func DynamicF32(value float32) DynamicValue
func DynamicString(value string) DynamicValue
func DynamicArray(values ...DynamicValue) DynamicValue
func DynamicObject(values map[string]DynamicValue) DynamicValue
```

`DynamicValue` serializes as plain JSON, not as tagged `PropertyValue` JSON.

Concrete request shape:

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

With inline parameters:

```json
{
  "request_type": "read",
  "query_name": "find_users",
  "query": { "queries": [], "returns": [] },
  "parameters": { "limit": 25 },
  "parameter_types": { "limit": "I64" }
}
```

## Inline Dynamic Parameters

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

Concrete type model:

```go
type QueryParamType struct {
    Kind  ParamKind
    Inner *QueryParamType
}

type ParamRef struct {
    Name   string
    Type   QueryParamType
}
```

Required inline parameter methods on both `ReadQueryBuilder` and `WriteQueryBuilder`:

```go
func (q *ReadQueryBuilder) ParamBool(name string, value bool) ParamRef
func (q *ReadQueryBuilder) ParamI64(name string, value any) ParamRef
func (q *ReadQueryBuilder) ParamF64(name string, value any) ParamRef
func (q *ReadQueryBuilder) ParamF32(name string, value any) ParamRef
func (q *ReadQueryBuilder) ParamString(name string, value string) ParamRef
func (q *ReadQueryBuilder) ParamDateTime(name string, value any) ParamRef
func (q *ReadQueryBuilder) ParamValue(name string, value any) ParamRef
func (q *ReadQueryBuilder) ParamObject(name string, value any, inner ...QueryParamType) ParamRef
func (q *ReadQueryBuilder) ParamArray(name string, value any, inner QueryParamType) ParamRef

func (q *WriteQueryBuilder) ParamBool(name string, value bool) ParamRef
func (q *WriteQueryBuilder) ParamI64(name string, value any) ParamRef
func (q *WriteQueryBuilder) ParamF64(name string, value any) ParamRef
func (q *WriteQueryBuilder) ParamF32(name string, value any) ParamRef
func (q *WriteQueryBuilder) ParamString(name string, value string) ParamRef
func (q *WriteQueryBuilder) ParamDateTime(name string, value any) ParamRef
func (q *WriteQueryBuilder) ParamValue(name string, value any) ParamRef
func (q *WriteQueryBuilder) ParamObject(name string, value any, inner ...QueryParamType) ParamRef
func (q *WriteQueryBuilder) ParamArray(name string, value any, inner QueryParamType) ParamRef
```

Required `ParamRef` helpers:

```go
func (r ParamRef) Expr() Expr
func (r ParamRef) Input() PropertyInput
func (r ParamRef) Bound() StreamBound
```

Required `QueryParamType` constructors for advanced array/object params and JSON-only parity fixtures:

```go
func ParamTypeBool() QueryParamType
func ParamTypeI64() QueryParamType
func ParamTypeF64() QueryParamType
func ParamTypeF32() QueryParamType
func ParamTypeString() QueryParamType
func ParamTypeDateTime() QueryParamType
func ParamTypeBytes() QueryParamType
func ParamTypeValue() QueryParamType
func ParamTypeObject() QueryParamType
func ParamTypeArray(inner QueryParamType) QueryParamType
```

Parameter validation:

- Duplicate parameter names on one query builder must record an error.
- Parameter methods must insert both runtime `parameters` and `parameter_types` metadata.
- `ParamBytes` is intentionally not part of the primary builder API because dynamic JSON cannot represent bytes; bytes metadata remains available through `ParamTypeBytes()` for JSON-only parity construction if needed.
- A `ParamRef` serializes as `Expr.Param(name)` when used as an expression, property input, predicate value, or stream bound.
- Query builders must preserve parameter insertion order internally where arrays are used, while request JSON parameter maps are sorted for deterministic output.

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
- Duplicate parameter names should be rejected.
- Parameter conversion failures should be recorded on the query builder and surfaced through request validation or `client.Exec`.

Exact native conversion rules:

- `Bool` accepts `bool` only.
- `I64` accepts signed integer types and unsigned integer types that fit in `int64`; reject overflow.
- `I64` rejects `float32` and `float64` even when integral.
- `F64` accepts `float64`, `float32`, and integer types; reject NaN and infinity.
- `F32` accepts `float32`, `float64`, and integer types; reject NaN and infinity.
- `String` accepts `string` only.
- `DateTime` accepts `DateTime`, `time.Time`, RFC3339 `string`, and integer millis; output canonical UTC RFC3339 with millisecond precision.
- `Bytes` always fails for dynamic JSON, even if the value is valid Go bytes.
- `Value` accepts `PropertyValue` and safe native property-value inputs, then converts to untagged dynamic JSON.
- `Object(inner)` accepts `map[string]any`, `map[string]PropertyValue`, and ordered object helpers; each entry is converted with `inner` or `ParamValue()` when omitted.
- `Array(inner)` accepts slices and arrays; each element is converted with `inner`.
- `nil` is accepted only where it maps to `PropertyValue.Null` or dynamic JSON `null`; it is not accepted for missing required parameters.
- Non-finite floats must be rejected anywhere they would be serialized as JSON numbers.

Dynamic conversion path formatting must match Rust/TypeScript diagnostics where practical:

- Nested object field: `payload.metadata.score`
- Array entry: `items[0]`

## HTTP Client

Mirror Rust/TypeScript behavior.

Client construction:

```go
client, err := helix.NewClient("")
client, err := helix.NewClient("https://cluster.helix-db.com")
client = client.WithAPIKey("hx_secret")
```

Concrete client constructors and options:

```go
type ClientOption func(*Client)

func NewClient(baseURL string, opts ...ClientOption) (*Client, error)
func WithHTTPClient(httpClient *http.Client) ClientOption
func WithAPIKey(apiKey string) ClientOption

func (c *Client) WithAPIKey(apiKey string) *Client
func (c *Client) ClearAPIKey() *Client
func (c *Client) BaseURL() string
```

`WithHTTPClient` is required for tests, custom timeouts, and application-owned transports. If no HTTP client is provided, use `http.DefaultClient`.

`NewClient("")` should use the default base URL.

Default base URL:

```text
http://localhost:6969
```

Routes:

- Dynamic query: `POST /v1/query`

Headers:

- Always set `Content-Type: application/json`.
- `WriterOnly()` sets `x-helix-require-writer: true`.
- `WarmOnly()` sets `x-helix-warm: true`.
- `ShouldAwaitDurability(bool)` sets `x-helix-await-durable: true|false`.
- API key sets `Authorization: Bearer <key>`.

Required API:

```go
func (c *Client) Exec(ctx context.Context, req Request, out any, opts ...ExecOption) error

type ExecOption func(*execOptions)

func WriterOnly() ExecOption
func WarmOnly() ExecOption
func AwaitDurability(should bool) ExecOption
```

Usage:

```go
err := client.Exec(ctx, FindUsers("acme", 25), &out)

err = client.Exec(ctx, CreateUser("Alice", "pro"), &created,
    helix.WriterOnly(),
    helix.AwaitDurability(false),
)
```

Concrete query builder behavior:

- `Exec` always posts to `/v1/query`.
- `Exec` serializes `req` internally with `MarshalRequest(req)`.
- `Exec` returns request validation or serialization errors before making an HTTP request.
- `Exec` applies request headers from `ExecOption` values.
- If `out == nil`, `Exec` should discard a successful response body after reading it.
- If `out` points to `any`, `map[string]any`, or `[]any`, decode with `json.Decoder.UseNumber()`.
- If `out` points to a concrete struct/slice, normal `encoding/json` decoding is acceptable, still using `UseNumber()` on the decoder.

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
- Inline parameter values and types from `ReadQuery` / `WriteQuery` builders.
- DateTime dynamic params as RFC3339.
- Omission of absent `parameters` and `parameter_types`.
- `MarshalRequest(req)` emits the expected dynamic request JSON.

### Request Builder Tests

Cover:

- Normal functions returning `helix.Request` work for read and write queries.
- Duplicate inline parameter names fail validation.
- Parameter refs work as predicate values, property inputs, and stream bounds.
- Empty query name emits `query_name: null`.
- Named query emits `query_name: "..."`.

### Client Tests

Use `httptest.Server`.

Cover:

- Default URL behavior.
- Invalid URL behavior.
- `Exec` posts to `/v1/query`.
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

Source-of-truth strategy:

- Treat the Rust generator as the canonical fixture list and fixture naming source.
- Port fixture construction mechanically from Rust first, then adjust only for idiomatic Go syntax.
- Use the TypeScript generator as a secondary check for ergonomic helper behavior and explicit dynamic parameter conversion.
- Keep Go fixture names, bucket names, ordering, and parameter values identical to Rust.
- When Rust/TypeScript fixture generators change later, update Go in the same PR.
- Add comments in the Go generator grouping fixture blocks by the same sections as Rust: runtime fixtures, node permutation fixtures, and JSON-only fixtures.

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
- Default to `../tests/parity/generated/go` when run from `sdks/go`.
- `package.json` must invoke the generator with an explicit output directory: `../tests/parity/generated/go`.
- Clear and recreate `runtime` and `json-only` output directories.
- Write compact JSON requests.
- Preserve all property pair and step ordering.
- Assert exact generated counts before exiting: 224 runtime fixtures and 8 JSON-only fixtures.
- Fail if two fixtures produce the same relative path.

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
    cache-dependency-path: sdks/go/go.sum
```

Add Go formatting and test steps before full parity:

```yaml
- name: Check Go formatting
  working-directory: sdks/go
  run: test -z "$(gofmt -l .)"
```

```yaml
- name: Test Go SDK
  working-directory: sdks/go
  run: go test ./...
```

If the module remains stdlib-only and has no `go.sum`, use `cache-dependency-path: sdks/go/go.mod` instead.

Keep final parity command:

```yaml
- name: Run parity suite
  run: npm run test:parity
```

The parity suite should now include Go fixture generation and Go runtime comparison.

## Documentation

Add `sdks/go/README.md` with:

- Install/import instructions.
- Dynamic-first quick start.
- Normal Go function examples returning `helix.Request`.
- Read query example.
- Write query example.
- Inline parameter example.
- `client.Exec` example.
- Notes about parity with Rust/TypeScript.
- Notes about int64, datetime, bytes dynamic params, and deterministic output.

## External Docs And Skill Updates

The Go SDK work is not complete until the public docs repo and the HelixDB skills repo are updated to teach the new Go DX.

### Public Docs Repo

Target repo:

```text
/Users/xav/GitHub/helix-ql-docs
```

Required docs changes:

- Add `database/go-project-setup.mdx`.
- Add `database/go-project-setup` to `docs.json` under the HelixDB Getting Started pages, next to Rust and TypeScript setup.
- Update `database/querying.mdx` to mention Go as a supported dynamic-query SDK.
- Update `database/querying-guide/parameters-bundles.mdx` to distinguish SDK workflows:
  - Rust and TypeScript support registration/bundle workflows.
  - Go v1 is dynamic-first.
  - Go params are declared inline through methods such as `q.ParamString`, `q.ParamI64`, and `q.ParamDateTime`.
- Include Go snippets that demonstrate the primary DX:
  - normal Go functions returning `helix.Request`
  - `helix.ReadQuery("find_users")`
  - `helix.WriteQuery("create_user")`
  - inline params
  - `client.Exec(ctx, request, &out)`
  - no `.With(...)`
  - no `WithQueryName(...)`
  - no primary `ToJSON()` / `ToJSONString()` usage
- Regenerate and verify AI docs indexes:
  - `npm run generate-llms`
  - `npm run check-llms`

Canonical docs snippet:

```go
func FindUsers(tenantID string, limit int64) helix.Request {
    q := helix.ReadQuery("find_users")

    tenant := q.ParamString("tenant_id", tenantID)
    maxRows := q.ParamI64("limit", limit)

    return q.
        VarAs("users",
            helix.G().
                NWithLabel("User").
                Where(helix.PredEq("tenantId", tenant)).
                Limit(maxRows).
                ValueMap("$id", "name", "tenantId"),
        ).
        Returning("users")
}

var out FindUsersResponse
err := client.Exec(ctx, FindUsers("acme", 25), &out)
```

### Skills Repo

Target repo:

```text
/Users/xav/GitHub/skills
```

Required skill changes:

- Add a new Go SDK skill:
  - `skills/helix-query-go/SKILL.md`
  - `skills/helix-query-go/REFERENCE.md`
  - `skills/helix-query-go/EXAMPLES.md`
- Update `/Users/xav/GitHub/skills/README.md` to list `helix-query-go` as an available skill.
- Update shared references as needed:
  - `docs/source-canon.md`
  - `docs/dsl-cheatsheet.md`, or add `docs/go-dsl-cheatsheet.md` if a separate Go cheat sheet is clearer.
- The Go skill must teach agents to use the dynamic-first Go SDK API:
  - write normal Go functions returning `helix.Request`
  - set query names with `ReadQuery` / `WriteQuery`
  - declare runtime params inline with `q.ParamString`, `q.ParamI64`, `q.ParamDateTime`, etc.
  - pass parameter refs directly to predicates, bounds, property inputs, search inputs, and projections where supported
  - execute with `client.Exec(ctx, request, &out)`
  - avoid `.With(...)`
  - avoid `WithQueryName(...)`
  - avoid stored-query and bundle workflows for Go v1
  - use `MarshalRequest` only for tests, parity fixtures, or debugging

Suggested `skills/helix-query-go/SKILL.md` frontmatter:

```yaml
---
name: helix-query-go
description: Write and revise HelixDB queries with the Go SDK. Use when building dynamic Helix queries in Go with normal functions returning helix.Request, ReadQuery/WriteQuery, inline params, traversal builders, projections, indexes, BM25 text search, vector search, and client.Exec. Dynamic-first; do not use stored-query or bundle workflows for Go v1.
license: MIT
metadata:
  author: HelixDB
  version: 0.1.0
---
```

Skill examples must match the public docs and SDK README examples exactly where possible so agents do not learn a different API than application engineers.

## Implementation Phases

### Phase 1: Scaffolding

- Add `go.mod`.
- Add package skeleton files.
- Add core error and JSON helper infrastructure.
- Implement the shared serde-style JSON helper layer in `json.go`.
- Implement the builder validation/error model.
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

- Implement `ReadQuery` and `WriteQuery` request builders.
- Implement inline parameter methods and conversion.
- Implement `ParamRef` expression/input/bound behavior.
- Implement datetime dynamic conversion and bytes rejection.
- Add dynamic request tests.

Validation:

```sh
cd sdks/go && go test ./...
```

### Phase 5: HTTP Client

- Implement client, `Exec`, request serialization, headers, response decoding, and typed errors.
- Add `httptest.Server` tests.

Validation:

```sh
cd sdks/go && go test ./...
```

### Phase 6: Parity Fixture Generator

- Port all Rust/TypeScript parity fixtures into Go.
- Generate `sdks/tests/parity/generated/go` fixture tree.
- Validate fixture counts.

Validation:

```sh
cd sdks/go && go run ./cmd/generate-parity-fixtures ../tests/parity/generated/go
```

### Phase 7: Three-Way Parity Harness

- Extend TypeScript paths, scripts, JSON comparison, and runtime runner for Go.
- Run full parity suite.

Validation:

```sh
cd sdks/typescript && npm run test:parity
```

### Phase 8: External Docs And Skills

- Update `/Users/xav/GitHub/helix-ql-docs` with Go SDK documentation and navigation.
- Regenerate and check `llms.txt` / `llms-full.txt` in the docs repo.
- Add `/Users/xav/GitHub/skills/skills/helix-query-go`.
- Update the skills repo README and shared references.

Validation:

```sh
cd /Users/xav/GitHub/helix-ql-docs && npm run generate-llms && npm run check-llms
```

### Phase 9: CI And Local Docs

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
- `gofmt -l .` returns no files in `sdks/go`.
- `go test ./...` passes in `sdks/go`.
- Go SDK exposes read/write DSL, dynamic request builders, inline params, and dynamic-only HTTP client execution.
- Normal Go functions returning `helix.Request` are the documented primary API.
- Go SDK exposes concrete `ReadQuery`, `WriteQuery`, `ParamRef`, `Request`, `MarshalRequest`, and `Client.Exec` APIs.
- Builder validation returns errors through `Validate`, `MarshalJSON`, `MarshalRequest`, and client `Exec`; normal user input errors do not panic.
- Go SDK emits structurally identical request JSON to Rust and TypeScript for every parity fixture.
- Go fixture generator emits 224 runtime fixtures and 8 JSON-only fixtures.
- Existing Rust/TypeScript parity remains green.
- Three-way structural parity passes.
- Three-way runtime parity passes through the Helix runner.
- `/Users/xav/GitHub/helix-ql-docs` includes a Go project setup page and dynamic-first Go SDK examples.
- `/Users/xav/GitHub/helix-ql-docs/docs.json` includes `database/go-project-setup` in navigation.
- `/Users/xav/GitHub/helix-ql-docs/llms.txt` and `llms-full.txt` are regenerated and pass `npm run check-llms`.
- `/Users/xav/GitHub/skills` includes `skills/helix-query-go/SKILL.md`, `REFERENCE.md`, and `EXAMPLES.md`.
- `/Users/xav/GitHub/skills/README.md` lists `helix-query-go` as an available skill.
- The Go docs and Go skill both show the same primary DX: normal Go functions returning `helix.Request` and `client.Exec(ctx, request, &out)`.
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
test -z "$(gofmt -l .)"
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

From `/Users/xav/GitHub/helix-ql-docs`:

```sh
npm run generate-llms
npm run check-llms
```

From `/Users/xav/GitHub/skills`:

```sh
test -f skills/helix-query-go/SKILL.md
test -f skills/helix-query-go/REFERENCE.md
test -f skills/helix-query-go/EXAMPLES.md
```

Final validation:

```sh
cd sdks/go && test -z "$(gofmt -l .)" && go test ./...
cd ../typescript && npm run test:parity
cd /Users/xav/GitHub/helix-ql-docs && npm run generate-llms && npm run check-llms
```
