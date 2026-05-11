# V2-Only CLI Remodel Plan

## Summary

Remodel the repository and CLI around two supported targets:

- V2 local development using the prebuilt `ghcr.io/helixdb/enterprise-dev` image.
- Enterprise Cloud workflows.

Remove all v1 local, v1 cloud, HQL, generated Rust, and custom Docker image build/deploy paths. Dynamic query execution through `POST /v1/query` becomes the local development query path. The CLI should no longer require a `push -> compile -> build -> run` loop.

## Decisions

- Add `helix run` as the primary local v2 command.
- Remove `helix start`; `run` replaces local start semantics.
- Keep `helix stop`, because `helix run --detach` needs an explicit stop command.
- Make `helix run` foreground by default and stop the local container on Ctrl-C.
- Add `helix run --detach` for background local development.
- Keep dynamic queries only for the initial local v2 workflow.
- Do not mount `queries.json` by default.
- Do not support `.hx`, HQL, or generated Rust in the CLI.
- Remove `helix push` entirely. Enterprise Cloud should not use push semantics for v2 dynamic-query usage.
- Keep `auth`, `init`, `add`, `config`, `sync`, and `logs` for Enterprise Cloud.
- Remove v1 standard cloud flows, Fly.io flows, ECR flows, and Docker registry publishing.
- Remove `helix backup`; it is LMDB/v1-specific and local v2 has no supported persistent backup flow.
- Extract shared Enterprise Cloud control-plane helpers before deleting old integration modules.
- Treat `helix sync` as Enterprise Cloud metadata/config sync only unless a concrete v2 artifact is defined.
- Enterprise Cloud dynamic query support requires a backend contract for gateway URL and runtime query auth.
- Remodel telemetry from compile/deploy events into run/query/sync events if telemetry remains enabled.

## Supported Workflows

### Local V2

```bash
helix init
helix run dev
helix query dev --file request.json
helix logs dev
helix status
helix stop dev
```

The local runtime uses:

```text
image: ghcr.io/helixdb/enterprise-dev
default port: 8080
query endpoint: POST /v1/query
storage: development-only, in-memory, wiped on container restart
```

The CLI should print the in-memory data warning whenever it starts the local v2 image.

Local v2 does not have a supported backup or persistent volume lifecycle in this remodel. `prune`, `delete`, and `stop` should be described as container lifecycle cleanup, not durable database management.

### Enterprise Cloud

```bash
helix auth login
helix init enterprise
helix add enterprise --name production
helix config workspace ...
helix config project ...
helix sync production
helix logs production
```

Enterprise Cloud support remains focused on:

- Authentication.
- Workspace/project/cluster selection.
- Enterprise cluster configuration.
- Syncing Enterprise Cloud metadata and local `helix.toml` config.
- Enterprise Cloud logs.
- Dynamic query execution once runtime gateway URL and auth details are known.

Enterprise Cloud support must not depend on v1 `.hx` files, v1 runtime config, generated Rust, or custom Docker images.

Before implementing Enterprise Cloud `helix query`, confirm the backend contract for:

- Runtime gateway URL discovery.
- Runtime query authentication header and credential source.
- Whether control-plane `helix_user_key` is valid for runtime queries or a separate cluster/query key is required.
- Whether query URLs are per-cluster, per-project, or workspace-scoped.

## Command Surface

### Keep And Remodel

| Command | New behavior |
| --- | --- |
| `helix init` | Create a v2 project. Supports local default and Enterprise Cloud setup. No HQL scaffold. |
| `helix add` | Add a local v2 instance or Enterprise Cloud instance. |
| `helix run` | Pull and run `ghcr.io/helixdb/enterprise-dev`. Foreground by default, `--detach` available. |
| `helix stop` | Stop detached local v2 containers. |
| `helix restart` | Restart detached local v2 containers. |
| `helix status` | Show local v2 and Enterprise Cloud instance status. |
| `helix logs` | Show local v2 container logs or Enterprise Cloud logs. |
| `helix query` | Send dynamic query JSON to local or Enterprise Cloud `/v1/query`. |
| `helix auth` | Keep for Enterprise Cloud. |
| `helix config` | Keep for local config and Enterprise Cloud workspace/project/cluster selection. |
| `helix sync` | Keep for Enterprise Cloud. Remove v1 source assumptions. |
| `helix dashboard` | Keep if compatible with v2 endpoints. Default local port becomes `8080`. |
| `helix update` | Keep. |
| `helix metrics` | Keep if telemetry remains desired. |
| `helix feedback` | Keep. |

