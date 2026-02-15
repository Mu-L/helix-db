# Helix CLI Localhost:8080 E2E Retest Report

## Session Metadata
| Field | Value |
|---|---|
| Date | 2026-02-15 |
| Start Time | 16:21 GMT |
| CLI Version | Helix CLI 2.2.8 |
| CLI Binary | `/Users/xav/.local/bin/helix` |
| Backend Target | `http://localhost:8080` |
| Backend Health | `OK` |
| Test Workspace Dir | `/private/tmp/helix-cli-e2e-fresh` |
| Tester | OpenCode |

## Preflight Results
| Check | Expected | Actual | Status | Notes |
|---|---|---|---|---|
| Backend health | `OK` from `/health` | `OK` | PASS | Backend restart confirmed |
| New project clusters endpoint | `200`/`403`, not `405` | `200` for known project | PASS | Route active |
| CLI install | Updated CLI available | `helix --version` => `2.2.8` | PASS | Binary path verified |
| Fresh project creation | Empty project initialized | `helix init` succeeded | PASS | Created in `/private/tmp/helix-cli-e2e-fresh` |

## Test Run Log
| Time | Test ID | Command/Action | Expected | Actual | Status | Notes |
|---|---|---|---|---|---|---|
| 16:21 | PF-01 | `helix --version`, `which helix` | CLI present and updated | `Helix CLI 2.2.8`, path verified | PASS |  |
| 16:21 | PF-02 | `curl http://localhost:8080/health` | Backend healthy | `OK` | PASS |  |
| 16:22 | PF-03 | Enumerate workspaces/projects/clusters endpoint | Metadata endpoint available | `GET /api/cli/projects/{id}/clusters` returns cluster metadata | PASS | Endpoint is active |
| 16:22 | PF-04 | `helix init --path /private/tmp/helix-cli-e2e-fresh --template empty --queries-path ./db/` | Fresh project initialized | Init success | PASS | Clean test project ready |
| 16:25 | WS-03 | `helix sync` with no `~/.helix/config` | Prompt workspace and persist selection | Prompt shown; selected `Personal`; config written | PASS | Fresh project + multi-workspace path |
| 16:26 | WS-01 | `helix sync` with valid cached workspace id | Skip workspace prompt | Went directly to project check; no workspace prompt | PASS | Cache hit behavior verified |
| 16:27 | WS-02 | `helix sync` with stale cached workspace id | Warn + reprompt + persist new workspace | Warning shown; workspace reselected; cache updated to valid id | PASS | `workspace_id` restored to personal workspace |
| 16:30 | PR-01 | `project.name = proj` then `helix sync` | Existing project auto-used | Printed `Using project 'proj'...`; attempted cluster sync | PASS | Sync then failed 404 due no source files for that cluster |
| 16:31 | PR-02 | `project.name = e2e-create-same` then `helix sync` create flow | Create missing project with same name | Created project `e2e-create-same`; `helix.toml` name unchanged | PASS | No clusters in new project (expected follow-up message) |
| 16:32 | PR-03 | Missing-project rename input path via prompt automation | Rename + create and update `helix.toml` | Prompt accepted create, but automated text injection did not change default name | BLOCKED | Deferred to manual keyboard validation |
| 16:34 | SY-01 | `project.name = proj-proj`; `helix sync` cluster picker | Show only project clusters | Prompt showed exactly `Production` and `polly-is-yummy` for `proj-proj` | PASS | Project-scoped cluster options verified |
| 16:35 | DP-00 | `helix check cloud-prod`, `helix build --instance cloud-prod` | Build pipeline prechecks pass | Check/build both succeeded | PASS | Fresh project schema/queries valid |
| 16:36 | DP-01 | `helix push cloud-prod` | CLI deploy succeeds | Redeployed successfully with cloud URL | PASS | Confirms deploy no longer failing |
| 16:36 | DP-02 | `helix push cloud-prod --dev` | One-shot dev override only | Warned one-shot override; deploy succeeded; file build mode unchanged | PASS | `helix.toml` retained `build_mode = "release"` pre-sync |
| 16:37 | SY-02/SY-03 | `helix sync` from `Production` cluster post-deploy | Source files synced and config reconciled | Sync succeeded and updated `helix.toml`; later inspection showed payload included only `schema.hx` | PARTIAL | Canonical cloud sections updated; query source file gap investigated |
| 16:39 | OV-01 | Modify local `schema.hx`; run `helix sync`; choose No | Warn + abort preserves local changes | Warning shown with `schema.hx`; sync aborted by user | PASS | Local modification remained intact |
| 16:40 | OV-02 | Re-run with differing file; choose Yes | Overwrite local file with remote | Warning shown; sync completed; local override removed | PASS | `schema.hx` restored to remote content |
| 16:41 | SY-05 | `helix sync Production` explicit instance mode | Backward-compatible explicit sync | Non-interactive run hit confirm prompt path (`not connected`); interactive run succeeded | PARTIAL | Functional in terminal; script-mode caveat when overwrite confirm required |
| 16:42 | RG-01 | `helix logs Production --range`, `--live` | Logs commands work | Range returned empty gracefully; live stream showed container/runtime logs | PASS | Live command stable for test window |
| 16:43 | RG-02 | `helix auth logout` then `helix push Production`, restore creds | Cloud command blocked when logged out; recovers after login state restored | Logged-out push returned auth guidance; restored credentials; `helix check Production` succeeded | PASS | Auth failure + recovery path verified |
| 16:45 | SY-02-EV | Inspect sync payload + S3 source objects for deployed cluster | Payload should include schema and query `.hx` files | Payload had `hx_files=['schema.hx']`; S3 key for query file stored as absolute path and skipped by sync sanitizer | FAIL | Query file key observed as `.../<cluster_id>//private/tmp/.../queries.hx` |
| 16:47 | DP-03-EV | `helix dashboard start Production --port 4015 --restart && helix dashboard status && helix dashboard stop` | Dashboard cloud mode should work on synced project config | Start/status/stop all succeeded in cloud mode | PASS | Confirms dashboard can operate against reconciled cloud config |
| 17:49 | FX-01 | `GET /api/cli/clusters/{cluster_id}/project` | New cluster-project resolver endpoint available | Returned `200` with project metadata | PASS | Confirms backend update active |
| 17:52 | FX-02 | `helix push Production` after relative-path fix | Redeploy should upload query files with relative keys | Deploy succeeded; new `queries.hx` key written at canonical path | PASS | Legacy bad key still present from old deploys |
| 17:53 | FX-03 | `/api/cli/clusters/{id}/sync` payload inspection | Payload should include schema + query files | `hx_files=['queries.hx','schema.hx']` | PASS | Sync file gap fixed for new deploys |
| 17:54 | FX-04 | `helix sync Production` (no `--yes`) with overwrite needed | Non-interactive command should provide actionable error | Command failed with clear `Re-run with '--yes'` guidance | PASS | Replaced prior `not connected` failure |
| 17:54 | FX-05 | `helix sync Production --yes` | Non-interactive overwrite should proceed | Sync succeeded and wrote 3 files (`schema.hx`,`queries.hx`,`helix.toml`) | PASS | Explicit instance sync now automation-safe |
| 17:55 | FX-06 | Force bad local project config then `helix sync Production --yes` | Explicit sync should reconcile `helix.toml` from canonical project metadata | `project.name` reset to `proj-proj`; stale cloud entry removed | PASS | Confirms explicit sync uses metadata source-of-truth |
| 17:57 | WS-04 | Remove `~/.helix/config`, run interactive `helix sync` with single workspace account | No workspace prompt; auto-select sole workspace | `SEEN_WORKSPACE=0`; cluster prompt shown directly; config recreated with personal workspace id | PASS | Single-workspace auto-select verified |
| 18:26 | PR-03-MANUAL | Manual keyboard entry for project rename in `helix sync` flow | Rename target accepted and reflected in cloud | User confirmed rename works and visible in dashboard | PASS | Closes automation-only prompt injection gap |
| 18:56 | RG-04-A | `GET /api/cli/workspaces/{no_billing_id}/billing` | Billing check should indicate no billing without failing | Returned `500 {"error":"Failed to verify billing"}` | FAIL | Endpoint error for no-billing workspace |
| 18:56 | RG-04-B | `POST /api/cli/workspaces/{no_billing_id}/projects` | Project creation should be blocked by billing gate | Returned `402 Payment Required` with billing-setup message | PASS | Correct enforcement path for create |
| 18:57 | RG-04-C | CLI `helix sync` create flow against no-billing workspace | CLI should surface billing block clearly | CLI failed with `402 Payment Required` create-project error | PASS | User-facing no-billing block validated |

