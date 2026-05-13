# Helix CLI Enterprise Cloud Testing Plan

## Scope

This plan covers the v2-only Enterprise Cloud CLI paths that remain after the local/runtime remodel:

- Authentication with `helix auth`.
- Workspace, project, and cluster selection with `helix workspace`, `helix project`, and `helix cluster`.
- Enterprise instance metadata in `helix.toml`.
- Enterprise query project deployment with `helix push`.
- Source snapshot reconciliation and metadata refresh with `helix sync`.
- Historical Enterprise logs with `helix logs --range`.
- Dynamic query execution with `helix query` when gateway URL and runtime auth are configured.

Out of scope:

- Local v2 runtime lifecycle. Use `helix-cli/TESTING.md` for local runtime coverage.
- Local v1 source upload, generated Rust, image build, or local deployment flow.
- Backend provisioning tests outside the CLI contract.

## Environment

- A test user that can run `helix auth login`.
- At least one accessible Enterprise workspace.
- At least one Enterprise cluster linked to a project.
- A known gateway URL if runtime dynamic query execution is being tested.
- The runtime query auth header and credential environment variable required by the gateway.

## Auth Checks

- `helix auth login` stores credentials under the isolated Helix config directory.
- `helix auth logout` clears stored credentials.
- `helix auth create-key <cluster-id>` creates and prints a usable API key.
- Commands that require auth fail clearly when credentials are missing.

## Config Checks

- `helix workspace list` renders accessible workspaces.
- `helix workspace show` renders the currently selected workspace.
- `helix workspace switch <workspace>` updates the selected workspace.
- `helix project list` renders projects for the selected workspace.
- `helix project show` renders the linked project.
- `helix project switch <project>` updates `helix.toml` when run inside a project.
- `helix cluster list` renders Enterprise clusters for the selected workspace/project.
- JSON output modes return valid JSON where supported.

## Enterprise Project Checks

- `helix init enterprise --name production --cluster-id <cluster-id> --gateway-url <url>` writes an Enterprise instance to `helix.toml`.
- `helix add enterprise --name staging --cluster-id <cluster-id> --gateway-url <url>` adds a second Enterprise instance.
- Duplicate Enterprise instance names fail clearly.
- Missing or empty `cluster_id` fails validation clearly.
- `helix status` renders Enterprise instances with cluster ID and gateway status.

## Sync Checks

- `helix push production` compiles the Enterprise query Cargo project, requires generated `queries.json`, snapshots allowed source files, and deploys to the configured Enterprise cluster.
- `helix push <local-instance>` fails clearly and suggests `helix run`.
- `helix sync production` reconciles Enterprise source snapshots and refreshes workspace, project, gateway, and Enterprise cluster metadata in `helix.toml` when the backend provides it.
- `helix sync production --yes` can run non-interactively when reconciliation requires confirmation.
- Missing backend metadata is handled with a clear error or documented fallback.
- Access denied responses render clear authorization errors.
- Invalid or stale cluster IDs render actionable errors.
- Sync regenerates `queries.json` after pulling Enterprise query project source.

## Logs Checks

- `helix logs production --range` queries the default one-hour historical range.
- `helix logs production --range --start <rfc3339> --end <rfc3339>` queries the requested range.
- `helix logs production --follow` fails clearly because live Enterprise logs are not supported yet.
- Backend errors include enough context to identify auth, cluster, or time-range problems.

## Query Checks

- `helix query production --file examples/request.json` sends the request to `<gateway_url>/v1/query`.
- Missing `gateway_url` fails clearly and suggests `helix sync` or setting `gateway_url` in `helix.toml`.
- Missing query auth environment variable fails clearly and names the required variable.
- Invalid runtime auth header values fail clearly before sending.
- `request_type` must be lowercase `read` or `write`.
- `--warm` is accepted only for read requests.
- HTTP error responses include the runtime status and response body.

## Exit Criteria

- Auth, config, push, sync, logs, and query error paths are verified against staging.
- Dynamic Enterprise query execution is either verified with a documented backend contract or explicitly marked blocked by gateway URL/auth contract availability.
- No Enterprise CLI path depends on removed local v1 image build or local deployment behavior.
