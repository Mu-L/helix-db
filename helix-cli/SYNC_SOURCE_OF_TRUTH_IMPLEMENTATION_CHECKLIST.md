# Sync Source-Of-Truth Implementation Checklist

## Objective
- Align `helix sync` and `helix add cloud` flows with PlanetScale metadata as source of truth.
- Ensure CLI deploy path uses the same backend deployment pipeline as dashboard deploy.

## Implemented
- [x] Added project-level cluster metadata endpoint: `GET /api/cli/projects/{project_id}/clusters`.
- [x] Updated CLI sync (interactive, no instance) to use project-driven flow from `helix.toml` project name.
- [x] Added workspace cache resolution with stale cache invalidation + reselection.
- [x] Added project resolution for sync:
  - [x] Use existing project if name matches `helix.toml`.
  - [x] If missing, prompt to create and allow rename.
  - [x] Persist renamed project name back to `helix.toml`.
- [x] Added project-cluster selection for sync and synced selected cluster source files into configured queries directory.
- [x] Added overwrite-warning + confirmation when local query files differ from incoming sync files.
- [x] Added canonical `helix.toml` reconciliation after project sync:
  - [x] Update `project.name` from cloud metadata.
  - [x] Replace `[cloud]` and `[enterprise]` sections from cloud project cluster metadata.
- [x] Updated `helix add cloud` flow to return resolved cloud project name and persist it in local config when changed.
- [x] Updated `helix init` cloud flow to persist resolved cloud project name.
- [x] Added CLI SSE support for unified `done` deploy event.

## Deploy Parity
- [x] Routed `/api/cli/clusters/{cluster_id}/deploy` to the same deployment pipeline used by dashboard deploy handler core.
- [x] Added CLI deploy wrapper in `api/clusters.rs` with API-key auth.
- [x] Extended shared deploy request model to accept optional source payload (`schema`, `queries`, `helix_toml`) for CLI deploys.
- [x] In shared deploy pipeline, when source payload is present:
  - [x] Generate `queries.rs`.
  - [x] Upload generated source to S3 for build input.
  - [x] Upload source artifacts (`schema.hx`, query `.hx`, optional `helix.toml`) for sync.

## Validation Run
- [x] `cargo check -p helix-cli`
- [x] `cargo test -p helix-cli --lib -- --test-threads=1`
- [x] `cargo check -p build-gateway`

## Follow-Up (Optional Hardening)
- [ ] Remove deprecated/unused legacy CLI deploy handler file once no longer referenced.
- [ ] Add explicit automated tests for new project-sync resolution and canonical reconciliation behaviors.
- [ ] Add endpoint-level tests for `GET /api/cli/projects/{project_id}/clusters`.
