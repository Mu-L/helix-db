#!/usr/bin/env bash

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BASELINES_PATH="$ROOT/scripts/cov005-coverage-baselines.json"
WORKSPACE_REPORT_PATH="${1:-}"
DB_PRODUCTION_REPORT_PATH="${2:-}"

usage() {
    echo "usage: scripts/cov005-coverage.sh <workspace-coverage.json> <db-production-coverage.json>" >&2
    exit 2
}

[[ -n "$WORKSPACE_REPORT_PATH" && -n "$DB_PRODUCTION_REPORT_PATH" ]] || usage
[[ -f "$WORKSPACE_REPORT_PATH" && -f "$DB_PRODUCTION_REPORT_PATH" ]] || usage

command -v jq >/dev/null 2>&1 || {
    echo "COV-005 coverage validation requires jq" >&2
    exit 1
}
command -v rg >/dev/null 2>&1 || {
    echo "COV-005 coverage validation requires ripgrep" >&2
    exit 1
}

jq -e '
    .schema_version == 1
    and (.scopes | length > 0)
    and all(
        .scopes[];
        (.report == "workspace" or .report == "db-production")
        and ((.paths // []) | type == "array")
        and ((.prefixes // []) | type == "array")
        and (((.paths // []) | length) + ((.prefixes // []) | length) > 0)
        and (.minimum_covered_lines | type == "number" and . >= 0 and floor == .)
        and (.maximum_uncovered_lines | type == "number" and . >= 0 and floor == .)
        and (.minimum_percent | type == "number" and . >= 0 and . <= 100)
    )
    and (.remaining_state_ids | type == "array" and length > 0)
    and (.public_boundary_paths | type == "array" and length > 0)
' "$BASELINES_PATH" >/dev/null

FAILED=0

for report_kind in workspace db-production; do
    if [[ "$report_kind" == "workspace" ]]; then
        report_path="$WORKSPACE_REPORT_PATH"
    else
        report_path="$DB_PRODUCTION_REPORT_PATH"
    fi

    while IFS= read -r scope_name; do
        actual="$(
            jq \
                --arg root "$ROOT" \
                --arg scope "$scope_name" \
                --arg report "$report_kind" \
                --slurpfile baselines "$BASELINES_PATH" \
                '
                def normalized_path($root):
                    if startswith($root + "/") then ltrimstr($root + "/")
                    else .
                    end;

                ($baselines[0].scopes[$scope]) as $baseline
                | [
                    .data[0].files[]
                    | .filename as $filename
                    | ($filename | normalized_path($root)) as $path
                    | select(
                        (($baseline.paths // []) | index($path)) != null
                        or any(
                            ($baseline.prefixes // [])[];
                            . as $prefix | $path | startswith($prefix)
                        )
                    )
                ] as $files
                | ($files | map(.summary.lines.count) | add // 0) as $count
                | ($files | map(.summary.lines.covered) | add // 0) as $covered
                | {
                    scope: $scope,
                    report: $report,
                    covered: $covered,
                    count: $count,
                    uncovered: ($count - $covered),
                    percent: (if $count == 0 then 0 else ($covered * 100 / $count) end),
                    required: {
                        minimum_covered_lines: $baseline.minimum_covered_lines,
                        maximum_uncovered_lines: $baseline.maximum_uncovered_lines,
                        minimum_percent: $baseline.minimum_percent
                    }
                }
                | .passed = (
                    .count > 0
                    and .covered >= .required.minimum_covered_lines
                    and .uncovered <= .required.maximum_uncovered_lines
                    and .percent >= .required.minimum_percent
                )
                ' "$report_path"
        )"
        jq -c . <<<"$actual"
        if ! jq -e '.passed' <<<"$actual" >/dev/null; then
            FAILED=1
        fi
    done < <(
        jq -r \
            --arg report "$report_kind" \
            '.scopes | to_entries[] | select(.value.report == $report) | .key' \
            "$BASELINES_PATH"
    )
done

expected_state_ids="$(jq -r '.remaining_state_ids[]' "$BASELINES_PATH")"
actual_state_ids="$(
    rg -o 'N:[A-Z]{2}-[0-9]{2}' "$ROOT/COV005_STATE_BACKLOG.md" \
        | sed 's/^N://' \
        | sort -u
)"
if [[ "$actual_state_ids" != "$expected_state_ids" ]]; then
    echo "COV-005 named production-state ownership changed without a reviewed baseline update" >&2
    diff -u <(printf '%s\n' "$expected_state_ids") <(printf '%s\n' "$actual_state_ids") || true
    FAILED=1
fi

while IFS= read -r boundary_path; do
    while IFS=: read -r source_path line_number _; do
        execution_count="$(
            jq \
                --arg filename "$ROOT/$source_path" \
                --argjson line "$line_number" \
                '[
                    .data[0].files[]
                    | select(.filename == $filename)
                    | .segments[]
                    | select(.[0] == $line and .[3] and (.[5] | not))
                    | .[2]
                ] | max // 0' \
                "$WORKSPACE_REPORT_PATH"
        )"
        if (( execution_count == 0 )); then
            echo "public boundary is completely unexecuted: $source_path:$line_number" >&2
            FAILED=1
        fi
    done < <(
        cd "$ROOT"
        rg --no-heading -n \
            '^[[:space:]]*pub(\([^)]*\))?[[:space:]]+(async[[:space:]]+)?fn[[:space:]]+' \
            "$boundary_path"
    )
done < <(jq -r '.public_boundary_paths[]' "$BASELINES_PATH")

if (( FAILED != 0 )); then
    echo "COV-005 coverage or state-ownership ratchets were not met" >&2
    exit 1
fi

echo "COV-005 coverage and state-ownership ratchets passed"
