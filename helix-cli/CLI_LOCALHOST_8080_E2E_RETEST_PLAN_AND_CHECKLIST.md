# Helix CLI Localhost:8080 E2E Retest Plan and Checklist

## Objective
- Validate end-to-end behavior after recent sync/deploy parity changes.
- Confirm `helix sync` follows project-first cloud source-of-truth behavior.
- Confirm CLI deploy path now behaves the same as dashboard deploy path.

## Scope
- Workspace resolution and cache behavior (`~/.helix/config`)
- Project resolution from `helix.toml` (`project.name`)
- Cluster selection + sync behavior
- Canonical `helix.toml` reconciliation from cloud metadata
- Overwrite-loss warning/confirmation for local `.hx` changes
- CLI deploy parity with dashboard deploy
- Core auth/logs/add/init regressions

## Success Criteria
- P0 tests pass with no unresolved blockers.
- No divergence between local `helix.toml` cluster metadata and cloud metadata after sync.
- CLI deploy and dashboard deploy use equivalent backend behavior and outcomes.

---

## Preflight Checklist

### Environment Readiness
- [ ] Backend is running on `http://localhost:8080` with latest changes.
- [ ] CLI binary rebuilt from latest `helix-cli` source.
- [ ] `helix auth login` completed and `~/.helix/credentials` present.
- [ ] Test account access confirmed.

### Required Test Fixtures
- [ ] Account with multiple workspaces (for workspace prompt tests).
- [ ] Account (or user) with exactly one workspace (for auto-select test).
- [ ] Workspace containing existing project with multiple clusters.
- [ ] At least one deployed cluster with sync source files in S3.
- [ ] Optional: no-billing workspace for billing negative tests.

### API Sanity Checks
- [ ] `GET /api/cli/projects/{project_id}/clusters` returns `200` (or `403` when expected), not `405`.
- [ ] `POST /api/cli/clusters/{cluster_id}/deploy` opens SSE stream.

---

## Execution Order
1. Phase 1: Workspace Resolution
2. Phase 2: Project Resolution
3. Phase 3: Cluster Selection + Sync
4. Phase 4: Overwrite Protection
5. Phase 5: Deploy Parity
6. Phase 6: Regression Sweep

---

## Phase 1 - Workspace Resolution

### WS-01 Cached workspace valid
- [ ] Seed `~/.helix/config` with valid workspace id.
- [ ] Run `helix sync` in project dir.
- [ ] Verify no workspace prompt appears and cached workspace is used.

### WS-02 Cached workspace stale
- [ ] Seed stale workspace id in `~/.helix/config`.
- [ ] Run `helix sync`.
- [ ] Verify stale warning appears, workspace is reselected, cache is updated.

### WS-03 No cache + multiple workspaces
- [ ] Remove `~/.helix/config`.
- [ ] Run `helix sync`.
- [ ] Verify workspace selection prompt appears.
- [ ] Verify selected workspace id is persisted.

### WS-04 No cache + single workspace
- [ ] Use single-workspace account context.
- [ ] Run `helix sync`.
- [ ] Verify no workspace prompt appears and workspace is auto-selected.

---

## Phase 2 - Project Resolution from `helix.toml`

### PR-01 Project exists
- [ ] Set `project.name` to existing cloud project.
- [ ] Run `helix sync`.
- [ ] Verify message indicates existing project is used.

### PR-02 Project missing, create with same name
- [ ] Set `project.name` to non-existent project name.
- [ ] Run `helix sync`, choose create, keep default name.
- [ ] Verify project is created with same name.
- [ ] Verify `helix.toml` project name remains unchanged.

### PR-03 Project missing, rename during create
- [ ] Set `project.name` to non-existent name.
- [ ] Run `helix sync`, choose create, enter different name.
- [ ] Verify project is created with new name.
- [ ] Verify `helix.toml` `project.name` is updated to new name.

---

## Phase 3 - Cluster Selection and Sync

