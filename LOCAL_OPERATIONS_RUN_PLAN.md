# Local Operations Run Plan

This checklist verifies the v2-only local CLI flow after the remodel. It focuses on local project scaffolding, local runtime lifecycle, dynamic query execution, cleanup commands, dashboard commands, and local-only CLI output quality.

## Scope

Included:

- `helix init`
- `helix add local`
- `helix run`
- `helix stop`
- `helix restart`
- `helix status`
- `helix logs`
- `helix query`
- `helix prune`
- `helix delete`
- `helix dashboard`
- `helix metrics`
- CLI help and welcome output

Excluded:

- Enterprise Cloud `auth`, workspace/project/cluster selection, `sync`, Enterprise logs, and Enterprise query execution.
- `helix update`, because it mutates the installed CLI binary.
- `helix feedback`, unless browser-opening side effects are acceptable.

## Environment Setup

Use isolated paths so the test run does not touch real user credentials, metrics, update cache, or project files.

```bash
RUN_ID=$(date +%Y%m%d%H%M%S)
RUN_ROOT="/var/folders/pt/xbkgvvss6ybcw4d30r26g5cr0000gn/T/opencode/helix-local-${RUN_ID}"
HOME="$RUN_ROOT/home"
HELIX_HOME="$HOME/.helix"
PROJECT_DEFAULT="$RUN_ROOT/default-scaffold-${RUN_ID}"
PROJECT="$RUN_ROOT/runtime-project-${RUN_ID}"
HELIX_BIN="/Users/xav/GitHub/helix-db/target/debug/helix"

mkdir -p "$HOME" "$RUN_ROOT"
```

Run all CLI commands with the isolated environment:

```bash
HOME="$HOME" HELIX_HOME="$HELIX_HOME" "$HELIX_BIN" <command>
```

## Preflight Checklist

- [ ] Build the debug CLI binary.

```bash
cargo build -p helix-cli
```

- [ ] Confirm Docker or Podman is installed and running.

```bash
docker info
```

- [ ] If Docker is unavailable, confirm Podman is installed and running.

```bash
podman info
```

- [ ] Confirm test ports are free: `18080`, `18081`, `18082`, `13000`.
- [ ] Confirm no stale containers conflict with the test names.

Expected container names:

- `helix-runtime-project-${RUN_ID}-dev`
- `helix-runtime-project-${RUN_ID}-qa`
- `helix-runtime-project-${RUN_ID}-delete-me`
- `helix-dashboard`

## Automated Verification Checklist

These checks should pass before manual local runtime testing.

- [ ] Validate workspace metadata.

```bash
cargo metadata --no-deps --format-version 1
```

- [ ] Check formatting.

```bash
cargo fmt --check
```

- [ ] Check CLI compilation.

```bash
cargo check -p helix-cli
```

- [ ] Run CLI tests.

```bash
cargo test -p helix-cli
```

- [ ] Run clippy.

```bash
cargo clippy -p helix-cli -- -D warnings
```

## Command Checklist

Record each command result in the final report tables at the bottom of this file.

### 1. Help And Welcome

- [ ] Run help.

```bash
HOME="$HOME" HELIX_HOME="$HELIX_HOME" "$HELIX_BIN" --help
```

Expected:

- [ ] Help renders cleanly.
- [ ] `build` is absent.
- [ ] `compile` is absent.
- [ ] `check` is absent.
- [ ] `push` is present for Enterprise only, not local v2 build/deploy.
- [ ] `migrate` is absent.
- [ ] `backup` is absent.
- [ ] `start` is absent.

- [ ] Run welcome output.

```bash
HOME="$HOME" HELIX_HOME="$HELIX_HOME" "$HELIX_BIN"
```

Expected:

- [ ] Welcome output renders cleanly.
- [ ] Next steps mention `helix init`.
- [ ] Next steps mention `helix run dev`.
- [ ] Next steps mention `helix query dev --file request.json`.
- [ ] Next steps mention `helix auth login`.
- [ ] Spacing and alignment are readable in TTY and non-TTY contexts.

