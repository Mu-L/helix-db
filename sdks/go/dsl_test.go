package helix

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync"
	"testing"
)

type findUsersResponse struct {
	Users []struct {
		ID   json.Number `json:"$id"`
		Name string      `json:"name"`
	} `json:"users"`
}

func findUsers(tenantID string, limit int64) Request {
	q := ReadQuery("find_users")
	tenant := q.ParamString("tenant_id", tenantID)
	maxRows := q.ParamI64("limit", limit)
	return q.VarAs("users", G().NWithLabel("User").Where(PredEq("tenantId", tenant)).Limit(maxRows).ValueMap("$id", "name", "tenantId")).Returning("users")
}

func TestDynamicRequestJSON(t *testing.T) {
	body, err := MarshalRequest(findUsers("acme", 25))
	if err != nil {
		t.Fatal(err)
	}
	jsonText := string(body)
	for _, want := range []string{`"request_type":"read"`, `"query_name":"find_users"`, `"tenant_id":"acme"`, `"limit":25`, `"parameter_types":{"limit":"I64","tenant_id":"String"}`} {
		if !strings.Contains(jsonText, want) {
			t.Fatalf("request JSON missing %s in %s", want, jsonText)
		}
	}
}

func TestReadQueryRejectsWriteTraversal(t *testing.T) {
	req := ReadQuery("bad").VarAs("created", G().AddN("User", Props{Prop("name", "Alice")})).Returning("created")
	if err := req.Validate(); err == nil {
		t.Fatal("expected read query to reject write traversal")
	}
}

func TestClientExec(t *testing.T) {
	var capturedPath string
	var capturedAuth string
	var capturedWriter string
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		capturedPath = r.URL.Path
		capturedAuth = r.Header.Get("Authorization")
		capturedWriter = r.Header.Get("x-helix-require-writer")
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"users":[{"$id":9223372036854775807,"name":"Alice"}]}`))
	}))
	defer server.Close()
	client, err := NewClient(server.URL, WithAPIKey("hx_secret"))
	if err != nil {
		t.Fatal(err)
	}
	var out findUsersResponse
	if err := client.Exec(context.Background(), findUsers("acme", 25), &out, WriterOnly()); err != nil {
		t.Fatal(err)
	}
	if capturedPath != "/v1/query" {
		t.Fatalf("unexpected path %s", capturedPath)
	}
	if capturedAuth != "Bearer hx_secret" || capturedWriter != "true" {
		t.Fatalf("headers not set: auth=%q writer=%q", capturedAuth, capturedWriter)
	}
	if got := out.Users[0].ID.String(); got != "9223372036854775807" {
		t.Fatalf("large id lost precision: %s", got)
	}
}

func TestClientAPIKeyMutationIsRaceSafe(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"users":[]}`))
	}))
	defer server.Close()

	client, err := NewClient(server.URL, WithAPIKey("initial"))
	if err != nil {
		t.Fatal(err)
	}

	var wg sync.WaitGroup
	errs := make(chan error, 8)
	wg.Add(1)
	go func() {
		defer wg.Done()
		for i := 0; i < 2000; i++ {
			if i%2 == 0 {
				client.WithAPIKey("updated")
			} else {
				client.ClearAPIKey()
			}
		}
	}()

	for i := 0; i < 4; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			for j := 0; j < 50; j++ {
				var out findUsersResponse
				if err := client.Exec(context.Background(), findUsers("acme", 1), &out); err != nil {
					select {
					case errs <- err:
					default:
					}
					return
				}
			}
		}()
	}
	wg.Wait()
	close(errs)
	if err := <-errs; err != nil {
		t.Fatal(err)
	}
}
