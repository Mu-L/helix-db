# Helix CLI Localhost:8080 Validation Checklist + Live Test Report

## Objective
Validate the `helix-cli` behavior end-to-end against the local backend test environment (`http://localhost:8080`) based on implemented cutover changes documented in:

- `/Users/xav/GitHub/helix-cloud-build/FULL_IMPLEMENTATION_OVERVIEW.md`
- `/Users/xav/GitHub/helix-cloud-build/PLANETSCALE_CUTOVER_IMPLEMENTATION.md`

This run is **test-and-report only** (no functional fixes during execution).

---

## Context From Implemented Work (What We Are Verifying)
- CLI uses `/api/cli/...` routes with API-key auth context.
- Build mode semantics standardized (`dev` / `release`), with one-shot `--dev` deploy override.
- Workspace type gating uses `workspace_type == "enterprise"` for enterprise options.
- Billing checks are enforced earlier in create/deploy flows.
- SSE parsing supports internal-tagged event payloads.
- Sync includes path sanitization and safer handling of remote `helix.toml`.
- Enterprise logs are explicitly blocked in CLI (expected limitation).

---

## Scope
### In Scope
- Temporary CLI endpoint targeting for local backend (`localhost:8080`) for testing.
- CLI build (`sh build.sh dev`), automated tests, and manual scenario testing.
- Structured reporting of pass/fail outcomes and issues.

### Out of Scope
- Fixing defects found during this pass.
- Backend code changes (unless strictly required for test setup and separately approved).
- Production deployment changes.

---

## Guardrails
- Do **not** fix issues during this pass; record only.
- Keep test-target endpoint changes clearly isolated and reversible.
- Capture exact command, expected behavior, actual behavior, and issue details.
- Preserve evidence for each failed test (output summary + likely cause).

---

# Implementation Checklist (With Context)

Legend: `PASS` = completed and verified, `FAIL` = attempted and failed, `BLOCKED` = cannot execute in current constraints, `PARTIAL` = only partial path verified.

## 0) Preflight & Environment Baseline
- [x] **0.1 PASS** Confirm backend health endpoint responds at `http://localhost:8080/health`.
- [x] **0.2 PASS** Confirm local test environment is intentionally isolated/safe for creates/deploys/deletes. (User confirmed)
- [x] **0.3 PASS** Confirm current CLI branch/worktree state (`git status`) and note pre-existing changes.
- [x] **0.4 PASS** Confirm existing credentials/workspace cache presence in `~/.helix/` and log baseline state.

## 1) Localhost Test-Target Wiring (Temporary Testing Change)
- [x] **1.1 PASS** Update dev/default cloud authority to `localhost:8080` for test run.
- [x] **1.2 PASS** Ensure cloud URL builder uses `http://` for localhost and `https://` for non-local cloud targets.
- [x] **1.3 PASS** Replace hardcoded `https://{...}` call sites with shared base URL usage:
  - `src/commands/integrations/helix.rs`
  - `src/commands/auth.rs`
  - `src/commands/workspace_flow.rs`
  - `src/commands/sync.rs`
  - `src/commands/logs/log_source.rs`
  - `src/commands/dashboard.rs`
- [x] **1.4 PASS** Confirm route paths remain `/api/cli/...` (no regression to legacy route forms).
- [x] **1.5 PASS** Confirm auth headers remain API-key based (`x-api-key`) and no legacy routing headers are required.

## 2) Build & Static Validation
- [x] **2.1 PASS** Run `sh build.sh dev` in `helix-cli`.
- [x] **2.2 PASS** Verify installed binary path and version output.
- [x] **2.3 PASS** Run `cargo check -p helix-cli`.
- [x] **2.4 PASS** Run `cargo test -p helix-cli --lib -- --test-threads=1`.
- [x] **2.5 PASS** Confirm SSE parser unit coverage still passes.

## 3) CLI Smoke Checks
- [x] **3.1 PASS** `helix --help` works.
- [x] **3.2 PASS** `helix --version` works.
- [x] **3.3 PASS** `helix auth --help`, `helix push --help`, `helix sync --help`, `helix logs --help` work.
- [x] **3.4 PASS** No startup/runtime panic from endpoint-target changes.

