use crate::InitTarget;
use crate::config::DEFAULT_LOCAL_PORT;
use crate::output::{Step, Verbosity};
use crate::prompts;
use eyre::{Result, eyre};
use serde_json::{Value, json};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const DEFAULT_PROJECT_DIR: &str = "my-first-helix-project";
const INSTANCE_NAME: &str = "dev";
const HELIX_DOCS_MCP_URL: &str = "https://docs.helix-db.com/mcp";

// add-mcp errors out non-zero when an incompatible agent is detected. Claude Desktop
// only supports local stdio servers, so an http MCP install aborts the whole run.
// Pin the agent list to the http-capable subset of `add-mcp list-agents`.
const MCP_HTTP_COMPATIBLE_AGENTS: &[&str] = &[
    "antigravity",
    "claude-code",
    "cline",
    "cline-cli",
    "codex",
    "cursor",
    "gemini-cli",
    "github-copilot-cli",
    "goose",
    "mcporter",
    "opencode",
    "vscode",
    "zed",
];

const DEFAULT_PROJECT_SPEC: &str = r#"You are building a **Personal CRM** as your default MVP because the user did not specify their own intent. Build exactly this — no extra features.

**Entities and edges:**
- `Contact` — properties: `name` (String), `email` (String), `phone` (String, optional), `createdAt` (Timestamp).
- `Company` — properties: `name` (String), `domain` (String, optional), `createdAt` (Timestamp).
- `Interaction` — properties: `kind` (String, one of `"call" | "email" | "note"`), `note` (String), `loggedAt` (Timestamp).
- `Contact -[WORKS_AT]-> Company` with property `since` (I64, year).
- `Contact -[LOGGED]-> Interaction`.

**Queries to write (one JSON file each under `examples/`):**
1. `examples/seed.json` — replace the existing User seed with 3 Companies, 5 Contacts (each linked to a Company via WORKS_AT), and 6 Interactions (each linked to a Contact via LOGGED). Use one `ForEach` block per entity type or combine them.
2. `examples/add_contact.json` — write request, params `name`, `email`, optional `phone`. Returns the created contact id.
3. `examples/add_interaction.json` — write request, params `contactId` (I64), `kind` (String), `note` (String). Creates the Interaction and the LOGGED edge from contact to interaction.
4. `examples/list_contacts.json` — read request, no params. Returns up to 50 contacts as `{id, name, email, phone}`.
5. `examples/contacts_at_company.json` — read request, param `company` (String, company name). Returns contacts at that company.
6. `examples/interactions_for_contact.json` — read request, param `contactId` (I64). Returns the contact's interactions ordered by `loggedAt` desc, limited to 10.
7. `examples/search_contacts.json` — read request, param `q` (String). Returns up to 25 contacts whose `name` starts with `q`. Use `NWhere` for the label, then `Where` with `StartsWith` for the prefix match.

**Frontend (`web/index.html`):**
- Top section: "Add contact" form (name, email, phone).
- Middle section: contact list with a search box. The box calls `search_contacts.json` on input; "Refresh" calls `list_contacts.json`.
- Each contact row has a "View" button that opens a detail panel showing the contact's Company (if any) and recent interactions, plus an "Add interaction" form (kind dropdown, note textarea).
- Results rendered as `<pre>` blocks or simple cards. No framework, no build step.

**Demo flow the user should be able to click through end to end:**
1. Add a Contact.
2. Search for that contact by partial name.
3. Open the contact, see their interactions (empty initially).
4. Add an interaction (`kind: "call"`, `note: "discussed Q3 roadmap"`).
5. Refresh the contact's detail panel; the new interaction appears."#;

const AGENT_PROMPT_TEMPLATE: &str = r#"# HelixDB MVP Builder

<role>
You are a HelixDB expert. The user just ran `helix chef` to bootstrap a new project. Your job: take the build intent below and ship a working MVP — a small set of dynamic JSON queries plus a tiny vanilla HTML/JS frontend that demonstrates them. Be persistent. Don't stop until every query you wrote returns valid JSON when run against the local DB and the demo flow works in a browser.
</role>

<environment>
`helix chef` already did all of this — do NOT redo any of it:

- Created `helix.toml` with a local instance named `dev` on port `8080`.
- Started the local DB (`helix run dev`). It is running in the background, in-memory.
- Seeded 3 example `User` nodes via `examples/seed.json`.
- Opened the dashboard at http://localhost:3000.
- Installed the HelixDB skills (`helix-query-json-dynamic`, `helix-query-authoring`, `helix-query-optimize`, `helix-query-from-gremlin`, `helix-query-from-cypher`). Invoke them when authoring queries — they are authoritative.
- Installed the Helix docs MCP (`helixdb-docs`). Query it when you need syntax details this prompt does not cover.

Existing files you must read before touching:
- `helix.toml` — project config. Do not edit.
- `examples/seed.json` — example write request that seeds Users via `ForEach` over `parameters.data`. Use it as the template for your own seed/write requests.
- `examples/read_users.json` — example read request that lists Users. Use it as the template for your own read requests.

This project uses **JSON dynamic queries only**. Never write Rust `.hx` files; there is no compile step. Every query is a JSON file under `examples/` that you send with `helix query dev --file examples/<name>.json`.
</environment>

<user_intent>
{intent}
</user_intent>

<workflow>
1. **Sketch entities and edges.** Helix has no schema file; labels and properties come into existence the first time you write them. Pick singular labels (`Contact`, not `contacts`). Pick edge labels that read as verbs (`WORKS_AT`, `LOGGED`). Write the sketch as a comment block at the top of `SCHEMA.md`.
2. **Write the seed query** at `examples/seed.json`, replacing the existing User seed. Use `ForEach` over `parameters.data` (`{"Array": "Object"}`) for bulk inserts. See `<patterns>` for the shape.
3. **Run the seed:** `helix query dev --file examples/seed.json`. If it errors, read the error, fix the JSON, retry. Do not move on until it returns `{"created": [...]}` (or whatever you named the returned variable).
4. **Write each read/write query** in its own file under `examples/`. Name them after what they do: `list_contacts.json`, `add_interaction.json`, etc. Test each one with `helix query dev --file examples/<name>.json` as you go.
5. **Wire the queries into `web/index.html`:** vanilla HTML, one `<script>` block, no build step, no framework. One section per write query, one panel per read query. See `<frontend>`.
6. **Open `web/index.html`** in a browser, click through every flow, confirm data appears. Loop on bugs.

If `helix query dev` returns an error: tail logs with `helix logs dev --follow` in another shell, read the error, fix, retry. If state gets corrupted in-memory mode, `helix restart dev` wipes everything and you can re-seed.
</workflow>

<json_dsl_quickref>
Every request has this envelope:

```json
{
  "request_type": "read" | "write",
  "query": {
    "queries": [
      { "Query":   { "name": "...", "steps": [...], "condition": null } },
      { "ForEach": { "param": "data", "body": [ ...queries ] } }
    ],
    "returns": ["name1", "name2"]
  },
  "parameters": { "key": <bare json> },
  "parameter_types": { "key": "String" | "I64" | "F64" | "Bool" | "DateTime" | {"Array": "String"} | {"Array": "Object"} }
}
```

- `request_type` is **lowercase**: `"read"` or `"write"`.
- `parameters` values are bare JSON (`"name": "Ada"`, NOT `"name": {"String": "Ada"}`).
- Inside the `query` AST, literals are tagged: `{"String": "Ada"}`, `{"I64": 42}`, `{"F64": 3.14}`, `{"Bool": true}`, `{"DateTime": "2026-05-18T12:00:00Z"}`.

**Sources** (must be the first step in any `Query`):
- `{"NWhere": <SourcePredicate>}` — nodes by indexed predicate. Example: `{"NWhere": {"Eq": ["$label", {"String": "Contact"}]}}`.
- `{"EWhere": <SourcePredicate>}` — edges by indexed predicate.
- `{"N": {"Ids": [42]}}` / `{"N": {"Param": "ids"}}` / `{"N": {"Var": "stored"}}` — nodes by id.
- `{"E": ...}` — edges by id (same NodeRef-style variants).
- `{"VectorSearchNodes": {"label": "Doc", "property": "embedding", "query_vector": {"Expr": {"Param": "vec"}}, "k": {"Literal": 10}, "tenant_value": null}}`.
- `{"TextSearchNodes": {"label": "Doc", "property": "body", "query_text": {"Expr": {"Param": "q"}}, "k": {"Literal": 10}, "tenant_value": null}}`.

