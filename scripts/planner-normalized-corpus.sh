#!/usr/bin/env bash
set -euo pipefail

workspace="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
corpus="$(mktemp -d "${TMPDIR:-/tmp}/helix-planner-corpus.XXXXXX")"
trap 'rm -rf "${corpus}"' EXIT

cd "${workspace}"
cargo run -p helix-db --example generate_parity_fixtures -- "${corpus}"
cargo run -p helix-db-testkit --example check_planner_corpus -- "${corpus}"