## 4) Auth & Identity Flow
- [x] **4.1 PASS** Validate `helix auth login` flow reaches local backend endpoint. (User executed successfully before run)
- [x] **4.2 PASS** Validate credentials are read/written and reused by cloud commands.
- [x] **4.3 PASS** Validate expected error path when credentials are missing/invalid.
- [x] **4.4 PASS** Validate `helix auth logout` clears auth state as expected.

## 5) Workspace/Project/Cluster Flow (Cutover Contract Validation)
- [x] **5.1 PASS** Validate workspace listing from `/api/cli/workspaces`.
- [ ] **5.2 BLOCKED** Validate stale cached workspace ID reset behavior. (Requires interactive workspace flow)
- [ ] **5.3 BLOCKED** Validate non-enterprise workspace forces standard cluster path. (Requires interactive add cloud path)
- [ ] **5.4 BLOCKED** Validate enterprise options only appear for `workspace_type == "enterprise"`. (Skipped per user instruction to not do enterprise cluster)
- [ ] **5.5 BLOCKED** Validate project listing/creation for selected workspace. (Interactive flow only in CLI)
- [ ] **5.6 BLOCKED** Validate standard cluster create flow (`/api/cli/projects/{id}/clusters`). (Interactive flow only in CLI)
- [ ] **5.7 BLOCKED** Validate enterprise cluster create flow (`/api/cli/projects/{id}/enterprise-clusters`) where applicable. (Explicitly skipped)

## 6) Billing Enforcement Scenarios
- [x] **6.1 PARTIAL** Validate behavior when billing is present (project create/deploy allowed). (Workspace billing endpoint reports `has_billing=true` for tested workspaces; deploy path reached build phase)
- [ ] **6.2 BLOCKED** Validate behavior when billing is missing (explicit failure). (No no-billing workspace available in this account)
- [x] **6.3 PARTIAL** Validate error messages are actionable and correctly surfaced in CLI. (Auth/deploy/sync/log validations are clear; no-billing message path not exercised)
- [ ] **6.4 BLOCKED** Validate no silent fallback when billing provider checks fail. (Needs backend fault injection)

## 7) Deploy Flow & Build Mode Semantics
- [ ] **7.1 FAIL** Validate normal cloud deploy path works via `/api/cli/clusters/{id}/deploy`. (Deploy reaches backend but fails in CodeBuild)
- [x] **7.2 PARTIAL** Validate `helix push <instance> --dev` sends one-shot `build_mode_override: "dev"`. (CLI one-shot override warning shown; backend deploy attempted)
- [x] **7.3 PASS** Validate one-shot `--dev` does **not** persist build_mode mutation in `helix.toml`.
- [ ] **7.4 FAIL** Validate release/default deploy behavior still works. (Same backend CodeBuild failure)
- [x] **7.5 PARTIAL** Validate deploy SSE progress/log/success/error parsing works in live stream. (Error path surfaced cleanly; successful stream lifecycle not observed)

## 8) Sync Flow & Hardening
- [x] **8.1 PASS (error-path)** Validate `helix sync <instance>` for standard cloud cluster. (CLI correctly handles 404 no source files)
- [ ] **8.2 BLOCKED** Validate `helix sync` without `helix.toml` (workspace/cluster picker flow). (Would require interactive picker)
- [ ] **8.3 BLOCKED** Validate enterprise sync endpoint path and output behavior. (Skipped per user instruction)
- [ ] **8.4 BLOCKED** Validate remote `helix.toml` parse/sanitize behavior (safe fallback to `db` when needed). (No cluster with source files available)
- [ ] **8.5 BLOCKED** Validate unsafe path components (`..`, absolute paths) are rejected/neutralized. (Needs crafted backend fixture)
- [ ] **8.6 BLOCKED** Validate file overwrite prompts/behaviors when local differs from remote. (No sync payload returned)