`SourcePredicate` allows the **same JSON as `Predicate`** for: `Eq`, `Neq`, `Gt`, `Gte`, `Lt`, `Lte`, `Between`, `HasKey`, `StartsWith`, `And`, `Or`. It does NOT allow `Contains`, `IsIn`, `IsNull`, `IsNotNull`, `EndsWith`, `Not`, `Compare`. Push those into a `Where` step after the source.

**Traversal** (between source and terminal):
- `{"Out": "WORKS_AT"}` / `{"Out": null}` — outgoing nodes (edge label or any).
- `{"In": "WORKS_AT"}` / `{"In": null}` — incoming nodes.
- `{"Both": "WORKS_AT"}` / `{"Both": null}` — either direction.
- `{"OutE": "WORKS_AT"}` / `{"InE": ...}` / `{"BothE": ...}` — switch to edges.
- `"OutN"` / `"InN"` / `"OtherN"` — from an edge stream back to a node endpoint.

**Filters** (post-source):
- `{"Where": <Predicate>}` — full predicate set. Use this (not `NWhere`) for parameterized comparisons.
- `{"Has": ["prop", {"String": "v"}]}` — literal equality shorthand.
- `{"HasLabel": "Contact"}`, `{"HasKey": "phone"}`.
- `"Dedup"` — drop duplicates (do this after multi-hop).
- `{"Limit": 25}`, `{"Skip": 10}`, `{"Range": [0, 25]}`.

**Ordering:**
- `{"OrderBy": ["loggedAt", "Desc"]}` — `"Desc"` or `"Asc"`.
- `{"OrderByMultiple": [["priority", "Desc"], ["name", "Asc"]]}`.

**Mutations** (only inside `"request_type": "write"`):
- `{"AddN": {"label": "Contact", "properties": [["name", {"Expr": {"Param": "name"}}], ["createdAt", {"Expr": "Timestamp"}]]}}` — at source position (no source step needed before it).
- `{"AddE": {"label": "WORKS_AT", "to": {"Param": "companyId"}, "properties": [["since", {"Expr": {"Param": "since"}}]]}}` — after a node source/traversal, attaches edge from current node to `to`.
- `{"SetProperty": ["name", {"Expr": {"Param": "newName"}}]}` — overwrite a property on the current stream.
- `"Drop"` — delete current nodes/edges.
- `{"DropEdge": {"Param": "targetId"}}` — drop all edges from current nodes to target.

**Terminals** (shape the result):
- `"Count"` → integer.
- `"Exists"` → boolean.
- `{"Values": ["name", "email"]}` → list of arrays.
- `{"ValueMap": ["$id", "name", "email"]}` → list of objects keyed by those names. `{"ValueMap": null}` returns all properties.
- `{"Project": [<Projection>, ...]}` — entries are **untagged objects** disambiguated by field shape:
  - PropertyProjection: `{"source": "name", "alias": "name"}` or `{"source": "$id", "alias": "id"}` (both fields required).
  - ExprProjection: `{"alias": "ageNext", "expr": {"Add": [{"Property": "age"}, {"Constant": {"I64": 1}}]}}`.

**Predicates** (used in `Where`; subset in `NWhere`/`EWhere` per above):
- `{"Eq": ["prop", <PropertyValue>]}`, `{"Neq": ...}`, `{"Gt": ...}`, `{"Gte": ...}`, `{"Lt": ...}`, `{"Lte": ...}`, `{"Between": ["prop", <PV>, <PV>]}`.
- `{"StartsWith": ["name", "A"]}`, `{"EndsWith": [...]}`, `{"Contains": ["body", "needle"]}` (last two are post-source only).
- `{"HasKey": "phone"}`, `{"IsNull": "deletedAt"}`, `{"IsNotNull": "deletedAt"}` (post-source only).
- `{"And": [<pred>, ...]}`, `{"Or": [<pred>, ...]}`, `{"Not": <pred>}`.
- **Parameterized comparison** (use in `Where`, NOT `NWhere`):
  ```json
  {"Compare": {"left": {"Property": "email"}, "op": "Eq", "right": {"Param": "email"}}}
  ```
  `op` is `"Eq" | "Neq" | "Gt" | "Gte" | "Lt" | "Lte"`. The `Expr` shapes: `{"Property": "name"}`, `{"Param": "name"}`, `{"Constant": {"I64": 1}}`, `"Id"`, `"Timestamp"`, `{"Add": [...]}` etc.

**`PropertyInput`** (right-hand side of mutation properties, vector inputs):
- `{"Value": <PropertyValue>}` — literal, e.g. `{"Value": {"String": "Ada"}}`.
- `{"Expr": <Expr>}` — runtime expression, e.g. `{"Expr": {"Param": "name"}}` or `{"Expr": "Timestamp"}`.

**Virtual fields** (use anywhere a property name is expected):
- `$id` — element id.
- `$label` — element label.
- `$from` / `$to` — edge source / target ids.
- `$distance` — vector/BM25 search distance. **Project it immediately after the search step**, before any `Out`/`In`/`Both` — traversal drops it.

When you need anything beyond this cheat sheet (`Repeat`, `Union`, `Choose`, `Coalesce`, `Optional`, `BatchCondition`, `AggregateBy`, expression math) — invoke the `helix-query-json-dynamic` skill or query the `helixdb-docs` MCP. Do not guess.
</json_dsl_quickref>

<patterns>

**1. Create one node (write request):**
```json
{
  "request_type": "write",
  "query": {
    "queries": [{"Query": {
      "name": "created",
      "steps": [
        {"AddN": {
          "label": "Contact",
          "properties": [
            ["name",      {"Expr": {"Param": "name"}}],
            ["email",     {"Expr": {"Param": "email"}}],
            ["createdAt", {"Expr": "Timestamp"}]
          ]
        }},
        {"ValueMap": ["$id", "name", "email", "createdAt"]}
      ],
      "condition": null
    }}],
    "returns": ["created"]
  },
  "parameters": {"name": "Ada Lovelace", "email": "ada@example.com"},
  "parameter_types": {"name": "String", "email": "String"}
}
```

**2. Bulk seed via `ForEach` (write request):**
```json
{
  "request_type": "write",
  "query": {
    "queries": [{"ForEach": {
      "param": "data",
      "body": [{"Query": {
        "name": "created",
        "steps": [{"AddN": {
          "label": "Contact",
          "properties": [
            ["name",  {"Expr": {"Param": "name"}}],
            ["email", {"Expr": {"Param": "email"}}]
          ]
        }}],
        "condition": null
      }}]
    }}],
    "returns": ["created"]
  },
  "parameters": {"data": [
    {"name": "Ada",   "email": "ada@example.com"},
    {"name": "Grace", "email": "grace@example.com"}
  ]},
  "parameter_types": {"data": {"Array": "Object"}}
}
```
Inside the `ForEach` body, each object's fields (`name`, `email`) are scoped as params for the inner query.

**3. Create an edge between two existing nodes by id (write request):**
```json
{
  "request_type": "write",
  "query": {
    "queries": [{"Query": {
      "name": "linked",
      "steps": [
        {"N": {"Param": "contactIds"}},
        {"AddE": {
          "label": "WORKS_AT",
          "to": {"Param": "companyId"},
          "properties": [["since", {"Expr": {"Param": "since"}}]]
        }}
      ],
      "condition": null
    }}],
    "returns": ["linked"]
  },
  "parameters": {"contactIds": [1], "companyId": [2], "since": 2024},
  "parameter_types": {"contactIds": {"Array": "I64"}, "companyId": {"Array": "I64"}, "since": "I64"}
}
```
`NodeRef::Param` and `AddE.to` both take an **array of ids** parameter (typed `{"Array": "I64"}`).

