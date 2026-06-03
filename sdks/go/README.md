# HelixDB Go SDK

Dynamic-first Go SDK for building and executing HelixDB queries.

## Install

```sh
go get github.com/helixdb/helix-db/sdks/go
```

```go
import helix "github.com/helixdb/helix-db/sdks/go"
```

## Query Functions

Write normal Go functions that return `helix.Request`. Set the query name with `ReadQuery` or `WriteQuery`, declare runtime parameters inline, then pass the request to `Client.Exec`.

```go
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
```

## Execute

```go
client, err := helix.NewClient("http://localhost:6969")
if err != nil {
	return err
}

var out FindUsersResponse
err = client.Exec(ctx, FindUsers("acme", 25), &out)
```

## Writes

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

err = client.Exec(ctx, CreateUser("Alice", "acme"), &created,
	helix.WriterOnly(),
	helix.AwaitDurability(true),
)
```

## Parameters

Inline parameter helpers insert both runtime values and `parameter_types` metadata:

```go
q := helix.ReadQuery("recent_users")
tenant := q.ParamString("tenant_id", "acme")
createdAfter := q.ParamDateTime("created_after", "2026-01-01T00:00:00.000Z")
limit := q.ParamI64("limit", int64(10))
```

Parameter refs can be used in predicates, property inputs, and bounds.

## Notes

- Go v1 is dynamic-first and posts to `/v1/query` through `client.Exec`.
- Stored-query registration and bundle generation are not part of the primary Go workflow.
- Use `MarshalRequest(req)` only for tests, parity fixtures, or debugging.
- `int64` values serialize as JSON numbers; response decoding uses `json.Decoder.UseNumber()`.
- Dynamic datetime parameters serialize as RFC3339 UTC strings with millisecond precision.
- Dynamic JSON cannot represent bytes parameters; bytes remain valid stored property values.