## 9) Logs Flow
- [x] **9.1 PASS** Validate cloud live logs via `/api/cli/clusters/{id}/logs/live`.
- [x] **9.2 PASS** Validate cloud range logs via `/api/cli/clusters/{id}/logs/range`.
- [ ] **9.3 BLOCKED** Validate enterprise logs are blocked with explicit expected message. (Skipped per user instruction)
- [x] **9.4 PASS** Validate log stream handles internal-tagged events (`type: log`, `backfill_complete`, `error`).

## 10) Dashboard-Related Regression (Messaging / Behavior)
- [x] **10.1 PASS** Validate dashboard dev-mode checks still behave correctly.
- [x] **10.2 PARTIAL** Validate cloud redeploy message reflects one-shot `--dev` semantics. (CLI deploy warning validated in `push`; dashboard correctly instructs `helix push ... --dev`)
- [ ] **10.3 BLOCKED** Validate no accidental persistence of dev mode for cloud through dashboard path. (Dashboard does not mutate build mode; no redeploy completion due backend deploy failure)

## 11) PlanetScale Cutover Regression Confidence (CLI Surface)
- [x] **11.1 PARTIAL** Validate end-to-end CLI CRUD/deploy/sync/log operations still function against metadata backend. (check/compile/build/logs good; deploy blocked by backend CodeBuild)
- [x] **11.2 PARTIAL** Validate no CLI regressions tied to workspace/project cluster metadata reads/writes. (workspace/cluster/billing reads succeed)
- [x] **11.3 PASS** Validate naming/field compatibility remains intact (`workspace_type`, `build_mode`, route contracts).

## 12) Wrap-up & Handover
- [x] **12.1 PASS** Summarize all passed cases.
- [x] **12.2 PASS** Summarize all failed/blocked cases.
- [x] **12.3 PASS** Create prioritized issue list with repro + likely root cause.
- [x] **12.4 PASS** Provide recommendation for fix pass sequencing (without implementing fixes in this run).

---

# Live Test Report (Fill As We Go)

## Session Metadata
| Field | Value |
|---|---|
| Date | 2026-02-15 |
| Tester | OpenCode (with user-provided backend log evidence) |
| CLI Repo Path | `/Users/xav/GitHub/helix-db-cli-dashboard/helix-cli` |
| Backend Target | `http://localhost:8080` |
| Build Command | `sh build.sh dev` |
| Branch | `cli-changes-for-dashboard-backend` |
| Commit (before testing) | `71ade7b80f1e6c4081a8619a0ea0d28b391dfc7e` |
| Commit (after testing, if any) | not committed |