**4. Indexed lookup by a literal value (read request):**
```json
{
  "request_type": "read",
  "query": {
    "queries": [{"Query": {
      "name": "contact",
      "steps": [
        {"NWhere": {"And": [
          {"Eq": ["$label", {"String": "Contact"}]},
          {"Eq": ["email",  {"String": "ada@example.com"}]}
        ]}},
        {"ValueMap": ["$id", "name", "email", "createdAt"]}
      ],
      "condition": null
    }}],
    "returns": ["contact"]
  },
  "parameters": {}
}
```
`And` of `Eq`s at source position uses the index. **For a parameterized email**, the value must move into a post-source `Where` with `Compare`:

```json
"steps": [
  {"NWhere": {"Eq": ["$label", {"String": "Contact"}]}},
  {"Where":  {"Compare": {"left": {"Property": "email"}, "op": "Eq", "right": {"Param": "email"}}}},
  {"ValueMap": ["$id", "name", "email"]}
]
```

**5. Multi-hop traversal — contacts at a company (read request):**
```json
{
  "request_type": "read",
  "query": {
    "queries": [{"Query": {
      "name": "contacts",
      "steps": [
        {"NWhere": {"Eq": ["$label", {"String": "Company"}]}},
        {"Where":  {"Compare": {"left": {"Property": "name"}, "op": "Eq", "right": {"Param": "company"}}}},
        {"In": "WORKS_AT"},
        "Dedup",
        {"ValueMap": ["$id", "name", "email"]}
      ],
      "condition": null
    }}],
    "returns": ["contacts"]
  },
  "parameters": {"company": "Acme"},
  "parameter_types": {"company": "String"}
}
```

**6. Ordered, limited traversal — recent interactions for a contact (read request):**
```json
{
  "request_type": "read",
  "query": {
    "queries": [{"Query": {
      "name": "interactions",
      "steps": [
        {"N":     {"Param": "contactId"}},
        {"Out":   "LOGGED"},
        {"OrderBy": ["loggedAt", "Desc"]},
        {"Limit": 10},
        {"ValueMap": ["$id", "kind", "note", "loggedAt"]}
      ],
      "condition": null
    }}],
    "returns": ["interactions"]
  },
  "parameters": {"contactId": [1]},
  "parameter_types": {"contactId": {"Array": "I64"}}
}
```

**7. Prefix search (read request):**
```json
{
  "request_type": "read",
  "query": {
    "queries": [{"Query": {
      "name": "matches",
      "steps": [
        {"NWhere": {"Eq": ["$label", {"String": "Contact"}]}},
        {"Where":  {"StartsWith": ["name", "Ad"]}},
        {"Limit": 25},
        {"ValueMap": ["$id", "name", "email"]}
      ],
      "condition": null
    }}],
    "returns": ["matches"]
  },
  "parameters": {}
}
```
`StartsWith` is index-friendly but not allowed in `NWhere`; use a `Where` step right after the label scan. For a **parameterized** prefix, swap the predicate for `{"Compare": {"left": {"Property": "name"}, "op": "Eq", "right": {"Param": "q"}}}` to do exact match, or use the `helix-query-json-dynamic` skill for the parameterized-StartsWith variant.
</patterns>

<frontend>
Write a single `web/index.html`. Vanilla HTML, inline CSS, one `<script>` block, no build step, no framework. Open it directly with `file://` or any tiny static server.

Pattern:

```html
<!doctype html>
<html>
<head><meta charset="utf-8"><title>My App</title>
<style>
  body { font-family: system-ui, sans-serif; max-width: 720px; margin: 2rem auto; padding: 0 1rem; }
  section { border: 1px solid #ddd; padding: 1rem; margin-bottom: 1rem; border-radius: 6px; }
  input, button, select, textarea { font: inherit; padding: 0.4rem; margin: 0.2rem 0; }
  pre { background: #f6f6f6; padding: 0.6rem; overflow-x: auto; }
</style>
</head>
<body>
<h1>My App</h1>

<section>
  <h2>Add contact</h2>
  <input id="name"  placeholder="Name">
  <input id="email" placeholder="Email">
  <button onclick="addContact()">Add</button>
  <pre id="addResult"></pre>
</section>

<section>
  <h2>Contacts</h2>
  <button onclick="listContacts()">Refresh</button>
  <pre id="listResult"></pre>
</section>

<script>
const ENDPOINT = "http://localhost:8080/v1/query";

async function helix(body) {
  const r = await fetch(ENDPOINT, {
    method: "POST",
    headers: {"Content-Type": "application/json"},
    body: JSON.stringify(body),
  });
  return r.json();
}

async function addContact() {
  const name  = document.getElementById("name").value;
  const email = document.getElementById("email").value;
  const result = await helix({
    request_type: "write",
    query: {
      queries: [{Query: {
        name: "created",
        steps: [
          {AddN: {label: "Contact", properties: [
            ["name",      {Expr: {Param: "name"}}],
            ["email",     {Expr: {Param: "email"}}],
            ["createdAt", {Expr: "Timestamp"}],
          ]}},
          {ValueMap: ["$id", "name", "email"]},
        ],
        condition: null,
      }}],
      returns: ["created"],
    },
    parameters: {name, email},
    parameter_types: {name: "String", email: "String"},
  });
  document.getElementById("addResult").textContent = JSON.stringify(result, null, 2);
}

async function listContacts() {
  const result = await helix({
    request_type: "read",
    query: {
      queries: [{Query: {
        name: "contacts",
        steps: [
          {NWhere: {Eq: ["$label", {String: "Contact"}]}},
          {Limit: 50},
          {ValueMap: ["$id", "name", "email", "createdAt"]},
        ],
        condition: null,
      }}],
      returns: ["contacts"],
    },
    parameters: {},
  });
  document.getElementById("listResult").textContent = JSON.stringify(result, null, 2);
}
</script>
</body>
</html>
```

Generate one section per write query and one panel per read query. Render results into `<pre>` blocks during the MVP — fancier UI is scope creep. The Helix gateway sends CORS headers permissive enough for `file://` and `localhost` origins.
</frontend>

<cli_commands>
The commands you should run while building:

- `helix query dev --file examples/<name>.json` — run a saved query.
- `helix query dev --json '<inline json>'` — one-off without a file.
- `helix query dev --file examples/<name>.json --compact | jq` — inspect response shape.
- `helix logs dev --follow` — tail DB logs in another shell; ctrl-C when done.
- `helix restart dev` — wipe in-memory state. Re-run your seed file afterwards.
- `helix status dev` — sanity check that the DB is up.

Do NOT run:
- `helix init`, `helix chef`, `helix run dev`, `helix dashboard start` — already done. Re-running can fail or duplicate state.
- `helix push`, `helix sync`, `helix deploy` — V2 Cloud commands; the user is on a local DB.
- `helix prune`, `helix delete` — destructive. Only the user runs these.

When `helix query` fails, the response body (or stderr) contains the error. Common causes are in `<antipatterns>`.
</cli_commands>

<antipatterns>
- DO NOT use `"request_type": "Read"` or `"Write"` — must be lowercase `"read"` or `"write"`.
- DO NOT mix mutations (`AddN`, `AddE`, `SetProperty`, `Drop`) inside a `"request_type": "read"` request — split them.
- DO NOT use `Contains`, `IsIn`, `IsNull`, `IsNotNull`, `EndsWith`, `Not`, `Compare` inside `NWhere` / `EWhere` — they are not allowed at source position. Use a post-source `Where` step.
- DO NOT use bare `{"Eq": ["prop", {"Param": "x"}]}` — `Eq`'s right side is a `PropertyValue` (tagged literal like `{"String": "..."}`), not an `Expr`. For a parameterized comparison use `Where` + `Compare`.
- DO NOT tag values inside top-level `parameters`. Write `"name": "Ada"`, not `"name": {"String": "Ada"}`. Tagging only happens inside the `query` AST.
- DO NOT project `$distance` after `Out` / `In` / `Both` — traversal drops it. Project it immediately after the search step.
- DO NOT use the old `Project` shapes (`{"Property": "name"}` or `{"Renamed": ["$id", "id"]}`) — entries are untagged objects with `source`+`alias` or `alias`+`expr`.
- DO NOT pass a single id to `N` as a scalar — use `{"Ids": [42]}`, `{"Param": "ids"}` (param typed `{"Array": "I64"}`), or `{"Var": "stored"}`.
- DO NOT write `.hx` files or invoke `helix compile` — this project uses dynamic JSON queries only.
- DO NOT re-run `helix init` / `helix run dev` / `helix dashboard start` — already running.
- DO NOT use plural label names (`Contacts`). Convention is singular (`Contact`). Edge labels are `SCREAMING_SNAKE` verbs (`WORKS_AT`).
- DO NOT pull in React, Vue, Svelte, Tailwind, shadcn, or any npm dependency. One HTML file with inline CSS and a `<script>` block.
- DO NOT add features the user did not ask for. Build the MVP, then stop.
</antipatterns>