### Remove

| Command | Reason |
| --- | --- |
| `helix build` | Tied to v1 HQL compilation and Docker image builds. |
| `helix compile` | Tied to HQL and stored-query generation. |
| `helix check` | Tied to HQL parsing and generated Rust checks. |
| `helix push` | Tied to v1 local/cloud deploy semantics and image publishing. |
| `helix migrate` | v1 is no longer supported. |
| `helix backup` | Tied to v1 LMDB files. Local v2 dev storage is in-memory. |
| Fly/ECR commands | Out of scope for v2 local plus Enterprise Cloud. |

If a future Enterprise Cloud deployment command is needed, add a new explicit command instead of reusing `push`, for example `helix enterprise deploy`. Do not bring back compile/build/push semantics.

## Local Runtime Design

Replace the current build-oriented Docker manager with a local v2 runtime manager.

The new manager should support:

- Docker and Podman availability checks.
- Pulling `ghcr.io/helixdb/enterprise-dev`.
- Running a named local instance.
- Foreground mode with log streaming and Ctrl-C shutdown.
- Detached mode with `helix stop` and `helix restart` support.
- Status and log inspection.
- Prune/delete of local container artifacts.
- A readiness check after container start.

It should not support:

- Dockerfile generation.
- Docker image builds.
- Docker image tags for project instances.
- Docker image pushes.
- Helix repository cache cloning.
- Generated `helix-container` workspaces.

Foreground mode should not use a restart policy. Prefer `docker run --rm` / `podman run --rm`, or an equivalent compose flow that guarantees Ctrl-C stops and removes the foreground container.

Detached mode can use compose and a restart policy:

```yaml
services:
  helix:
    image: ghcr.io/helixdb/enterprise-dev:latest
    restart: unless-stopped
    ports:
      - "8080:8080"
```

Do not set `PATH_TO_QUERIES` in the default local path. Dynamic queries work without stored routes.

Container names should be stable and derived from project plus instance, for example `helix-<project>-<instance>`. Avoid the old v1 `_app` suffix unless there is a compatibility reason.

## Dynamic Query Command

Add `helix query` for dynamic query requests.

Minimum initial interface:

```bash
helix query dev --file request.json
helix query dev --file request.json --warm
helix query dev --file request.json --host localhost --port 8080
```

Initial behavior:

- Resolve the selected local or Enterprise Cloud instance.
- Read a full dynamic request envelope from JSON.
- Send it to `POST /v1/query`.
- Add `X-Helix-Warm: true` only when `--warm` is set.
- Pretty-print JSON responses by default.
- Return a nonzero exit code on HTTP or API errors.

Local resolution:

- Use `http://localhost:<local.port>/v1/query` unless `--host` or `--port` overrides it.

Enterprise Cloud resolution:

- Use the selected Enterprise instance `gateway_url`.
- Apply the runtime query auth header from Enterprise config or credentials.
- Fail with a clear message if `gateway_url` or runtime auth is missing and suggest `helix sync <instance>`.

Canonical request shape:

```json
{
  "request_type": "read",
  "query": {
    "queries": [
      {
        "Query": {
          "name": "node_count",
          "steps": ["Count"],
          "condition": null
        }
      }
    ],
    "returns": ["node_count"]
  },
  "parameters": {}
}
```

Validation rules:

- `request_type` must be lowercase `read` or `write`.
- `query` must be one inline batch object, not a full `queries.json` bundle.
- Dynamic parameters are untagged JSON values.
- AST property literals inside query steps remain tagged property values.
- `DateTime` and typed arrays require `parameter_types`.
- `--warm` is valid only for read requests.

Do not add HQL-to-dynamic translation in this remodel.

## Config Remodel

The config should represent only v2 local and Enterprise Cloud.

Suggested `helix.toml`:

```toml
[project]
name = "my-app"
container_runtime = "docker"

[local.dev]
port = 8080
image = "ghcr.io/helixdb/enterprise-dev"
tag = "latest"

[enterprise.production]
cluster_id = "..."
workspace_id = "..."
project_id = "..."
gateway_url = "https://..."
query_auth_header = "Authorization"
query_auth_env = "HELIX_API_KEY"
```

Remove from config:

- v1 `CloudConfig` and standard cloud config.
- Fly.io config.
- ECR config.
- `BuildMode`.
- `DbConfig::to_runtime_config`.
- `InstanceInfo::to_legacy_json`.
- `config.hx.json` compatibility.
- HQL query directory semantics.

Keep or add:

- `ContainerRuntime`.
- `LocalInstanceConfig` for v2 local image settings.
- `EnterpriseInstanceConfig` for Enterprise Cloud cluster metadata.
- Workspace/project IDs required by Enterprise Cloud APIs.
- Enterprise runtime gateway URL and query auth metadata.

## Repository And Workspace Cleanup

Remove these workspace members:

- `helix-db`
- `helix-container`
- `helix-macros`
- `hql-tests`

Keep:

- `helix-cli`
- `metrics`, if telemetry remains desired

Update:

- Root `Cargo.toml` workspace members.
- Root `Cargo.lock`.
- CLI package dependencies.
- Release workflows and package metadata.

Remove from `helix-cli/Cargo.toml`:

- `helix-db = { path = "../helix-db" }`
- CLI features that proxy `helix-db` features.
- HQL/compiler-related dependencies.
- `helix-enterprise-ql`, unless a separate Enterprise Cloud stored-route workflow is explicitly retained later.

## CLI Code Changes

### Delete

- `helix-cli/src/commands/build.rs`
- `helix-cli/src/commands/compile.rs`
- `helix-cli/src/commands/check.rs`
- `helix-cli/src/commands/migrate.rs`
- `helix-cli/src/commands/backup.rs`
- v1 cloud deploy code paths
- Fly.io integration code
- ECR integration code
- Docker Hub and GHCR publishing integration code
- HQL helper code in `utils.rs`
- Helix repository cache logic in `project.rs`

### Rewrite

- `helix-cli/src/main.rs`
- `helix-cli/src/lib.rs`
- `helix-cli/src/config.rs`
- `helix-cli/src/docker.rs`, or replace it with `local_runtime.rs`
- `helix-cli/src/project.rs`
- `helix-cli/src/commands/init.rs`
- `helix-cli/src/commands/add.rs`
- `helix-cli/src/commands/config.rs`
- `helix-cli/src/commands/sync.rs`
- `helix-cli/src/commands/logs/*`
- `helix-cli/src/commands/status.rs`
- `helix-cli/src/commands/stop.rs`
- `helix-cli/src/commands/restart.rs`
- `helix-cli/src/commands/dashboard.rs`
- `helix-cli/src/metrics_sender.rs`, if telemetry stays

### Add

- `helix-cli/src/commands/run.rs`
- `helix-cli/src/commands/query.rs`
- `helix-cli/src/local_runtime.rs`, if splitting from `docker.rs`
- `helix-cli/src/enterprise_cloud.rs` for shared Enterprise Cloud base URL, API clients, and runtime gateway/auth resolution

## Enterprise Cloud Remodel

Enterprise Cloud remains supported, but only as v2 Enterprise Cloud.

Keep:

- `auth login/logout/create-key` if these APIs still apply.
- Workspace/project/cluster selection.
- Enterprise cluster config in `helix.toml`.
- Enterprise logs.
- Enterprise sync.
- Runtime gateway URL and query auth discovery.

Remove from Enterprise Cloud paths:

- v1 standard cloud deploy payloads.
- `.hx` file collection.
- HQL parse/analyze/generation.
- `queries.json` generation by running a Rust query project, unless product explicitly keeps stored routes later.
- Docker image build/deploy hooks.

Enterprise Cloud query execution through CLI should use the same `helix query` dynamic request path, with auth headers resolved from the selected Enterprise instance.

Before deleting `commands::integrations::helix`, move shared helpers such as cloud authority/base URL handling into `enterprise_cloud.rs`. `auth`, `config`, `sync`, `logs`, and `query` should depend on this neutral module, not on deleted deployment integrations.

Enterprise config should distinguish:

- Control-plane auth used for workspace/project/cluster APIs.
- Runtime query auth used for `POST /v1/query`.

If the backend does not yet expose runtime query auth and gateway URL in CLI APIs, implement local-only `helix query` first and gate Enterprise `helix query` behind a clear unsupported/missing-config error.

## Init And Add Behavior

### `helix init`

Default local project scaffold:

- Create `helix.toml`.
- Create `.helix/`.
- Create an optional `queries/` or `requests/` directory containing dynamic query JSON examples, not `.hx` files.
- Add `.helix/`, `target/`, and logs to `.gitignore`.
- Print next steps using `helix run`, not `helix push`.

Example next steps:

```text
1. Run 'helix run dev' to start local Helix Enterprise dev
2. Edit examples/request.json or create your own dynamic query JSON
3. Run 'helix query dev --file examples/request.json'
```

Enterprise project scaffold:

- Require `helix auth login` or prompt login.
- Run Enterprise Cloud workspace/project/cluster selection.
- Store Enterprise config.
- Do not create `.hx` files.

### `helix add`

Supported variants:

- `helix add local --name testing --port 8081`
- `helix add enterprise --name production`

Remove variants:

- standard cloud
- Fly.io
- ECR

## Logs And Status

Local logs:

- Use Docker/Podman logs for the `enterprise-dev` container.
- Support live and range modes if current implementation remains useful.

Enterprise logs:

- Keep current Enterprise Cloud log API path if still valid.
- Remove standard cloud/v1 log paths.

Status:

- Show local instance state, port, image, and endpoint.
- Show Enterprise Cloud instance metadata and remote status if API supports it.
- Do not show v1 build artifacts, image tags, or compiled query info.

## Sync

Keep `helix sync` for Enterprise Cloud, but remodel it away from v1/HQL source files.

The new sync should support:

- Pulling Enterprise Cloud cluster/project metadata into `helix.toml`.
- Reconciling selected workspace/project/cluster config.
- Resolving and saving Enterprise runtime `gateway_url`.
- Resolving and saving runtime query auth metadata or the name of the environment variable that should supply it.

The new sync should not support:

- `.hx` files.
- `queries.json` regeneration.
- HQL project validation.
- standard cloud/v1 clusters.
- Local file manifest push/pull unless a concrete v2 artifact is defined later.

If the cloud API cannot provide gateway/auth details yet, `sync` should still update available Enterprise cluster metadata and clearly report which runtime query fields are missing.

## Tests

Remove tests for:

- HQL parsing.
- HQL compile.
- Generated Rust.
- Generated `queries.rs`.
- Generated `config.hx.json`.
- Docker image builds.
- `helix build`.
- `helix push`.
- `helix backup`.
- v1 migration.
- Fly.io/ECR/v1 cloud.

Rewrite tests for:

- Minimal v2 `helix.toml` generation.
- No `.hx` scaffold from `helix init`.
- Local image config uses `ghcr.io/helixdb/enterprise-dev`.
- Default local port is `8080`.
- `helix run` foreground and detached command construction.
- Foreground `helix run` does not use restart policy and cleans up on Ctrl-C.
- Detached `helix run --detach` uses the configured restart policy.
- `helix stop` detached container behavior.
- `helix query` request URL/body/header handling.
- `helix query --warm` rejects write requests before sending.
- Enterprise Cloud auth/config/sync/log command routing remains available.
- Enterprise Cloud query fails clearly when gateway URL or runtime auth config is missing.
- Standard v1 cloud paths are gone.

Likely files to delete or rewrite:

- `helix-cli/src/tests/compile_tests.rs`
- `helix-cli/src/tests/check_tests.rs`
- `helix-cli/src/tests/init_tests.rs`
- `helix-cli/src/tests/docker_tests.rs`
- `helix-cli/src/tests/lifecycle_tests.rs`
- `helix-cli/src/tests/project_tests.rs`
- `helix-cli/src/tests/utility_tests.rs`

## CI

Remove workflows tied to deleted crates:

- `.github/workflows/hql_tests.yml`
- `.github/workflows/db_tests.yml`
- `.github/workflows/dev_instance_tests.yml`
- `.github/workflows/production_db_tests.yml`

Update workflows:

- CLI release builds.
- CLI tests.
- Clippy.
- Any packaging workflow that assumes the full old source tree is a deploy template.