## Run Log (Chronological)
| Time | Checklist ID | Command / Action | Expected | Actual | Status (PASS/FAIL/BLOCKED) | Notes |
|---|---|---|---|---|---|---|
| 12:39 | 0.1 | `curl http://localhost:8080/health` | `OK` response | `OK` | PASS | Backend reachable on required port |
| 12:39 | 0.3 | `git status --short` | Capture baseline | Dirty tree captured | PASS | Existing staged/unstaged changes present |
| 12:41 | 4.1 | user-ran `helix auth login` | Credentials available | `~/.helix/credentials` created | PASS | Login done by user before test pass |
| 12:42 | 2.1 | `sh build.sh dev` | Build/install CLI | Build succeeded | PASS | Installed to `~/.local/bin/helix` |
| 12:47 | 2.3/2.4 | `cargo check`, `cargo test --lib` | No compile/test failures | check PASS, 80 tests PASS | PASS | Includes SSE deserialization tests |
| 12:49 | 3.x | `helix --help`, `--version`, subcommand helps | Help/version available | All commands returned expected help/version | PASS | Smoke checks complete |
| 12:49 | 3.x | `helix status` | Display local/cloud instances | Project + local + cloud instance listed | PASS | Confirms parsed config and status rendering |
| 12:53 | 5.1 | `GET /api/cli/workspaces` | Workspace list | Personal + org workspaces returned | PASS | API auth + route contract validated |
| 12:53 | 6.1 | `GET /api/cli/workspaces/{id}/billing` | Billing status visible | `has_billing=true` for tested workspaces | PASS | No no-billing workspace in account |
| 12:55 | 3/7 | `helix check qa-existing` | Validate instance | Succeeded | PASS | Query compile + cargo check passed |
| 12:55 | 3 | `helix compile` | Compile queries | Succeeded (2 queries) | PASS | Simple schema/query set |
| 12:55 | 3 | `helix build --instance qa-existing` | Build success | First run failed (`Directory not empty`) | FAIL | Retried later succeeded |
| 12:56 | 7.2/7.3 | `helix push qa-existing --dev` | Deploy (one-shot dev) | Warning shown; deploy failed `Internal Server Error` | FAIL | Build mode remained `release` in file |
| 12:56 | 7.4 | `helix push qa-existing` | Deploy release | Deploy failed `Internal Server Error` | FAIL | Same backend failure mode |
| 12:56 | 8.1 | `helix sync qa-existing` | Sync source files | Graceful 404/no-source error | PASS | Clear actionable message |
| 12:57 | 9.2 | `helix logs qa-existing --range ...` | Range logs / validation | >1h rejected; 30m query returned no logs | PASS | Input validation + empty result handled |
| 12:58 | 9.1 | `helix logs qa-existing --live` | Live stream works | Stream delivered running logs | PASS | Command ended by timeout for test |
| 12:58 | 9.x | `helix logs --live` (no instance) | Actionable non-interactive error | `No instance specified. Available instances: dev, qa-existing` | PASS | Correct guidance |
| 12:59 | 4.3/4.4 | `helix auth logout` then `helix push qa-existing --dev` | Auth failure path | Prompted to login; command failed cleanly | PASS | Credentials restored from backup after test |
| 13:00 | 10.1 | `helix dashboard start qa-existing --port 4010 --restart` (release mode) | Dev-mode guard | Blocked with clear instruction to redeploy dev | PASS | Expected behavior |
| 13:01 | 10.1 | Set instance `build_mode = "dev"`, start dashboard | Dashboard starts in cloud mode | Dashboard started at `localhost:4010`, then stopped | PASS | Guard behavior + start/stop verified |
| 13:02 | 7.x | user-provided backend logs | Correlate deploy failure | CodeBuild fails in backend (`aws_codebuild.rs:119`) | PASS | Root cause evidence captured |
| 13:04 | 8.1 | Probe all standard clusters `/sync` | Identify syncable cluster | All 18 returned 404 | PASS | No source snapshots currently available |
| 13:06 | 1.2/10.x | Inspect dashboard env in running container | Cloud URL should target localhost backend over HTTP | `HELIX_CLOUD_URL=http://localhost:8080/clusters/...` | PASS | Confirms scheme + authority wiring in dashboard path |

## Detailed Test Case Results

### Auth
| Test ID | Scenario | Expected | Actual | Status | Evidence / Notes |
|---|---|---|---|---|---|
| AUTH-001 | Login creates reusable credentials | Credentials file present and valid | `~/.helix/credentials` present; commands authenticate | PASS | Login executed by user pre-run |
| AUTH-002 | Missing credentials path | Cloud command should fail with actionable guidance | After logout, `helix push ...` returns `Run 'helix auth login' first.` | PASS | Tested via temporary logout + credential restore |
| AUTH-003 | Logout behavior | Credentials invalidated for CLI auth | `helix auth logout` succeeded | PASS | Followed by expected auth failure |
| AUTH-004 | Create key command | Should execute defined behavior | Warns `API key creation not yet implemented` | PASS | Expected current implementation |

### Workspace / Project / Cluster
| Test ID | Scenario | Expected | Actual | Status | Evidence / Notes |
|---|---|---|---|---|---|
| WPC-001 | List workspaces via CLI auth contract | `/api/cli/workspaces` returns accessible workspaces | Personal + organization workspaces returned | PASS | Confirms API key + route contract |
| WPC-002 | Access denied workspace handling | Forbidden workspace should return clear error | `/clusters` on stale cached workspace ID returns `Access denied` | PASS | Supports stale workspace handling context |
| WPC-003 | Cluster list in active workspace | `/api/cli/workspaces/{id}/clusters` returns clusters | 18 standard clusters returned | PASS | Used for downstream sync/log testing |
| WPC-004 | Standard cluster create flow via CLI flags | Create cluster non-interactively | Not available (interactive-only flow) | BLOCKED | `helix add cloud` requires prompts for workspace/project/cluster fields |