<deploy_imperative>
Before you end your turn, all three of these must be true:

1. Every JSON file under `examples/` runs cleanly via `helix query dev --file examples/<name>.json` and returns a JSON body (not an error).
2. `web/index.html` opens in a browser and every button you wired up works — adding data, listing it, traversing it.
3. The user can click through the demo flow described in `<user_intent>` end to end.

If any is not true: read the error, fix the query or the frontend, retry. Tail `helix logs dev --follow` if the error is opaque. Be persistent. Do not stop until the demo works.

**Final summary — print this and then stop.** The user reads only this; make it scannable. Use exactly these sections, in this order:

### What you built
One or two sentences naming the entities, edges, and what the frontend demonstrates. No marketing language.

### Files created
Bullet list of every new file (`examples/*.json`, `web/index.html`, `SCHEMA.md`, anything else). One line per file with a 3–8-word description of its purpose.

### Files modified
Bullet list of files that already existed and were changed (typically `examples/seed.json`, possibly `examples/read_users.json`). One line per file describing what changed. Empty list if you didn't modify anything.

### How to try it
One `helix query dev --file examples/<name>.json` invocation per query file (every entry from "Files created" that's a JSON request). Then a single line pointing to `web/index.html` and how to open it.

### Known gaps
Anything you couldn't finish or that's flaky. Empty list if everything works. Be honest — do not paper over broken behavior.

Nothing else after these five sections. No closing pleasantries, no offer of next steps.
</deploy_imperative>
"#;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SetupMode {
    Automatic,
    Manual,
}

#[derive(Debug)]
struct ChefOptions {
    build_intent: Option<String>,
    mode: SetupMode,
    project_dir: PathBuf,
    install_skills: bool,
    install_mcp: bool,
    install_global: bool,
    init_project: bool,
    write_queries: bool,
    run_database: bool,
    seed_data: bool,
}

pub async fn run() -> Result<()> {
    let options = collect_options()?;
    fs::create_dir_all(&options.project_dir)?;

    if options.install_skills {
        install_skills(&options.project_dir, options.mode, options.install_global)?;
    }
    if options.install_mcp {
        install_mcp(&options.project_dir, options.mode, options.install_global)?;
    }
    if options.init_project {
        init_project(&options.project_dir).await?;
    }
    write_agent_prompt(&options.project_dir, options.build_intent.as_deref())?;
    if options.write_queries {
        write_example_queries(&options.project_dir)?;
    }

    env::set_current_dir(&options.project_dir)?;

    if options.run_database {
        run_database().await?;
    }
    if options.seed_data {
        seed_starter_data().await?;
    }

    match detect_agent() {
        Some(agent) => match select_permission_mode()? {
            Some(mode) => launch_agent(agent, mode, &options.project_dir).await,
            None => print_no_agent_fallback(&options.project_dir),
        },
        None => print_no_agent_fallback(&options.project_dir),
    }

    Ok(())
}

fn collect_options() -> Result<ChefOptions> {
    let interactive = prompts::is_interactive();
    let build_intent = if interactive {
        prompts::input_optional("What do you want to build? (leave blank to skip)")?
    } else {
        None
    };
    let mode = if interactive {
        select_setup_mode()?
    } else {
        SetupMode::Automatic
    };
    let default_project_dir = default_project_dir()?;
    let project_dir = if mode == SetupMode::Manual && interactive {
        input_project_dir(&default_project_dir)?
    } else {
        default_project_dir
    };

    // The starter seed/read JSON files and the seed query target a built-in `User`
    // schema. When the user has their own build intent they will define their own
    // entities, so the User-shaped placeholders would just be misleading clutter.
    let has_intent = build_intent
        .as_deref()
        .map(str::trim)
        .is_some_and(|s| !s.is_empty());

    let mut options = ChefOptions {
        build_intent,
        mode,
        project_dir,
        install_skills: true,
        install_mcp: true,
        install_global: true,
        init_project: true,
        write_queries: !has_intent,
        run_database: true,
        seed_data: !has_intent,
    };

    if mode == SetupMode::Manual && interactive {
        options.install_skills =
            prompts::confirm("Install Helix skills with npx skills add HelixDB/skills?")?;
        options.install_mcp = prompts::confirm("Install Helix docs MCP with npx add-mcp?")?;
        if options.install_skills || options.install_mcp {
            options.install_global = prompts::confirm(
                "Install globally (~/.claude, available to every project)? Choose no for project-local install.",
            )?;
        }
        options.init_project =
            prompts::confirm("Initialize the Helix project with helix init local?")?;
        options.write_queries =
            prompts::confirm("Write the starter query JSON files (User-shaped examples)?")?;
        options.run_database = prompts::confirm("Start the local database with helix run dev?")?;
        options.seed_data = options.write_queries
            && prompts::confirm("Run the seed query to insert starter data?")?;
    }

    Ok(options)
}

fn select_setup_mode() -> Result<SetupMode> {
    Ok(cliclack::select("How should Helix set up your project?")
        .item(
            SetupMode::Automatic,
            "Automatic setup",
            "Run every setup step with defaults",
        )
        .item(
            SetupMode::Manual,
            "Manual setup",
            "Confirm or customize each setup step",
        )
        .interact()?)
}

fn input_project_dir(default: &Path) -> Result<PathBuf> {
    let default = default.display().to_string();
    let input: String = cliclack::input("Where should Helix create the project?")
        .default_input(&default)
        .placeholder(&default)
        .validate(|input: &String| {
            if input.trim().is_empty() {
                Err("project path cannot be empty")
            } else {
                Ok(())
            }
        })
        .interact()?;
    expand_home(input.trim())
}

fn default_project_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| eyre!("Cannot find home directory"))?;
    Ok(home.join(DEFAULT_PROJECT_DIR))
}

fn expand_home(path: &str) -> Result<PathBuf> {
    if path == "~" {
        return dirs::home_dir().ok_or_else(|| eyre!("Cannot find home directory"));
    }
    if let Some(rest) = path.strip_prefix("~/") {
        let home = dirs::home_dir().ok_or_else(|| eyre!("Cannot find home directory"))?;
        return Ok(home.join(rest));
    }
    Ok(PathBuf::from(path))
}

fn skills_install_args(mode: SetupMode, global: bool) -> Vec<&'static str> {
    let mut args = match mode {
        SetupMode::Automatic => vec![
            "-y",
            "skills",
            "add",
            "HelixDB/skills",
            "--skill",
            "*",
            "-y",
        ],
        SetupMode::Manual => vec!["skills", "add", "HelixDB/skills"],
    };
    if global {
        args.push("-g");
    }
    args
}

fn mcp_install_args(mode: SetupMode, global: bool) -> Vec<&'static str> {
    let mut args = match mode {
        SetupMode::Automatic => {
            let mut args = vec![
                "-y",
                "add-mcp",
                HELIX_DOCS_MCP_URL,
                "--name",
                "helixdb-docs",
                "-y",
            ];
            for agent in MCP_HTTP_COMPATIBLE_AGENTS {
                args.push("-a");
                args.push(agent);
            }
            args
        }
        SetupMode::Manual => vec!["add-mcp", HELIX_DOCS_MCP_URL, "--name", "helixdb-docs"],
    };
    if global {
        args.push("-g");
    }
    args
}

fn install_skills(project_dir: &Path, mode: SetupMode, global: bool) -> Result<()> {
    let args = skills_install_args(mode, global);
    run_external_command(
        project_dir,
        "Installing Helix skills",
        "npx",
        &args,
        mode == SetupMode::Automatic,
    )
}