### SY-01 Cluster list is project-scoped
- [ ] Run `helix sync`.
- [ ] Compare selectable clusters with `/api/cli/projects/{project_id}/clusters` response.
- [ ] Verify only selected project's clusters appear.

### SY-02 Selected cluster files sync to configured path
- [ ] Select a cluster from the prompt.
- [ ] Verify `.hx` files are written to `project.queries` path (default `./db`).

### SY-03 Canonical `helix.toml` reconciliation
- [ ] After sync, inspect `helix.toml`.
- [ ] Verify `project.name` matches cloud project metadata.
- [ ] Verify `[cloud]` and `[enterprise]` reflect all project clusters from cloud.
- [ ] Verify stale local cluster entries are removed.

### SY-04 Subsequent sync behavior
- [ ] Run `helix sync` again with no instance.
- [ ] Verify project message is shown and cluster prompt appears again.

### SY-05 Backward compatibility (`helix sync <instance>`)
- [ ] Run `helix sync <instance>` for valid cloud instance.
- [ ] Verify explicit-instance sync remains functional.

---

## Phase 4 - Overwrite Protection

### OV-01 Warning + cancel path
- [ ] Modify local `.hx` files so they differ from remote.
- [ ] Run `helix sync`.
- [ ] Verify overwrite warning lists impacted files.
- [ ] Choose cancel.
- [ ] Verify local files remain unchanged.

### OV-02 Warning + confirm path
- [ ] Repeat differing-file setup.
- [ ] Run `helix sync`.
- [ ] Choose proceed.
- [ ] Verify local files are overwritten with remote content.

---

## Phase 5 - Deploy Parity (CLI vs Dashboard)

### DP-01 CLI deploy happy path
- [ ] Run `helix push <instance>`.
- [ ] Verify deploy completes using expected SSE lifecycle.

### DP-02 One-shot dev override
- [ ] Run `helix push <instance> --dev`.
- [ ] Verify one-shot dev behavior is applied.
- [ ] Verify persisted build mode in `helix.toml` is not permanently switched by override.

### DP-03 Dashboard deploy comparison
- [ ] Trigger deploy/redeploy from dashboard for same cluster.
- [ ] Compare backend behavior and result with CLI deploy.
- [ ] Verify no path-specific drift.

### DP-04 SSE done-event compatibility
- [ ] Verify CLI handles `done` event with `auth_key` for new deploy.
- [ ] Verify CLI handles `done` event without `auth_key` for redeploy.

---

## Phase 6 - Regression Sweep

### RG-01 Logs
- [ ] `helix logs <instance> --live` works.
- [ ] `helix logs <instance> --range ...` works with valid window.

### RG-02 Auth
- [ ] `helix auth logout` then cloud command fails with actionable guidance.
- [ ] `helix auth login` restores command functionality.

### RG-03 Add/init cloud flow project name persistence
- [ ] Run `helix add cloud` flow and verify resolved cloud project name persistence.
- [ ] Run `helix init` cloud flow and verify resolved cloud project name persistence.

### RG-04 Billing negative path (if fixture available)
- [ ] Attempt create/deploy in no-billing workspace.
- [ ] Verify expected payment-required behavior and messaging.

---

## P0 / P1 Priority

### P0 (must pass)
- [ ] WS-02
- [ ] PR-03
- [ ] SY-03
- [ ] OV-01
- [ ] DP-01
- [ ] DP-03

### P1 (should pass)
- [ ] All other test cases in this plan

---

## Evidence Capture Template

Use this entry format for each executed test case.

| Test ID | Command/Action | Expected | Actual | Status | Evidence |
|---|---|---|---|---|---|
|  |  |  |  | PASS/FAIL/BLOCKED |  |

---

## Final Sign-Off Checklist
- [ ] All P0 tests passed.
- [ ] Failures triaged with clear root-cause notes.
- [ ] Any blocked tests documented with concrete unblock requirements.
- [ ] Final E2E summary written and shared.