### 2. Status Outside Project

- [ ] Run status outside a Helix project.

```bash
HOME="$HOME" HELIX_HOME="$HELIX_HOME" "$HELIX_BIN" status
```

Expected:

- [ ] Message is clear: not in a Helix project directory.
- [ ] Message suggests `helix init`.
- [ ] Record exit code.

UX review:

- [ ] Decide whether returning success for this case is acceptable.

### 3. Default Scaffold

- [ ] Initialize a default project.

```bash
HOME="$HOME" HELIX_HOME="$HELIX_HOME" "$HELIX_BIN" init --path "$PROJECT_DEFAULT"
```

Expected:

- [ ] Command succeeds.
- [ ] Output uses v2 local next steps.
- [ ] Output has clean spacing around success message and instructions.
- [ ] `$PROJECT_DEFAULT/helix.toml` exists.
- [ ] `$PROJECT_DEFAULT/.helix/` exists.
- [ ] `$PROJECT_DEFAULT/.gitignore` exists.
- [ ] `$PROJECT_DEFAULT/examples/request.json` exists.
- [ ] No `.hx` files exist.
- [ ] No `queries.rs` exists.
- [ ] No `config.hx.json` exists.

- [ ] Re-run init on the same directory.

```bash
HOME="$HOME" HELIX_HOME="$HELIX_HOME" "$HELIX_BIN" init --path "$PROJECT_DEFAULT"
```

Expected:

- [ ] Command fails.
- [ ] Error clearly says `helix.toml` already exists.
- [ ] Error includes the project path.

### 4. Runtime Project Scaffold

- [ ] Initialize a local runtime project on a non-default port.

```bash
HOME="$HOME" HELIX_HOME="$HELIX_HOME" "$HELIX_BIN" init --path "$PROJECT" local --name dev --port 18080
```

Expected:

- [ ] Command succeeds.
- [ ] `helix.toml` contains local `dev` config.
- [ ] `dev.port` is `18080`.
- [ ] `dev.image` is `ghcr.io/helixdb/enterprise-dev`.
- [ ] `dev.tag` is `latest`.
- [ ] No v1/HQL scaffold files exist.

### 5. Add Local Instance

- [ ] Add a second local instance.

```bash
cd "$PROJECT"
HOME="$HOME" HELIX_HOME="$HELIX_HOME" "$HELIX_BIN" add local --name qa --port 18081
```

Expected:

- [ ] Command succeeds.
- [ ] `helix.toml` contains local `qa` config.
- [ ] `qa.port` is `18081`.

- [ ] Try adding the same instance again.

```bash
HOME="$HOME" HELIX_HOME="$HELIX_HOME" "$HELIX_BIN" add local --name qa --port 18081
```

Expected:

- [ ] Command fails.
- [ ] Error clearly says instance `qa` already exists.

### 6. Status Before Runtime Start

- [ ] Show status before any local container exists.

```bash
HOME="$HOME" HELIX_HOME="$HELIX_HOME" "$HELIX_BIN" status
```

Expected:

- [ ] Project name is shown.
- [ ] Project root is shown.
- [ ] `dev (local)` is shown as `not created`.
- [ ] `qa (local)` is shown as `not created`.
- [ ] Fields align clearly.

### 7. Background Runtime Lifecycle

- [ ] Start `dev` in the background.

```bash
HOME="$HOME" HELIX_HOME="$HELIX_HOME" "$HELIX_BIN" run dev
```

Expected:

- [ ] Runtime availability check succeeds.
- [ ] Image pull succeeds.
- [ ] Container starts.
- [ ] Readiness check succeeds.
- [ ] In-memory storage warning is shown.
- [ ] Output includes URL `http://localhost:18080`.
- [ ] Output includes container name.
- [ ] Spacing between warning, operation success, and details is readable.

UX review:

- [ ] Confirm success text does not say `runned`.
- [ ] If it says `runned`, fix `past_tense("Running")` to return `Started` or `Ran`.

- [ ] Show status after start.

```bash
HOME="$HOME" HELIX_HOME="$HELIX_HOME" "$HELIX_BIN" status
```

Expected:

- [ ] `dev` shows running state.
- [ ] `dev` shows `http://localhost:18080`.
- [ ] `qa` remains `not created`.

### 8. Dynamic Query Success Paths

- [ ] Send the example read query.

```bash
HOME="$HOME" HELIX_HOME="$HELIX_HOME" "$HELIX_BIN" query dev --file examples/request.json
```

Expected:

- [ ] Request is sent to `POST /v1/query`.
- [ ] JSON response is pretty-printed.
- [ ] Command exits successfully.

- [ ] Send compact query.

```bash
HOME="$HOME" HELIX_HOME="$HELIX_HOME" "$HELIX_BIN" query dev --file examples/request.json --compact
```

Expected:

- [ ] JSON response is one line.
- [ ] Command exits successfully.

- [ ] Send warm read query.

```bash
HOME="$HOME" HELIX_HOME="$HELIX_HOME" "$HELIX_BIN" query dev --file examples/request.json --warm
```

Expected:

- [ ] Command succeeds.
- [ ] `X-Helix-Warm: true` is accepted for read request.

### 9. Dynamic Query Validation Failures

- [ ] Query with a missing file.

```bash
HOME="$HOME" HELIX_HOME="$HELIX_HOME" "$HELIX_BIN" query dev --file missing-request.json
```

Expected:

- [ ] Command fails before HTTP send.
- [ ] Error includes filename.
- [ ] Error includes read failure details.

- [ ] Create invalid JSON and query it.

```bash
printf '{ invalid json' > invalid-request.json
HOME="$HOME" HELIX_HOME="$HELIX_HOME" "$HELIX_BIN" query dev --file invalid-request.json
```

Expected:

- [ ] Command fails before HTTP send.
- [ ] Error includes filename.
- [ ] Error includes JSON parse details.

- [ ] Create uppercase request type and query it.

```bash
cp examples/request.json uppercase-request.json
# Edit uppercase-request.json so request_type is "READ".
HOME="$HOME" HELIX_HOME="$HELIX_HOME" "$HELIX_BIN" query dev --file uppercase-request.json
```

Expected:

- [ ] Command fails before HTTP send.
- [ ] Error says `request_type must be lowercase 'read' or 'write'`.

- [ ] Create write request and query it with `--warm`.

```bash
cp examples/request.json write-request.json
# Edit write-request.json so request_type is "write".
HOME="$HOME" HELIX_HOME="$HELIX_HOME" "$HELIX_BIN" query dev --file write-request.json --warm
```

Expected:

- [ ] Command fails before HTTP send.
- [ ] Error says `--warm is only valid for read requests`.

### 10. Logs

- [ ] Print local logs.

```bash
HOME="$HOME" HELIX_HOME="$HELIX_HOME" "$HELIX_BIN" logs dev
```

Expected:

- [ ] Container logs print directly.
- [ ] No extra CLI formatting corrupts log lines.

- [ ] Follow logs.

```bash
HOME="$HOME" HELIX_HOME="$HELIX_HOME" "$HELIX_BIN" logs dev --follow
```

Expected:

- [ ] Logs stream.
- [ ] Ctrl-C returns terminal to normal state.

- [ ] Try local logs with `--range`.

```bash
HOME="$HOME" HELIX_HOME="$HELIX_HOME" "$HELIX_BIN" logs dev --range
```

Expected:

- [ ] Current behavior: local range flag is ignored.

UX review:

- [ ] Consider warning that `--range` only applies to Enterprise logs.

### 11. Restart And Stop

- [ ] Restart `dev`.

```bash
HOME="$HOME" HELIX_HOME="$HELIX_HOME" "$HELIX_BIN" restart dev
```