fn install_mcp(project_dir: &Path, mode: SetupMode, global: bool) -> Result<()> {
    let args = mcp_install_args(mode, global);
    run_external_command(
        project_dir,
        "Installing Helix docs MCP",
        "npx",
        &args,
        mode == SetupMode::Automatic,
    )
}

async fn init_project(project_dir: &Path) -> Result<()> {
    if project_dir.join("helix.toml").exists() {
        let mut step = Step::with_messages("Initializing project", "Project already initialized");
        step.start();
        step.done();
        return Ok(());
    }

    let path_arg = project_dir.display().to_string();
    run_quietly("Initializing project", "Project initialized", || {
        crate::commands::init::run(
            Some(path_arg),
            Some(InitTarget::Local {
                name: INSTANCE_NAME.to_string(),
                port: DEFAULT_LOCAL_PORT,
                disk: false,
            }),
        )
    })
    .await
}

async fn run_database() -> Result<()> {
    run_quietly("Starting local database", "Local database started", || {
        crate::commands::run::run(Some(INSTANCE_NAME.to_string()), false, None, false)
    })
    .await
}

async fn seed_starter_data() -> Result<()> {
    run_quietly("Seeding starter data", "Seeded starter data", || {
        crate::commands::query::run(
            Some(INSTANCE_NAME.to_string()),
            Some("examples/seed.json".to_string()),
            None,
            false,
            None,
            None,
            false,
        )
    })
    .await
}

/// Run an async op behind a Step spinner with the inner command's output silenced.
///
/// `init::run` and `run::run` write through the shared `Verbosity` knob (Operation
/// headers, info/warning lines, print_details summaries). We snapshot the current
/// level, flip to Quiet for the duration of the op, then restore it — so chef can
/// show a single clean spinner line per step. `-v` users keep the detailed output.
async fn run_quietly<F, Fut>(progress: &str, completion: &str, op: F) -> Result<()>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<()>>,
{
    let original = Verbosity::current();
    let suppress = original != Verbosity::Verbose;

    let mut step = Step::with_messages(progress, completion);
    step.start();

    if suppress {
        Verbosity::set(Verbosity::Silent);
    }

    let result = op().await;

    if suppress {
        Verbosity::set(original);
    }

    match result {
        Ok(()) => {
            step.done();
            Ok(())
        }
        Err(err) => {
            step.fail();
            Err(err)
        }
    }
}

fn run_external_command(
    project_dir: &Path,
    description: &str,
    program: &str,
    args: &[&str],
    quiet: bool,
) -> Result<()> {
    let quiet = quiet && Verbosity::current() != Verbosity::Verbose;

    let mut step = Step::with_messages(description, description);
    step.start();

    if quiet {
        let output = Command::new(program)
            .args(args)
            .current_dir(project_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()?;
        if !output.status.success() {
            step.fail();
            if !output.stdout.is_empty() {
                eprintln!("{}", String::from_utf8_lossy(&output.stdout));
            }
            if !output.stderr.is_empty() {
                eprintln!("{}", String::from_utf8_lossy(&output.stderr));
            }
            return Err(eyre!("{description} failed with status {}", output.status));
        }
    } else {
        let status = Command::new(program)
            .args(args)
            .current_dir(project_dir)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()?;
        if !status.success() {
            step.fail();
            return Err(eyre!("{description} failed with status {status}"));
        }
    }

    step.done();
    Ok(())
}

pub(crate) fn write_agent_prompt(project_dir: &Path, build_intent: Option<&str>) -> Result<()> {
    let mut step = Step::with_messages("Writing agent prompt", "Wrote agent prompt");
    step.start();

    let result = fs::write(
        project_dir.join("HELIX_CHEF_PROMPT.md"),
        starter_prompt(build_intent),
    )
    .map_err(eyre::Report::from);

    match result {
        Ok(()) => {
            step.done();
            Ok(())
        }
        Err(err) => {
            step.fail();
            Err(err)
        }
    }
}

pub(crate) fn write_example_queries(project_dir: &Path) -> Result<()> {
    let mut step = Step::with_messages(
        "Writing starter query JSON files",
        "Wrote starter query JSON files",
    );
    step.start();

    let examples_dir = project_dir.join("examples");
    let result = (|| -> Result<()> {
        fs::create_dir_all(&examples_dir)?;
        fs::write(
            examples_dir.join("seed.json"),
            serde_json::to_string_pretty(&starter_seed_request())?,
        )?;
        fs::write(
            examples_dir.join("read_users.json"),
            serde_json::to_string_pretty(&starter_read_request())?,
        )?;
        Ok(())
    })();

    match result {
        Ok(()) => {
            step.done();
            Ok(())
        }
        Err(err) => {
            step.fail();
            Err(err)
        }
    }
}

fn starter_prompt(build_intent: Option<&str>) -> String {
    let intent = build_intent
        .map(str::trim)
        .filter(|intent| !intent.is_empty())
        .unwrap_or(DEFAULT_PROJECT_SPEC);
    AGENT_PROMPT_TEMPLATE.replace("{intent}", intent)
}

pub(crate) fn starter_seed_request() -> Value {
    json!({
        "request_type": "write",
        "query": {
            "queries": [
                {"ForEach": {
                    "param": "data",
                    "body": [
                        {"Query": {
                            "name": "created",
                            "steps": [
                                {"AddN": {
                                    "label": "User",
                                    "properties": [
                                        ["externalId", {"Expr": {"Param": "externalId"}}],
                                        ["name", {"Expr": {"Param": "name"}}],
                                        ["email", {"Expr": {"Param": "email"}}],
                                        ["role", {"Expr": {"Param": "role"}}],
                                        ["createdAt", {"Expr": "Timestamp"}]
                                    ]
                                }}
                            ],
                            "condition": null
                        }}
                    ]
                }}
            ],
            "returns": ["created"]
        },
        "parameters": {
            "data": [
                {"externalId": "u-1", "name": "Ada Lovelace", "email": "ada@example.com", "role": "admin"},
                {"externalId": "u-2", "name": "Grace Hopper", "email": "grace@example.com", "role": "builder"},
                {"externalId": "u-3", "name": "Katherine Johnson", "email": "katherine@example.com", "role": "analyst"}
            ]
        },
        "parameter_types": {"data": {"Array": "Object"}}
    })
}

pub(crate) fn starter_read_request() -> Value {
    json!({
        "request_type": "read",
        "query": {
            "queries": [
                {"Query": {
                    "name": "users",
                    "steps": [
                        {"NWhere": {"Eq": ["$label", {"String": "User"}]}},
                        {"Limit": 25},
                        {"ValueMap": ["$id", "externalId", "name", "email", "role", "createdAt"]}
                    ],
                    "condition": null
                }}
            ],
            "returns": ["users"]
        },
        "parameters": {}
    })
}

// ---------- Coding-agent detection and launch ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AgentKind {
    ClaudeCode,
    OpenAiCodex,
    OpenCode,
}