## Detailed Results

### Workspace Resolution
| Test ID | Scenario | Expected | Actual | Status | Evidence |
|---|---|---|---|---|---|
| WS-01 | Cached workspace valid | No workspace prompt | Workspace prompt skipped | PASS | Immediate project resolution path |
| WS-02 | Cached workspace stale | Warning + workspace reprompt + cache update | Warning shown; workspace reselected; cache rewritten | PASS | `~/.helix/config` reset to valid workspace id |
| WS-03 | No cache + multiple workspaces | Workspace prompt appears and selection persisted | Prompt shown; selected `Personal`; config created | PASS | Confirmed on fresh project |
| WS-04 | No cache + single workspace | Auto-select sole workspace without prompt | Workspace prompt was skipped; config recreated with personal workspace id | PASS | `SEEN_WORKSPACE=0` and `~/.helix/config` contains personal workspace id |

### Project Resolution
| Test ID | Scenario | Expected | Actual | Status | Evidence |
|---|---|---|---|---|---|
| PR-01 | Existing project name used | Existing project auto-selected | Printed use-existing-project message and proceeded | PASS | `Using project 'proj' from your selected workspace.` |
| PR-02 | Missing project -> create same name | Create cloud project with same name | Created project `e2e-create-same`; no rename applied | PASS | `helix.toml` name unchanged as expected |
| PR-03 | Missing project -> rename+create | Prompt rename and persist new `project.name` | Manual keyboard test confirmed rename/create works and project is visible in dashboard | PASS | User-verified interactive rename path |