Expected:

- [ ] Command succeeds.
- [ ] Readiness check succeeds.
- [ ] Query works after restart.

- [ ] Stop `dev`.

```bash
HOME="$HOME" HELIX_HOME="$HELIX_HOME" "$HELIX_BIN" stop dev
```

Expected:

- [ ] Container stops.
- [ ] Container is removed.
- [ ] Success output is clear.

- [ ] Stop `dev` again.

```bash
HOME="$HOME" HELIX_HOME="$HELIX_HOME" "$HELIX_BIN" stop dev
```

Expected:

- [ ] Command is idempotent.
- [ ] Record whether success wording is misleading when no container existed.

### 12. Second Instance Lifecycle

- [ ] Start `qa` in the background.

```bash
HOME="$HOME" HELIX_HOME="$HELIX_HOME" "$HELIX_BIN" run qa
```

Expected:

- [ ] Container starts on `18081`.
- [ ] Details show `http://localhost:18081`.

- [ ] Query `qa`.

```bash
HOME="$HOME" HELIX_HOME="$HELIX_HOME" "$HELIX_BIN" query qa --file examples/request.json
```

Expected:

- [ ] Query succeeds through configured `qa` port.

- [ ] Stop `qa`.

```bash
HOME="$HOME" HELIX_HOME="$HELIX_HOME" "$HELIX_BIN" stop qa
```

Expected:

- [ ] Container stops and is removed.

### 13. Foreground Runtime Lifecycle

Use two terminals.

Terminal A:

```bash
cd "$PROJECT"
HOME="$HOME" HELIX_HOME="$HELIX_HOME" "$HELIX_BIN" run dev --port 18082
```

Expected:

- [ ] Runs in foreground.
- [ ] Output says `Running in foreground. Press Ctrl-C to stop.`
- [ ] Container uses `--rm` semantics.

Terminal B:

```bash
cd "$PROJECT"
HOME="$HOME" HELIX_HOME="$HELIX_HOME" "$HELIX_BIN" query dev --file examples/request.json --port 18082
```

Expected:

- [ ] Query succeeds against foreground instance.

Terminal A:

- [ ] Press Ctrl-C.

Expected:

- [ ] Foreground command exits cleanly.
- [ ] Container is removed.

After Ctrl-C:

```bash
HOME="$HOME" HELIX_HOME="$HELIX_HOME" "$HELIX_BIN" status
```

Expected:

- [ ] `dev` is not created, exited, or otherwise clearly not running.

UX review:

- [ ] `status` still shows configured port `18080`, not temporary override `18082`; decide whether this is acceptable.

### 14. Prune

- [ ] Start `dev` in the background again.

```bash
HOME="$HOME" HELIX_HOME="$HELIX_HOME" "$HELIX_BIN" run dev
```

- [ ] Prune one instance.

```bash
HOME="$HOME" HELIX_HOME="$HELIX_HOME" "$HELIX_BIN" prune dev
```

Expected:

- [ ] Container is removed.
- [ ] `.helix/dev` workspace is removed if present.
- [ ] `helix.toml` still contains `dev`.

- [ ] Prune all.

```bash
HOME="$HOME" HELIX_HOME="$HELIX_HOME" "$HELIX_BIN" prune --all
```

Expected:

- [ ] Warning is clear.
- [ ] Confirmation prompt is clear.
- [ ] All local instance containers/workspaces are removed after confirmation.

- [ ] Optional: prune unused runtime resources.

```bash
HOME="$HOME" HELIX_HOME="$HELIX_HOME" "$HELIX_BIN" prune
```

Expected:

- [ ] Current behavior runs Docker/Podman `system prune -f`.

UX review:

- [ ] Consider narrowing this command to Helix resources or adding much clearer warning text.

### 15. Delete

- [ ] Add a delete test instance.

```bash
HOME="$HOME" HELIX_HOME="$HELIX_HOME" "$HELIX_BIN" add local --name delete-me --port 18082
```