impl AgentKind {
    fn binary(self) -> &'static str {
        match self {
            AgentKind::ClaudeCode => "claude",
            AgentKind::OpenAiCodex => "codex",
            AgentKind::OpenCode => "opencode",
        }
    }

    fn display(self) -> &'static str {
        match self {
            AgentKind::ClaudeCode => "Claude Code",
            AgentKind::OpenAiCodex => "OpenAI Codex",
            AgentKind::OpenCode => "OpenCode",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PermissionMode {
    FullAuto,
    Scoped,
}

const AGENT_PRIORITY: &[AgentKind] = &[
    AgentKind::ClaudeCode,
    AgentKind::OpenAiCodex,
    AgentKind::OpenCode,
];

const PROMPT_FILENAME: &str = "HELIX_CHEF_PROMPT.md";
const AGENT_USER_PROMPT: &str =
    "Build the MVP described in HELIX_CHEF_PROMPT.md and stop when the demo works.";

fn detect_agent() -> Option<AgentKind> {
    AGENT_PRIORITY
        .iter()
        .copied()
        .find(|agent| crate::utils::command_exists(agent.binary()))
}

fn select_permission_mode() -> Result<Option<PermissionMode>> {
    if !prompts::is_interactive() {
        return Ok(None);
    }
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Choice {
        FullAuto,
        Scoped,
        Skip,
    }
    let choice = cliclack::select("Give the agent full autonomy?")
        .item(
            Choice::FullAuto,
            "Yes",
            "Skip permission prompts and let it finish unattended (recommended)",
        )
        .item(
            Choice::Scoped,
            "Scoped",
            "Ask before each shell command (safer, slower)",
        )
        .item(
            Choice::Skip,
            "Don't launch",
            "Just print the prompt path so I can use my own agent",
        )
        .interact()?;
    Ok(match choice {
        Choice::FullAuto => Some(PermissionMode::FullAuto),
        Choice::Scoped => Some(PermissionMode::Scoped),
        Choice::Skip => None,
    })
}

fn build_agent_argv(
    kind: AgentKind,
    mode: PermissionMode,
    prompt_file: &str,
    project_dir: &Path,
) -> Vec<String> {
    match kind {
        AgentKind::ClaudeCode => {
            let _ = project_dir;
            // -p / --print runs Claude headless instead of opening the TUI. Tool use
            // is still active; only the interactive interface is suppressed.
            // stream-json + --verbose lets us parse tool-use events live and surface
            // progress lines above the chef spinner. --verbose is required with
            // stream-json in print mode per Anthropic's CLI docs.
            let mut args = vec![
                "--append-system-prompt-file".to_string(),
                prompt_file.to_string(),
            ];
            match mode {
                PermissionMode::FullAuto => {
                    args.push("--dangerously-skip-permissions".to_string());
                }
                PermissionMode::Scoped => {
                    args.push("--permission-mode".to_string());
                    args.push("acceptEdits".to_string());
                }
            }
            args.push("--output-format".to_string());
            args.push("stream-json".to_string());
            args.push("--verbose".to_string());
            args.push("-p".to_string());
            args.push(AGENT_USER_PROMPT.to_string());
            args
        }
        AgentKind::OpenAiCodex => {
            let _ = project_dir;
            let mut args = vec!["exec".to_string()];
            match mode {
                PermissionMode::FullAuto => {
                    args.push("--dangerously-bypass-approvals-and-sandbox".to_string());
                }
                PermissionMode::Scoped => {
                    args.push("--sandbox".to_string());
                    args.push("workspace-write".to_string());
                    args.push("--ask-for-approval".to_string());
                    args.push("on-request".to_string());
                }
            }
            args.push(format!(
                "Follow the spec in ./{prompt_file}. {AGENT_USER_PROMPT}"
            ));
            args
        }
        AgentKind::OpenCode => {
            let mut args = vec![
                "run".to_string(),
                "--dir".to_string(),
                project_dir.display().to_string(),
            ];
            if matches!(mode, PermissionMode::FullAuto) {
                args.push("--dangerously-skip-permissions".to_string());
            }
            args.push(format!(
                "Follow the spec in ./{prompt_file}. {AGENT_USER_PROMPT}"
            ));
            args
        }
    }
}

// ---------- Claude stream-json event parsing ----------
//
// With `--output-format stream-json --verbose`, Claude Code emits one JSON event
// per line. We parse them into a tagged enum and surface human-readable progress
// lines (tool calls, retries) above the chef spinner via Step::println.

#[derive(Debug, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ClaudeEvent {
    System {
        #[serde(default)]
        subtype: String,
    },
    Assistant {
        message: AssistantMessage,
    },
    User {
        message: UserMessage,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, serde::Deserialize)]
struct AssistantMessage {
    #[serde(default)]
    content: Vec<ContentBlock>,
}

#[derive(Debug, serde::Deserialize)]
struct UserMessage {
    #[serde(default)]
    content: Vec<ContentBlock>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ContentBlock {
    #[serde(rename = "text")]
    Text {
        #[allow(dead_code)]
        #[serde(default)]
        text: String,
    },
    ToolUse {
        #[serde(default)]
        name: String,
        #[serde(default)]
        input: serde_json::Value,
    },
    ToolResult {
        #[serde(default)]
        is_error: bool,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, serde::Deserialize)]
struct ResultEvent {
    #[serde(default)]
    is_error: bool,
    #[serde(default)]
    duration_ms: Option<u64>,
    #[serde(default)]
    total_cost_usd: Option<f64>,
}

fn format_claude_event(event: &ClaudeEvent) -> Vec<String> {
    let mut out = Vec::new();
    match event {
        ClaudeEvent::System { subtype } if subtype == "api_retry" => {
            out.push("⟳ Retrying API call...".to_string());
        }
        ClaudeEvent::Assistant { message } => {
            for block in &message.content {
                if let ContentBlock::ToolUse { name, input } = block
                    && let Some(line) = format_tool_use(name, input)
                {
                    out.push(line);
                }
            }
        }
        ClaudeEvent::User { message } => {
            for block in &message.content {
                if let ContentBlock::ToolResult { is_error: true } = block {
                    out.push("✗ tool error".to_string());
                }
            }
        }
        _ => {}
    }
    out
}

fn format_tool_use(name: &str, input: &serde_json::Value) -> Option<String> {
    let s = |k: &str| input.get(k).and_then(|v| v.as_str());
    Some(match name {
        "Edit" => format!("✎ Editing {}", s("file_path")?),
        "Write" => format!("✎ Writing {}", s("file_path")?),
        "Read" => format!("📖 Reading {}", s("file_path")?),
        "Bash" => format!("💻 {}", s("description").or_else(|| s("command"))?),
        "Glob" => format!("🔍 Glob {}", s("pattern")?),
        "Grep" => format!("🔍 Grep {}", s("pattern")?),
        "WebSearch" => format!("🌐 Searching: {}", s("query")?),
        "WebFetch" => format!("🌐 Fetch {}", s("url")?),
        "TodoWrite" => return None,
        other if other.starts_with("mcp__") => format!("🔌 MCP: {other}"),
        other => format!("🔧 {other}"),
    })
}

/// Returns just the parenthesized "(37.2s, $0.412)" segment, or an empty string
/// when no fields are present. Empty if both `duration_ms` and `total_cost_usd`
/// are missing. The success/failure prefix is handled by the surrounding Step.
fn format_result_stats(r: &ResultEvent) -> String {
    let mut parts = Vec::new();
    if let Some(ms) = r.duration_ms {
        parts.push(format!("{:.1}s", ms as f64 / 1000.0));
    }
    if let Some(cost) = r.total_cost_usd {
        parts.push(format!("${cost:.3}"));
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!("({})", parts.join(", "))
    }
}

async fn launch_agent(kind: AgentKind, mode: PermissionMode, project_dir: &Path) {
    // Step shows an animated spinner while the agent works. For Claude, the
    // spinner message updates in place as stream-json events arrive — one line
    // total, no scroll spam. Codex / opencode stream their own text directly.
    let progress = format!("Cheffing in {}", project_dir.display());
    let completion = format!("Cheffed in {}", project_dir.display());
    let mut step = Step::with_messages(&progress, &completion);
    step.start();

    let status_result = match kind {
        AgentKind::ClaudeCode => launch_claude_streaming(mode, project_dir, &mut step).await,
        AgentKind::OpenAiCodex | AgentKind::OpenCode => {
            launch_other_inherited(kind, mode, project_dir)
        }
    };

    match status_result {
        Ok(status) if status.success() => {
            step.done();
        }
        Ok(_) => {
            step.fail();
            crate::output::warning(&format!(
                "{} exited without completing the build (see error above).",
                kind.display(),
            ));
            print_paste_prompt_hint(
                project_dir,
                "Fix the underlying issue and re-run `helix chef`, or:",
            );
        }
        Err(error) => {
            step.fail();
            crate::output::warning(&format!("Could not run {}: {error}", kind.display()));
            print_paste_prompt_hint(project_dir, "");
        }
    }
}

fn launch_other_inherited(
    kind: AgentKind,
    mode: PermissionMode,
    project_dir: &Path,
) -> Result<std::process::ExitStatus> {
    let argv = build_agent_argv(kind, mode, PROMPT_FILENAME, project_dir);
    let argv_refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    Command::new(kind.binary())
        .args(&argv_refs)
        .current_dir(project_dir)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(Into::into)
}

async fn launch_claude_streaming(
    mode: PermissionMode,
    project_dir: &Path,
    step: &mut Step,
) -> Result<std::process::ExitStatus> {
    use tokio::io::AsyncBufReadExt;
    use tokio::process::Command as TokioCommand;

    let argv = build_agent_argv(AgentKind::ClaudeCode, mode, PROMPT_FILENAME, project_dir);
    let argv_refs: Vec<&str> = argv.iter().map(String::as_str).collect();

    let mut child = TokioCommand::new(AgentKind::ClaudeCode.binary())
        .args(&argv_refs)
        .current_dir(project_dir)
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| eyre!("Claude stdout was not piped"))?;
    let mut lines = tokio::io::BufReader::new(stdout).lines();