### Sync and Config Reconciliation
| Test ID | Scenario | Expected | Actual | Status | Evidence |
|---|---|---|---|---|---|
| SY-01 | Project-scoped cluster list | Show only clusters for resolved project | Prompt listed only `proj-proj` clusters (`Production`, `polly-is-yummy`) | PASS | Cluster picker scoped correctly |
| SY-02 | Sync selected cluster files | Pull selected cluster files to queries dir | After fix + redeploy, sync payload contains both `schema.hx` and `queries.hx` and writes both | PASS | `fx-03`/`fx-05` confirmed query file sync restored |
| SY-03 | Canonical `helix.toml` reconciliation | Replace cloud cluster sections from cloud metadata | `helix.toml` updated with both project clusters and cloud build modes | PASS | `[cloud.Production.helix]` + `[cloud.polly-is-yummy.helix]` present |
| SY-04 | Subsequent no-instance sync prompt behavior | Reuse project name and prompt cluster each run | Repeated `helix sync` runs show project message + cluster prompt | PASS | Verified across multiple runs |
| SY-05 | `helix sync <instance>` compatibility | Explicit instance sync remains usable | Explicit sync works interactively; non-interactive now returns clear guidance or succeeds with `--yes` | PASS | `fx-04` + `fx-05` verified |

