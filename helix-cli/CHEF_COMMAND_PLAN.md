# Helix Chef Command Plan

## Goal

Add a simple `helix chef` onboarding command that bootstraps a first HelixDB application for a coding agent. The command should install Helix agent context, initialize a local project, start the local database, seed starter data through dynamic JSON requests, and open the dashboard.

## User Flow

1. Run `helix chef` or `helix chef --auto`.
2. Prompt first: `What do you want to build?` with an empty answer allowed to skip.
3. For `helix chef`, prompt for setup mode:
   - `Automatic setup`: run the full flow with defaults.
   - `Manual setup`: confirm or customize each step.
4. For `helix chef -a/--auto`, skip setup-mode prompts and run automatic setup.
5. Default project path: `~/my-first-helix-project`.
6. Default local instance: `dev` on port `8080`.

## Orchestrated Steps

1. Install Helix skills with `npx skills add HelixDB/skills`.
2. Install the Helix docs MCP using Mintlify's hosted MCP endpoint at `https://docs.helix-db.com/mcp`.
3. Initialize a Helix project at the chosen path by reusing `helix init local` behavior.
4. Write a placeholder coding-agent prompt that includes the user's app intent.
5. Write starter dynamic JSON requests for seed data and reads.
6. Start the local database by reusing `helix run dev` behavior.
7. Run the seed dynamic query to insert data.
8. Start the dashboard by reusing `helix dashboard start` behavior.

## Placeholder Query Generation

The final query-generation prompt is still to be authored. Until then, `helix chef` will:

1. Persist the user's build intent in `HELIX_CHEF_PROMPT.md`.
2. Include instructions for a coding agent to replace the starter JSON files with app-specific dynamic queries.
3. Generate generic starter files under `examples/`:
   - `seed.json`: creates example `User` nodes.
   - `read_users.json`: reads seeded `User` nodes.

## Implementation Checklist

- [x] Add `Chef { auto: bool }` to the root CLI command enum.
- [x] Add `cook` as a command alias for `chef` if supported cleanly by `clap`.
- [x] Add `commands::chef` module and register it in `commands/mod.rs`.
- [x] Add interactive prompts for build intent, setup mode, project path, and step confirmation.
- [x] Add command execution helpers for skills and MCP install.
- [x] Reuse `commands::init::run` for project initialization.
- [x] Change current directory to the generated project before `run`, `query`, and `dashboard` orchestration.
- [x] Generate `HELIX_CHEF_PROMPT.md`, `examples/seed.json`, and `examples/read_users.json`.
- [x] Reuse `commands::run::run` to start `dev`.
- [x] Reuse `commands::query::run` to apply seed data.
- [x] Reuse `commands::dashboard::run` to start the dashboard.
- [x] Add parser tests for `helix chef`, `helix chef --auto`, `helix chef -a`, and `helix cook`.
- [x] Add unit tests for generated JSON request shape and prompt content.
- [x] Run `cargo fmt` and `cargo test -p helix-cli`.

## Follow-Up Work

- Replace the placeholder prompt with the final generation prompt.
- Decide whether app-specific JSON generation should be done by the CLI itself, the active coding agent, or a local/remote model provider.
- Consider a `--path` flag if users need non-interactive custom project paths without manual mode.