    let dir_display = project_dir.display().to_string();
    let mut final_stats: Option<String> = None;

    while let Some(line) = lines.next_line().await? {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Ok(event) = serde_json::from_str::<ClaudeEvent>(trimmed) {
            // Two-line spinner: the first line is the static "Cheffing in <dir>"
            // header, the second line carries the latest action. Embedding `\n`
            // in indicatif's message works because the template is `{spinner} {msg}`
            // — the message body wraps onto a fresh line and gets rewritten in place
            // on each update (line count is stable, so no visual artifacts).
            // The 4-space indent on line 2 aligns under the spinner's message column.
            if let Some(rendered) = format_claude_event(&event).into_iter().last() {
                step.set_message(&format!("Cheffing in {dir_display}\n    {rendered}"));
            }
            continue;
        }
        if let Ok(result) = serde_json::from_str::<ResultEvent>(trimmed) {
            final_stats = Some(format_result_stats(&result));
        }
    }

    let status = child.wait().await?;
    if let Some(stats) = final_stats.filter(|s| !s.is_empty()) {
        step.set_completion(&format!("Cheffed in {dir_display} {stats}"));
    }
    Ok(status)
}

fn print_no_agent_fallback(project_dir: &Path) {
    let lead = format!(
        "No supported coding-agent CLI was found in PATH ({}, {}, {}).",
        AgentKind::ClaudeCode.binary(),
        AgentKind::OpenAiCodex.binary(),
        AgentKind::OpenCode.binary(),
    );
    print_paste_prompt_hint(project_dir, &lead);
}