Target CI commands:

```bash
cargo metadata --no-deps
cargo fmt --check
cargo check -p helix-cli
cargo test -p helix-cli
cargo clippy -p helix-cli -- -D warnings
cargo build -p helix-cli --release
```

## Documentation

Rewrite:

- `README.md`
- `helix-cli/README.md`
- CLI command docs.
- Local development docs.
- Enterprise Cloud CLI docs.

Remove or archive:

- HQL docs in this repository.
- v1 migration docs.
- Old build/push workflow docs.
- HQL compiler plans.
- HQL test docs.

New docs should show:

```bash
helix init
helix run dev
helix query dev --file request.json
```

and direct HTTP usage:

```bash
curl -X POST http://localhost:8080/v1/query \
  -H 'Content-Type: application/json' \
  --data @request.json
```

## Implementation Phases

### Phase 0: Enterprise Cloud Contract Check

- Confirm how the CLI discovers Enterprise runtime gateway URLs.
- Confirm which credential/header is accepted by Enterprise runtime `POST /v1/query`.
- Confirm whether query auth is stored in Helix Cloud, returned by `sync`, or supplied by an environment variable.
- Document unsupported Enterprise query behavior if the backend contract is not ready.

### Phase 1: Command Surface And Config Skeleton

- Remove old commands from `main.rs` and `commands/mod.rs`.
- Add `run` and `query` command stubs.
- Remodel config structs to local v2 plus Enterprise Cloud.
- Remove CLI dependency on `helix-db`.
- Extract `enterprise_cloud.rs` before deleting old integration modules.

### Phase 2: Local V2 Runtime

- Replace Docker build manager with prebuilt image manager.
- Implement `helix run` foreground mode.
- Ensure foreground mode does not use restart policy and removes/stops the container on Ctrl-C.
- Implement `helix run --detach`.
- Use restart policy only for detached mode.
- Implement `helix stop`, `restart`, `status`, and local `logs` against the new container naming.
- Add readiness checks.

### Phase 3: Dynamic Queries

- Implement `helix query` for local instances.
- Add Enterprise Cloud URL/auth resolution for `helix query`.
- Add warm request support.
- Add JSON pretty/error output.

### Phase 4: Enterprise Cloud Preservation

- Keep and simplify `auth`.
- Rewrite `init enterprise` and `add enterprise`.
- Rewrite `config` around Enterprise Cloud only.
- Rewrite `sync` around Enterprise Cloud metadata/config only.
- Rewrite Enterprise Cloud `logs`.
- Add gateway/auth resolution for Enterprise `helix query` when backend support exists.

### Phase 5: Delete V1/HQL Crates

- Remove `helix-db` from workspace.
- Remove `helix-container` from workspace.
- Remove `helix-macros` from workspace.
- Remove `hql-tests` from workspace.
- Regenerate lockfile.

### Phase 6: Tests, CI, Docs

- Delete or rewrite tests.
- Update workflows.
- Rewrite README and CLI docs.
- Run verification.

## Verification

Automated:

```bash
cargo metadata --no-deps
cargo fmt --check
cargo check -p helix-cli
cargo test -p helix-cli
cargo clippy -p helix-cli -- -D warnings
cargo build -p helix-cli --release
```

Manual local smoke:

```bash
helix init
helix run dev --detach
helix status
helix query dev --file request.json
helix logs dev
helix stop dev
```

Manual foreground smoke:

```bash
helix run dev
```

Then confirm Ctrl-C stops the local container cleanly.

Manual Enterprise Cloud smoke:

```bash
helix auth login
helix init enterprise
helix add enterprise --name production
helix config workspace list
helix config project show
helix sync production
helix logs production
```

Manual Enterprise Cloud query smoke, once runtime gateway/auth contract is available:

```bash
helix query production --file request.json
```

## Non-Goals

- No HQL support.
- No `.hx` support.
- No v1 local runtime.
- No v1 cloud runtime.
- No generated Rust query handlers.
- No local Docker image builds.
- No Docker image publishing.
- No HQL-to-dynamic-query translator.
- No stored-query local development in the initial remodel.
- No local v2 backup command in the initial remodel.
- No local v2 persistent data management in the initial remodel.
