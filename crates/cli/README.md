# Helix CLI

Command-line interface for managing Helix projects, local development instances, and Helix Cloud deployments. The v3 CLI is a runtime orchestrator: queries are validated by a running instance, with no local compile/check step.

## Commands

- `init`: initialize a project with `helix.toml` and a query example.
- `chef`: bootstrap a first Helix app with skills, docs MCP, local runtime, starter queries, seed data, and a launched coding agent.
- `add`: add a local or Helix Cloud instance to an existing project.
- `start` (alias `run`): run `ghcr.io/helixdb/helixdb:v0.0.4` in the background by default, attached with `--foreground`, with persistent local storage using `--disk`, or against a user-managed S3/S3-compatible prefix using `--storage-uri`.
- `stop` / `restart` / `status`: manage local instances and inspect Helix Cloud config.
- `logs`: view local container logs or query Helix Cloud historical logs.
- `query`: send a query request JSON file to `POST /v2/query`.
- `push`: deploy a query project to a Helix Cloud cluster.
- `auth`: login, logout, or create a Helix Cloud API key.
- `workspace`: manage active Helix Cloud workspace selection.
- `project`: manage linked Helix Cloud project selection.
- `cluster`: list and inspect Helix Cloud clusters.
- `sync`: reconcile query project source and sync Helix Cloud metadata into `helix.toml`.
- `prune`: clean Helix-owned local containers, disk-mode volumes, and workspaces.
- `delete`: remove an instance from `helix.toml` and clean local runtime state.
- `skills`: install, update, and list Helix agent skills.
- `metrics`: manage telemetry level.
- `update`: update the CLI.
- `feedback`: send feedback to the Helix team.

Run `helix <command> --help` for command-specific flags and options.

When run in a terminal, commands with missing choices use interactive prompts powered by `cliclack`. When run in non-interactive contexts, commands do not prompt and require explicit arguments or flags.

## State directories

Helix-owned user state has one layout on every platform. Credentials,
workspace configuration, update caches, skills-update cache, and metrics live
under `$HOME/.helix` on Linux and macOS or `%USERPROFILE%\.helix` on Windows.

Set `HELIX_HOME` to use a different directory. The value is the exact state
directory, not a parent directory, and every Helix-owned state file follows it.
Set `HELIX_CACHE_DIR` separately when disposable CLI cache data, such as the
TypeScript query runtime, should live elsewhere. Project-local `.helix`
directories and external agent-skill state are unaffected by either override.

User-facing home-relative paths follow `HOME` on Linux and macOS and
`USERPROFILE` on Windows before falling back to platform discovery. This
includes the default `~/my-first-helix-project` created by `helix chef` and `~`
expansion in its interactive project prompt.

## Error handling

- Recoverable/library errors use `thiserror::Error` (config, project, port).
- CLI commands return `eyre::Result` and render `CliError` for consistent output.

## Testing

See [TESTING.md](TESTING.md) for the unit, mocked service, actual-binary, Docker runtime, coverage, and cross-platform CI contracts.