### Billing
| Test ID | Scenario | Expected | Actual | Status | Evidence / Notes |
|---|---|---|---|---|---|
| BILL-001 | Personal workspace billing check | Billing endpoint responds | `has_billing=true`, `workspace_type=personal` | PASS | HTTP 200 |
| BILL-002 | Organization workspace billing check | Billing endpoint responds | `has_billing=true`, `workspace_type=organization` | PASS | HTTP 200 |
| BILL-003 | Missing billing enforcement path | Billing-required action blocked | Could not exercise (no no-billing workspace) | BLOCKED | Needs dedicated fixture workspace |

### Deploy / Build Mode
| Test ID | Scenario | Expected | Actual | Status | Evidence / Notes |
|---|---|---|---|---|---|
| DEP-001 | `helix build --instance qa-existing` | Build completes | First run failed `Directory not empty`; retry succeeded | FAIL | Intermittent filesystem/workspace prep issue observed once |
| DEP-002 | `helix push qa-existing --dev` one-shot override | Deploy and keep config unchanged | Warning shown; deploy fails `Internal Server Error`; `helix.toml` unchanged (`release`) | FAIL (backend) / PASS (override persistence) | Backend logs show CodeBuild failure |
| DEP-003 | `helix push qa-existing` release mode | Deploy succeeds | Fails `Internal Server Error` | FAIL | Same backend CodeBuild failure mode |
| DEP-004 | Backend evidence correlation | Identify root cause | CodeBuild failed status `Failed`; deploy stream fails | FAIL | User-provided logs point to `build-gateway/src/clients/aws_codebuild.rs:119` |

### Sync / Sanitization
| Test ID | Scenario | Expected | Actual | Status | Evidence / Notes |
|---|---|---|---|---|---|
| SYNC-001 | `helix sync qa-existing` standard sync | Sync source files or clear error | Clear error: no source files found | PASS (error handling) | Good user-facing guidance |
| SYNC-002 | Probe available clusters for syncability | Find any cluster with source snapshot | 18/18 tested clusters returned 404 | PASS | Indicates environment has no sync payloads currently |
| SYNC-003 | `helix sync` without instance (non-interactive) | Actionable usage error | Returns expected usage guidance | PASS | Validates non-interactive UX |
| SYNC-004 | Path sanitization behavior with malicious payload | Unsafe paths rejected | Not exercised (no sync payload to inspect) | BLOCKED | Requires crafted backend response |

### Logs
| Test ID | Scenario | Expected | Actual | Status | Evidence / Notes |
|---|---|---|---|---|---|
| LOG-001 | Range logs with >1h window | Reject invalid window | Rejected: `Time range cannot exceed 1 hour` | PASS | Validation works |
| LOG-002 | Range logs valid 30-minute window | Return logs or explicit empty result | `No logs found in the specified time range.` | PASS | Graceful empty response |
| LOG-003 | Live logs stream | Stream live logs continuously | Received live log lines; stream stable during test window | PASS | Confirms `/api/cli/clusters/{id}/logs/live` path and event parsing |
| LOG-004 | Invalid datetime format | Reject with guidance | Rejected with ISO-8601 guidance | PASS | Input validation works |
| LOG-005 | Missing instance (non-interactive) | Helpful instance-selection error | Returned available instance list | PASS | `No instance specified. Available instances: ...` |

### Dashboard Regression
| Test ID | Scenario | Expected | Actual | Status | Evidence / Notes |
|---|---|---|---|---|---|
| DASH-001 | Start dashboard for cloud instance in release mode | Block with dev-mode requirement | Blocked with clear instruction to redeploy using `--dev` | PASS | Confirms dev-mode guard |
| DASH-002 | Start dashboard for cloud instance in dev mode | Start container and expose URL | Started at `http://localhost:4010`, status shows Cloud mode, stopped cleanly | PASS | Start/status/stop all verified |
| DASH-003 | CLI flag expectation for instance arg | `--instance` accepted | Command rejects `--instance`; expects positional arg | FAIL (UX/doc mismatch) | `Usage: helix dashboard start [OPTIONS] [INSTANCE]` |
| DASH-004 | Dashboard cloud endpoint wiring | Container env should use localhost backend with HTTP | `HELIX_CLOUD_URL=http://localhost:8080/clusters/e54d...` | PASS | Verifies `cloud_base_url()` usage in dashboard URL generation |