- [ ] Start it.

```bash
HOME="$HOME" HELIX_HOME="$HELIX_HOME" "$HELIX_BIN" run delete-me
```

- [ ] Delete it.

```bash
HOME="$HOME" HELIX_HOME="$HELIX_HOME" "$HELIX_BIN" delete delete-me
```

Expected:

- [ ] Warning is clear.
- [ ] Confirmation prompt is clear.
- [ ] Container is stopped and removed.
- [ ] `delete-me` is removed from `helix.toml`.
- [ ] `.helix/delete-me` is removed if present.

- [ ] Delete a missing instance.

```bash
HOME="$HOME" HELIX_HOME="$HELIX_HOME" "$HELIX_BIN" delete missing
```

Expected:

- [ ] Command fails clearly.
- [ ] Error says instance is not found.

### 16. Dashboard

- [ ] Start `dev` if not already running.

```bash
HOME="$HOME" HELIX_HOME="$HELIX_HOME" "$HELIX_BIN" run dev
```

- [ ] Show dashboard status before start.

```bash
HOME="$HOME" HELIX_HOME="$HELIX_HOME" "$HELIX_BIN" dashboard status
```

Expected:

- [ ] Current behavior may print no output when dashboard is not running.

UX review:

- [ ] Consider printing `Dashboard not running`.

- [ ] Start dashboard.

```bash
HOME="$HOME" HELIX_HOME="$HELIX_HOME" "$HELIX_BIN" dashboard start --host localhost --helix-port 18080 --port 13000
```

Expected:

- [ ] Dashboard image pull succeeds.
- [ ] Dashboard container starts.
- [ ] Output shows `Dashboard started at http://localhost:13000`.

- [ ] Show dashboard status after start.

```bash
HOME="$HOME" HELIX_HOME="$HELIX_HOME" "$HELIX_BIN" dashboard status
```

Expected:

- [ ] Output shows `helix-dashboard` container state.
- [ ] Raw Docker/Podman output is readable.

- [ ] Stop dashboard.

```bash
HOME="$HOME" HELIX_HOME="$HELIX_HOME" "$HELIX_BIN" dashboard stop
```

Expected:

- [ ] Dashboard container is removed.
- [ ] Success output is clear.

- [ ] Stop dashboard again.

```bash
HOME="$HOME" HELIX_HOME="$HELIX_HOME" "$HELIX_BIN" dashboard stop
```

Expected:

- [ ] Command is idempotent.

UX review:

- [ ] Record whether `Dashboard stopped` is misleading if no dashboard existed.

### 17. Metrics

- [ ] Show metrics status.

```bash
HOME="$HOME" HELIX_HOME="$HELIX_HOME" "$HELIX_BIN" metrics status
```

Expected:

- [ ] Metrics status renders cleanly.
- [ ] Metrics level is shown.
- [ ] Last updated text is understandable.

UX review:

- [ ] Verify `Last updated` is not incorrectly displaying an epoch timestamp as “seconds ago”.

- [ ] Enable basic metrics.

```bash
HOME="$HOME" HELIX_HOME="$HELIX_HOME" "$HELIX_BIN" metrics basic
```

Expected:

- [ ] Command succeeds.
- [ ] Output is concise and clear.

- [ ] Disable metrics.

```bash
HOME="$HOME" HELIX_HOME="$HELIX_HOME" "$HELIX_BIN" metrics off
```

Expected:

- [ ] Command succeeds.
- [ ] Output is concise and clear.

- [ ] Optional: enable full metrics.

```bash
HOME="$HOME" HELIX_HOME="$HELIX_HOME" "$HELIX_BIN" metrics full
```

Expected:

- [ ] Email prompt is clear.
- [ ] Invalid email retry output is clear.
- [ ] Valid email succeeds.

## Verbosity Checklist

Run representative commands in quiet and verbose modes.

- [ ] Quiet init failure only prints essential error.