fn print_paste_prompt_hint(project_dir: &Path, lead: &str) {
    if !lead.is_empty() {
        crate::output::info(lead);
    }
    crate::output::info(&format!(
        "Paste the contents of {} into your agent of choice to get started.",
        project_dir.join(PROMPT_FILENAME).display(),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starter_seed_request_is_write_request() {
        let request = starter_seed_request();

        assert_eq!(request["request_type"], "write");
        assert!(request["query"]["queries"][0].get("ForEach").is_some());
        assert_eq!(request["parameter_types"]["data"]["Array"], "Object");
    }

    #[test]
    fn starter_read_request_reads_users() {
        let request = starter_read_request();
        let steps = &request["query"]["queries"][0]["Query"]["steps"];

        assert_eq!(request["request_type"], "read");
        assert!(steps[0].get("NWhere").is_some());
        assert_eq!(steps[1]["Limit"], 25);
    }

    #[test]
    fn starter_prompt_includes_user_intent() {
        let prompt = starter_prompt(Some("Build a todo app"));

        assert!(prompt.contains("Build a todo app"));
        assert!(prompt.contains("<user_intent>"));
        assert!(!prompt.contains("Personal CRM"));
    }

    #[test]
    fn starter_prompt_falls_back_to_default_project() {
        let prompt = starter_prompt(None);

        assert!(prompt.contains("Personal CRM"));
        assert!(prompt.contains("Contact"));
        assert!(prompt.contains("WORKS_AT"));
    }

    #[test]
    fn starter_prompt_treats_blank_intent_as_default() {
        let prompt = starter_prompt(Some("   "));

        assert!(prompt.contains("Personal CRM"));
    }

    #[test]
    fn write_agent_prompt_creates_prompt_file() {
        let dir = env::temp_dir().join(format!(
            "helix-chef-test-prompt-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();

        write_agent_prompt(&dir, Some("Build a CRM")).unwrap();

        assert!(dir.join("HELIX_CHEF_PROMPT.md").exists());
        assert!(!dir.join("examples/seed.json").exists());
        assert!(!dir.join("examples/read_users.json").exists());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn write_example_queries_creates_seed_and_read_files() {
        let dir = env::temp_dir().join(format!(
            "helix-chef-test-examples-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();

        write_example_queries(&dir).unwrap();

        assert!(!dir.join("HELIX_CHEF_PROMPT.md").exists());
        assert!(dir.join("examples/seed.json").exists());
        assert!(dir.join("examples/read_users.json").exists());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn agent_priority_is_claude_codex_opencode() {
        assert_eq!(
            AGENT_PRIORITY,
            &[
                AgentKind::ClaudeCode,
                AgentKind::OpenAiCodex,
                AgentKind::OpenCode,
            ],
        );
        assert_eq!(AgentKind::ClaudeCode.binary(), "claude");
        assert_eq!(AgentKind::OpenAiCodex.binary(), "codex");
        assert_eq!(AgentKind::OpenCode.binary(), "opencode");
    }

    #[test]
    fn build_agent_argv_claude_full_auto() {
        let argv = build_agent_argv(
            AgentKind::ClaudeCode,
            PermissionMode::FullAuto,
            "HELIX_CHEF_PROMPT.md",
            Path::new("/tmp/proj"),
        );
        assert!(!argv.iter().any(|a| a == "--bare"));
        assert_eq!(argv[0], "--append-system-prompt-file");
        assert_eq!(argv[1], "HELIX_CHEF_PROMPT.md");
        assert!(argv.iter().any(|a| a == "--dangerously-skip-permissions"));
        assert!(!argv.iter().any(|a| a == "--permission-mode"));
        // Streaming output for progress visibility.
        assert!(argv.iter().any(|a| a == "--output-format"));
        assert!(argv.iter().any(|a| a == "stream-json"));
        assert!(argv.iter().any(|a| a == "--verbose"));
        // -p keeps Claude headless instead of launching its TUI.
        let p_index = argv.iter().position(|a| a == "-p").expect("-p present");
        assert_eq!(argv[p_index + 1], AGENT_USER_PROMPT);
        assert_eq!(argv.last().unwrap(), AGENT_USER_PROMPT);
    }

    #[test]
    fn build_agent_argv_claude_scoped() {
        let argv = build_agent_argv(
            AgentKind::ClaudeCode,
            PermissionMode::Scoped,
            "HELIX_CHEF_PROMPT.md",
            Path::new("/tmp/proj"),
        );
        assert!(!argv.iter().any(|a| a == "--bare"));
        assert!(argv.iter().any(|a| a == "--permission-mode"));
        assert!(argv.iter().any(|a| a == "acceptEdits"));
        assert!(!argv.iter().any(|a| a == "--dangerously-skip-permissions"));
        assert!(argv.iter().any(|a| a == "--output-format"));
        assert!(argv.iter().any(|a| a == "stream-json"));
        assert!(argv.iter().any(|a| a == "--verbose"));
        assert!(argv.iter().any(|a| a == "-p"));
        assert_eq!(argv.last().unwrap(), AGENT_USER_PROMPT);
    }

    #[test]
    fn build_agent_argv_codex_full_auto() {
        let argv = build_agent_argv(
            AgentKind::OpenAiCodex,
            PermissionMode::FullAuto,
            "HELIX_CHEF_PROMPT.md",
            Path::new("/tmp/proj"),
        );
        assert_eq!(argv[0], "exec");
        assert!(
            argv.iter()
                .any(|a| a == "--dangerously-bypass-approvals-and-sandbox")
        );
        assert!(!argv.iter().any(|a| a == "--sandbox"));
        assert!(argv.last().unwrap().contains("HELIX_CHEF_PROMPT.md"));
    }

    #[test]
    fn build_agent_argv_codex_scoped() {
        let argv = build_agent_argv(
            AgentKind::OpenAiCodex,
            PermissionMode::Scoped,
            "HELIX_CHEF_PROMPT.md",
            Path::new("/tmp/proj"),
        );
        assert_eq!(argv[0], "exec");
        assert!(argv.iter().any(|a| a == "--sandbox"));
        assert!(argv.iter().any(|a| a == "workspace-write"));
        assert!(argv.iter().any(|a| a == "--ask-for-approval"));
        assert!(argv.iter().any(|a| a == "on-request"));
        assert!(
            !argv
                .iter()
                .any(|a| a == "--dangerously-bypass-approvals-and-sandbox")
        );
    }

    #[test]
    fn build_agent_argv_opencode_full_auto() {
        let argv = build_agent_argv(
            AgentKind::OpenCode,
            PermissionMode::FullAuto,
            "HELIX_CHEF_PROMPT.md",
            Path::new("/tmp/proj"),
        );
        assert_eq!(argv[0], "run");
        assert_eq!(argv[1], "--dir");
        assert_eq!(argv[2], "/tmp/proj");
        assert!(argv.iter().any(|a| a == "--dangerously-skip-permissions"));
    }

    #[test]
    fn build_agent_argv_opencode_scoped() {
        let argv = build_agent_argv(
            AgentKind::OpenCode,
            PermissionMode::Scoped,
            "HELIX_CHEF_PROMPT.md",
            Path::new("/tmp/proj"),
        );
        assert_eq!(argv[0], "run");
        assert_eq!(argv[1], "--dir");
        assert!(!argv.iter().any(|a| a == "--dangerously-skip-permissions"));
    }

    #[test]
    fn skills_install_args_automatic_global() {
        let args = skills_install_args(SetupMode::Automatic, true);
        assert_eq!(args[0], "-y");
        assert!(args.contains(&"skills"));
        assert!(args.contains(&"add"));
        assert!(args.contains(&"HelixDB/skills"));
        assert_eq!(args.last(), Some(&"-g"));
    }

    #[test]
    fn skills_install_args_automatic_project_local() {
        let args = skills_install_args(SetupMode::Automatic, false);
        assert!(!args.contains(&"-g"));
        assert!(args.contains(&"HelixDB/skills"));
    }

    #[test]
    fn skills_install_args_manual_global() {
        let args = skills_install_args(SetupMode::Manual, true);
        // Manual mode skips the -y flags so the user sees CLI prompts.
        assert!(!args.contains(&"-y"));
        assert!(args.contains(&"-g"));
    }

    #[test]
    fn skills_install_args_manual_project_local() {
        let args = skills_install_args(SetupMode::Manual, false);
        assert!(!args.contains(&"-g"));
        assert!(!args.contains(&"-y"));
    }

    #[test]
    fn mcp_install_args_automatic_global() {
        let args = mcp_install_args(SetupMode::Automatic, true);
        assert_eq!(args[0], "-y");
        assert!(args.contains(&"add-mcp"));
        assert!(args.contains(&HELIX_DOCS_MCP_URL));
        assert!(args.contains(&"helixdb-docs"));
        assert!(args.contains(&"-g"));
    }

    #[test]
    fn mcp_install_args_automatic_project_local() {
        let args = mcp_install_args(SetupMode::Automatic, false);
        assert!(!args.contains(&"-g"));
        assert!(args.contains(&HELIX_DOCS_MCP_URL));
    }

    #[test]
    fn mcp_install_args_manual_global() {
        let args = mcp_install_args(SetupMode::Manual, true);
        assert!(!args.contains(&"-y"));
        assert!(args.contains(&"-g"));
    }

    #[test]
    fn mcp_install_args_manual_project_local() {
        let args = mcp_install_args(SetupMode::Manual, false);
        assert!(!args.contains(&"-g"));
        assert!(!args.contains(&"-y"));
    }

    // ---------- Claude stream-json event parsing ----------

    #[test]
    fn format_tool_use_edit() {
        let input = serde_json::json!({"file_path": "examples/seed.json", "old_string": "x", "new_string": "y"});
        assert_eq!(
            format_tool_use("Edit", &input).unwrap(),
            "✎ Editing examples/seed.json"
        );
    }

    #[test]
    fn format_tool_use_read() {
        let input = serde_json::json!({"file_path": "helix.toml"});
        assert_eq!(
            format_tool_use("Read", &input).unwrap(),
            "📖 Reading helix.toml"
        );
    }

    #[test]
    fn format_tool_use_bash_prefers_description() {
        let input =
            serde_json::json!({"command": "rm -rf /tmp/x", "description": "Clean up tmp dir"});
        assert_eq!(
            format_tool_use("Bash", &input).unwrap(),
            "💻 Clean up tmp dir"
        );
    }

    #[test]
    fn format_tool_use_bash_falls_back_to_command() {
        let input = serde_json::json!({"command": "ls -la"});
        assert_eq!(format_tool_use("Bash", &input).unwrap(), "💻 ls -la");
    }

    #[test]
    fn format_tool_use_todowrite_returns_none() {
        let input = serde_json::json!({"todos": []});
        assert!(format_tool_use("TodoWrite", &input).is_none());
    }

    #[test]
    fn format_tool_use_unknown_tool() {
        let input = serde_json::json!({});
        assert_eq!(
            format_tool_use("SomethingNew", &input).unwrap(),
            "🔧 SomethingNew"
        );
    }

    #[test]
    fn format_tool_use_mcp_tool() {
        let input = serde_json::json!({});
        assert_eq!(
            format_tool_use("mcp__helixdb-docs__search", &input).unwrap(),
            "🔌 MCP: mcp__helixdb-docs__search"
        );
    }

    #[test]
    fn parse_claude_event_assistant_with_tool_use() {
        let line = r#"{
            "type": "assistant",
            "message": {
                "content": [
                    {"type": "tool_use", "name": "Edit", "input": {"file_path": "examples/seed.json"}}
                ]
            }
        }"#;
        let event: ClaudeEvent = serde_json::from_str(line).unwrap();
        let rendered = format_claude_event(&event);
        assert_eq!(rendered, vec!["✎ Editing examples/seed.json"]);
    }

    #[test]
    fn parse_claude_event_user_with_tool_error() {
        let line = r#"{
            "type": "user",
            "message": {
                "content": [
                    {"type": "tool_result", "is_error": true}
                ]
            }
        }"#;
        let event: ClaudeEvent = serde_json::from_str(line).unwrap();
        let rendered = format_claude_event(&event);
        assert_eq!(rendered, vec!["✗ tool error"]);
    }

    #[test]
    fn parse_claude_event_system_api_retry() {
        let line = r#"{"type": "system", "subtype": "api_retry"}"#;
        let event: ClaudeEvent = serde_json::from_str(line).unwrap();
        let rendered = format_claude_event(&event);
        assert_eq!(rendered, vec!["⟳ Retrying API call..."]);
    }

    #[test]
    fn parse_claude_event_unknown_type_falls_through() {
        let line = r#"{"type": "stream_event", "event": {"type": "message_start"}}"#;
        let event: ClaudeEvent = serde_json::from_str(line).unwrap();
        assert!(matches!(event, ClaudeEvent::Other));
        assert!(format_claude_event(&event).is_empty());
    }

    #[test]
    fn parse_result_event_success() {
        let line = r#"{"type": "result", "is_error": false, "duration_ms": 37200, "total_cost_usd": 0.412}"#;
        let result: ResultEvent = serde_json::from_str(line).unwrap();
        assert!(!result.is_error);
        assert_eq!(result.duration_ms, Some(37200));
        assert_eq!(result.total_cost_usd, Some(0.412));
        let stats = format_result_stats(&result);
        assert_eq!(stats, "(37.2s, $0.412)");
    }

    #[test]
    fn parse_result_event_empty_stats_when_no_fields() {
        let line = r#"{"type": "result", "is_error": true}"#;
        let result: ResultEvent = serde_json::from_str(line).unwrap();
        assert!(result.is_error);
        assert_eq!(format_result_stats(&result), "");
    }

    #[test]
    fn format_result_stats_duration_only() {
        let result = ResultEvent {
            is_error: false,
            duration_ms: Some(1500),
            total_cost_usd: None,
        };
        assert_eq!(format_result_stats(&result), "(1.5s)");
    }
}