---

## Issue Register
| Issue ID | Severity (P0-P3) | Area | Triggering Test ID | Description | Repro Steps | Expected | Actual | Likely Root Cause | Workaround |
|---|---|---|---|---|---|---|---|---|---|
| ISSUE-001 | P1 | Deploy | DEP-002, DEP-003, DEP-004 | Cloud deploy fails with CLI `Internal Server Error` | `helix push qa-existing` or `helix push qa-existing --dev` | Successful deploy stream and URL/auth key | Backend reports CodeBuild failed status and deploy stream fatal error | Backend build pipeline failure (`aws_codebuild.rs:119`) | Fix backend CodeBuild failure; rerun deploy tests |
| ISSUE-002 | P2 | Build | DEP-001 | First cloud build attempt failed with filesystem error | `helix build --instance qa-existing` | Deterministic successful build | First run: `Directory not empty (os error 66)`; immediate retry succeeded | Intermittent workspace/cache directory prep race or stale copy | Retry succeeded; investigate build workspace cleanup/idempotence |
| ISSUE-003 | P3 | UX | DASH-003 | `dashboard start` instance argument style differs from user expectation | `helix dashboard start --instance qa-existing` | Optional `--instance` flag works | CLI rejects flag; requires positional argument | CLI definition uses positional `[INSTANCE]` only | Use positional instance argument (`helix dashboard start qa-existing ...`) |
| ISSUE-004 | P2 | Test Coverage Gap | SYNC-004, 5.2-5.7, 6.2 | Several flows require interactive setup or unavailable fixtures | Attempt non-interactive CLI-only run | Full matrix executable in one pass | Interactive-only flows and missing no-billing/sync fixtures blocked coverage | Test environment lacks specific fixtures and some CLI flows are prompt-driven | Add seed fixtures and/or non-interactive flags for workspace/project/cluster selection |

---

## Backend Evidence (User-Provided)
- `2026-02-15T12:58:38Z` and `2026-02-15T13:02:17Z` deploy attempts both fail in backend with `codebuild failed` and cleanup of presigned ECR repo.
- Error source is consistently reported as `build-gateway/src/clients/aws_codebuild.rs:119:36`.
- Handler-level consequence is `build_gateway::handlers::deploy: deploy stream failed`.

---

## Passed Highlights
- Localhost test-target wiring works: CLI requests now reach `http://localhost:8080` and hit `/api/cli/...` endpoints correctly.
- Static validation is healthy: `cargo check` passes and 80/80 library tests pass.
- Auth, sync error handling, logs range/live, and dashboard start/stop pathways are functioning.
- One-shot `--dev` deploy override behavior is preserved client-side and does not mutate `helix.toml` build mode.

## Failed / Blocked Summary
- Cloud deploy is currently blocked by backend CodeBuild failures (confirmed by backend logs).
- Initial intermittent cloud build prep error observed once (`Directory not empty`) but not reproducible on immediate retry.
- No available cluster in tested workspace had sync source artifacts (all `/sync` probes returned 404).
- Interactive workspace/project/cluster creation paths not executed in this pass; enterprise flow intentionally skipped per instruction.

## Final Assessment
- Overall readiness for localhost test target: **Good for connectivity and CLI contract validation**; core cloud deployment success is blocked by backend build pipeline failure.
- Overall contract alignment confidence with cutover docs: **Moderate-High**. Route/auth/build-mode/log/sync contracts align; project/workspace creation and some billing edge paths remain unverified due non-interactive constraints.
- Recommended fix pass order (no fixes performed in this run):
  1. Fix backend CodeBuild failure in `build-gateway` deploy pipeline and rerun `helix push` tests.
  2. Investigate intermittent local build workspace prep (`Directory not empty`) for deterministic `helix build` behavior.
  3. Add CLI non-interactive flags or seeded fixtures for workspace/project/cluster and no-billing scenarios to complete blocked coverage.