```bash
HOME="$HOME" HELIX_HOME="$HELIX_HOME" "$HELIX_BIN" --quiet init --path "$PROJECT_DEFAULT"
```

- [ ] Quiet status/query output is not overly noisy.
- [ ] Verbose run prints useful pull/runtime detail.

```bash
HOME="$HOME" HELIX_HOME="$HELIX_HOME" "$HELIX_BIN" --verbose run dev
```

- [ ] Verbose output timing is readable.
- [ ] Non-TTY output does not contain spinner artifacts.

## Cleanup Checklist

- [ ] Stop local instances.

```bash
cd "$PROJECT"
HOME="$HOME" HELIX_HOME="$HELIX_HOME" "$HELIX_BIN" stop dev
HOME="$HOME" HELIX_HOME="$HELIX_HOME" "$HELIX_BIN" stop qa
```

- [ ] Stop dashboard.

```bash
HOME="$HOME" HELIX_HOME="$HELIX_HOME" "$HELIX_BIN" dashboard stop
```

- [ ] Remove any stale test containers if necessary.

```bash
docker rm -f "helix-runtime-project-${RUN_ID}-dev" "helix-runtime-project-${RUN_ID}-qa" "helix-runtime-project-${RUN_ID}-delete-me" helix-dashboard
```

- [ ] Remove run root if no longer needed.

```bash
rm -rf "$RUN_ROOT"
```

## UX Review Checklist

For every command, review these output qualities:

- [ ] Exit code matches success/failure expectation.
- [ ] Error messages include relevant file, instance, port, or container name.
- [ ] Success messages use correct grammar.
- [ ] Blank lines before and after success messages are consistent.
- [ ] Detail lines align consistently.
- [ ] Bullets, labels, and values are easy to scan.
- [ ] Warnings are visually distinct but not noisy.
- [ ] Prompts clearly state destructive effects.
- [ ] Logs are not polluted by CLI formatting.
- [ ] Non-TTY output remains readable.
- [ ] `--quiet` suppresses nonessential output.
- [ ] `--verbose` adds useful detail without clutter.

## Known Output Issues To Watch

- [ ] Background `helix run` should print `Started 'dev' successfully`; if it prints `runned`, update output tense handling.
- [ ] `helix status` outside a project currently prints a message and returns success; decide if it should return nonzero.
- [ ] `helix dashboard status` may print nothing when dashboard is not running; consider explicit empty state.
- [ ] `helix dashboard stop` may report success even when no dashboard existed; consider a distinct “not running” message.
- [ ] `helix logs dev --range` ignores `--range` for local logs; consider a warning or validation error.
- [ ] `helix prune` without arguments runs broad Docker/Podman `system prune -f`; consider narrowing scope or adding stronger warning text.
- [ ] `helix metrics status` may label an absolute timestamp as “seconds ago”; verify and fix if needed.
- [ ] `helix run --port` override is temporary, but `status` still shows configured port; decide if this needs clearer messaging.

## End Report Template

### Summary

| Area | Passed | Failed | Notes |
| --- | ---: | ---: | --- |
| Help/welcome |  |  |  |
| Scaffold/init |  |  |  |
| Local config/add |  |  |  |
| Runtime lifecycle |  |  |  |
| Query |  |  |  |
| Logs/status |  |  |  |
| Prune/delete |  |  |  |
| Dashboard |  |  |  |
| Metrics |  |  |  |
| Output/UX |  |  |  |

### Failures

| Command | Expected | Actual | Exit Code | Likely Cause | Recommended Fix |
| --- | --- | --- | ---: | --- | --- |
|  |  |  |  |  |  |

### UX Notes

| Command | Observation | Severity | Suggested Improvement |
| --- | --- | --- | --- |
|  |  |  |  |

### Cleanup Results

| Resource | Result | Notes |
| --- | --- | --- |
| Local containers |  |  |
| Dashboard container |  |  |
| Temporary project files |  |  |
| Isolated home/config |  |  |
