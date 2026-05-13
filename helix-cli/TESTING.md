# Helix CLI Testing Guide

Use an isolated temporary project and home directory when testing local runtime behavior. From the repository root, use `cargo run -p helix-cli -- <args>` or the built `target/debug/helix` binary.

## Interactive Prompt Flows

- In a real terminal, run `helix init`; verify it prompts for local vs Enterprise and still creates the selected project type.
- In a project with multiple local instances, run `helix run`, `helix stop`, `helix restart`, and `helix logs`; verify each prompts for an instance.
- In a project with multiple instances, run `helix status`; verify it prompts for all instances or a single instance.
- Run `helix add`; verify it prompts for local vs Enterprise and required fields.
- Run `helix prune`; verify it prompts for one local instance or all local instances.
- Run `helix workspace`, `helix project`, and `helix cluster`; verify each prompts for the relevant Enterprise Cloud selection.
- Repeat representative commands with explicit arguments in a non-TTY context; verify they do not prompt.

## Local V2 Flows

- `helix init` with default settings; verify `helix.toml`, `.helix/`, `.gitignore`, and `examples/request.json` are created.
- `helix init --path /custom/path local --name dev --port 18080`; verify the local instance is configured with the requested name and port.
- `helix add local --name qa --port 18081`; verify the second local instance is added.
- `helix add local --name qa --port 18081`; verify duplicate names fail clearly.
- `helix run dev`; verify the local container starts in the background and becomes query-ready.
- `helix query dev --file examples/request.json`; verify the scaffolded dynamic query succeeds.
- `helix query dev --file examples/request.json --compact`; verify compact JSON output.
- `helix logs dev`; verify local container logs are printed.
- `helix logs dev --range`; verify local range filters are rejected with a clear Enterprise-only message.
- `helix status`; verify local instances show their runtime state.
- `helix restart dev`; verify the instance restarts and becomes query-ready again.
- `helix stop dev`; verify the background local container is removed.
- `helix stop dev`; verify repeated stop reports that the instance was not running.
- `helix prune dev`; verify Helix-owned local runtime state for that instance is removed.
- `helix prune --all --yes`; verify all local instance runtime state is removed.
- `helix delete qa --yes`; verify the instance is removed from `helix.toml` and local runtime state is cleaned.
- `helix dashboard status`; verify a not-running dashboard is reported clearly.
- `helix dashboard start --host localhost --helix-port 18080 --port 13000`; verify the dashboard starts.
- `helix dashboard stop`; verify the dashboard container is removed.
- `helix metrics full`, `helix metrics basic`, `helix metrics off`, and `helix metrics status`; verify telemetry settings and status output.

## Enterprise Cloud Flows

- `helix auth login`; verify credentials are stored.
- `helix auth logout`; verify credentials are cleared.
- `helix auth create-key <cluster-id>`; verify an API key is created for the requested cluster.
- `helix init enterprise --name production --cluster-id <cluster-id> --gateway-url <url>`; verify Enterprise config is written.
- `helix add enterprise --name staging --cluster-id <cluster-id> --gateway-url <url>`; verify Enterprise config is added.
- `helix workspace list`; verify accessible workspaces render.
- `helix project list`; verify projects render for the selected workspace.
- `helix cluster list`; verify Enterprise clusters render.
- `helix push production`; verify an Enterprise query Cargo project compiles, produces `queries.json`, and deploys to the configured cluster.
- `helix push dev`; verify local v2 instances are rejected with a clear `helix run` suggestion.
- `helix sync production`; verify Enterprise source snapshots are reconciled and metadata is synced into `helix.toml`.
- `helix sync production --yes`; verify non-interactive reconciliation can proceed without prompts.
- `helix logs production --range`; verify Enterprise historical logs are queried.
- `helix query production --file examples/request.json`; verify the command fails clearly if `gateway_url` or query auth configuration is missing.

## Error Scenarios

- `helix status` outside a project; verify it exits nonzero with a project configuration error.
- `helix init` in a directory with existing `helix.toml`; verify it fails clearly.
- `helix query dev --file missing.json`; verify missing query files fail clearly.
- `helix query dev --file invalid.json`; verify invalid JSON fails clearly.
- `helix query dev --file write.json --warm`; verify `--warm` is rejected for write requests.
- `helix run dev --foreground`; verify attached mode runs in the foreground and Ctrl-C stops the container.
- `helix run dev` without Docker or Podman running; verify runtime availability errors are clear.