### Overwrite Protection
| Test ID | Scenario | Expected | Actual | Status | Evidence |
|---|---|---|---|---|---|
| OV-01 | Warn + cancel preserve local files | Show overwrite warning and preserve local edits on cancel | Warning listed `schema.hx`; cancel aborted sync; local file unchanged | PASS | `local_only_note` remained after cancel |
| OV-02 | Warn + confirm overwrite files | Overwrite differing local files when confirmed | Warning shown; confirm yes; file overwritten to remote version | PASS | `local_only_note` removed after sync |

### Deploy Parity
| Test ID | Scenario | Expected | Actual | Status | Evidence |
|---|---|---|---|---|---|
| DP-01 | CLI deploy happy path | Successful deploy for configured cloud cluster | `helix push cloud-prod` redeployed successfully | PASS | No backend CodeBuild failure observed |
| DP-02 | `--dev` one-shot behavior | Override only for current deploy, no persistent mutation | Warning shown and deploy succeeded; persisted build mode remained unchanged before reconciliation | PASS | One-shot message confirmed |
| DP-03 | Dashboard deploy parity check | CLI deploy path should use same backend core as dashboard deploy | Route behavior validated in implementation + CLI deploy success on unified path | PARTIAL | Direct dashboard-route invocation not executed due auth-mode differences |
| DP-04 | `done` SSE event handling | CLI correctly handles unified deploy completion event | Deploy output rendered successful redeploy on unified stream path | PASS | No SSE parse failure; success event handled |

### Regression Sweep
| Test ID | Scenario | Expected | Actual | Status | Evidence |
|---|---|---|---|---|---|
| RG-01 | Logs live/range | Both commands operational | Range returns clean empty-state; live stream emits runtime logs | PASS | Production instance logs retrieved |
| RG-02 | Auth logout/login behavior | Logged-out cloud command fails with guidance and recovers after auth restored | Logout + deploy failed with auth guidance; restored creds and cloud check succeeded | PASS | Recovery confirmed with `helix check Production` |
| RG-03 | Add/init cloud project-name persistence | Resolved cloud project name persisted in add/init cloud flows | Code paths implemented; existing-project behavior validated via sync flow | PARTIAL | Rename text-entry path needs manual add/init validation |
| RG-04 | No-billing negative path | Billing-gated creation/deploy should be blocked with clear errors | Project creation blocked with `402 Payment Required`; CLI surface clear. Billing status endpoint currently errors (`500`) on no-billing workspace | PARTIAL | Enforcement path PASS; billing status endpoint needs fix |

## Open Issues Found
- `ISSUE-001 (RESOLVED)` Query `.hx` sync gap fixed for new deploys by normalizing query filenames to relative paths during upload. Existing legacy absolute-path S3 keys may still produce warning logs until cleaned up.
- `ISSUE-002 (RESOLVED)` Non-interactive explicit sync now provides actionable guidance and supports `--yes` for automation.
- `ISSUE-003 (RESOLVED)` Manual keyboard verification confirmed project rename/create prompt path works as expected.
- `ISSUE-004 (P2)` `GET /api/cli/workspaces/{workspace_id}/billing` returns `500 Failed to verify billing` for known no-billing workspace (`8f5b5a0f-7ac4-4f7f-8829-19b8e8f6b9d5`) instead of returning a non-error billing status payload.

## Notes
- This run is using a fresh empty project directory as requested.
- Single-workspace auto-select case has now been validated with personal-workspace-only account state.
- Sync success required at least one cluster with source artifacts; this was achieved after successful deploy of cluster `e493bbea-c192-4d6e-a34b-87f50f44bd42`.
- Additional forensics captured for sync source gap: direct sync endpoint payload inspection and S3 recursive listing.
- Continued retest after backend restart confirms the primary sync regression and non-interactive explicit sync regression are fixed.
- Manual user validation also confirmed PR-03 rename/create path behavior in real interactive usage.
- No-billing workspace fixture was provided and used; billing enforcement on project creation works (`402`), while billing status endpoint behavior still needs correction.
