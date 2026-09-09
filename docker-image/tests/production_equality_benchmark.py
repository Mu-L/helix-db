#!/usr/bin/env python3
"""Replay HEL-850's recovered ASTs on disposable localhost images.

Parameters and graph data are synthetic. The independent graph oracle checks
complete projected results, traversal tenant filters, distinctness and ordering.
"""
import argparse
import copy
import json
import statistics
from pathlib import Path

import range_benchmark as bench
import equality_benchmark as equality

FIXTURES = bench.FIXTURES / "hel850-production-queries.json"
TENANT = "fixture-tenant"


def fixture(size=10000, services=50):
    rows = equality.fixture_rows(size // 5, 4096)
    for row in rows:
        if row["tenant"] == bench.TENANT:
            row["tenant"] = TENANT
    edges = []
    for i in range(services):
        kinds = {"svc": "service", "pod": "pod", "pod2": "pod", "rs": "replicaset",
                 "dep": "deployment", "sts": "statefulset", "ds": "daemonset",
                 "foreign_svc": "service", "foreign_pod": "pod", "foreign_dep": "deployment",
                 "critical": "vulnerability", "critical2": "vulnerability", "low": "vulnerability",
                 "foreign_vuln": "vulnerability"}
        for suffix, kind in kinds.items():
            rows.append({"id": f"graph-{i}-{suffix}", "tenant": "other" if suffix.startswith("foreign") else TENANT,
                         "type": kind, "deleted": False, "last_seen": bench.CUTOFF + len(rows) + 1,
                         "severity": "low" if suffix == "low" else "critical", "name": suffix,
                         "raw_data": "x" * 4096})
        connections = [("svc", "ROUTES_TO", "pod"), ("svc", "ROUTES_TO", "pod2"),
                       ("foreign_svc", "ROUTES_TO", "pod"), ("svc", "ROUTES_TO", "foreign_pod"),
                       ("rs", "MANAGES", "pod"), ("rs", "MANAGES", "pod2"),
                       ("dep", "CREATES", "rs"), ("dep", "MANAGES", "pod2"),
                       ("sts", "MANAGES", "pod"), ("ds", "MANAGES", "pod2"),
                       ("foreign_dep", "MANAGES", "pod"), ("foreign_dep", "CREATES", "rs"),
                       ("dep", "HAS_VULNERABILITY", "critical"), ("dep", "HAS_VULNERABILITY", "critical2"),
                       ("sts", "HAS_VULNERABILITY", "critical"), ("ds", "HAS_VULNERABILITY", "low"),
                       ("rs", "HAS_VULNERABILITY", "foreign_vuln"),
                       ("foreign_dep", "HAS_VULNERABILITY", "critical")]
        edges.extend((f"graph-{i}-{a}", label, f"graph-{i}-{b}") for a, label, b in connections)
    return rows, edges


def prepare(port, rows, edges):
    # Equality indexes are demonstrated by the production probes. Retain a
    # last_seen range index to also exercise existing ordered-read alternatives.
    equality.prepare(port, rows)
    ids, _ = bench.request(port, bench.batch("read", [{"value_map": {
        "input": {"nodes_where": {"predicate": {"eq": {
            "left": {"property": "$label"}, "right": {"constant": {"string": "Resource"}}
        }}}}, "properties": ["$id", "id"]}}]))
    entity_ids = {row["id"]: row["$id"] for row in ids["q0"]}
    for offset in range(0, len(edges), 100):
        roots = [{"add_e": {"input": {"nodes": {"reference": {"ids": [entity_ids[source]]}}},
                            "label": label, "to": {"ids": [entity_ids[target]]}, "properties": []}}
                 for source, label, target in edges[offset:offset + 100]]
        bench.request(port, bench.batch("write", roots))


def expected(name, request, rows, edges):
    params = request["parameters"]
    nodes = {row["id"]: row for row in rows}
    tenant = params["tenant"] if "tenant" in params else None
    if name == "service_workload_map":
        result = set()
        for svc, relation, pod in edges:
            if relation != "ROUTES_TO" or nodes[svc]["type"] != params["svc_type"]:
                continue
            if nodes[svc]["tenant"] != tenant or nodes[pod]["tenant"] != tenant:
                continue
            for manager, manages, child in edges:
                if manages != "MANAGES" or child != pod or nodes[manager]["tenant"] != tenant:
                    continue
                workloads = [manager] if nodes[manager]["type"] in ["deployment", "statefulset", "daemonset"] else []
                if nodes[manager]["type"] == "replicaset":
                    workloads += [parent for parent, creates, rs in edges if creates == "CREATES" and rs == manager
                                  and nodes[parent]["type"] == "deployment" and nodes[parent]["tenant"] == tenant]
                result.update((svc, wid, nodes[wid]["type"]) for wid in workloads)
        return [{"service_id": svc, "workload_id": wid, "workload_type": kind} for svc, wid, kind in sorted(result)]
    if name == "covered_workloads":
        kinds = {params[f"t{i}"] for i in range(5)}
        matches = {source for source, relation, target in edges if relation == "HAS_VULNERABILITY"
                   and nodes[source]["type"] in kinds and nodes[source]["tenant"] == tenant
                   and nodes[target]["tenant"] == tenant and nodes[target]["severity"] == params["tval"]}
        return [{"wid": wid} for wid in sorted(matches)]
    selected = [row for row in rows if all(row[prop] == params[prop] for prop in ["tenant", "type", "deleted"] if prop in params)]
    if "since_last_seen_us" in params:
        selected = [row for row in selected if row["last_seen"] > params["since_last_seen_us"]]
        selected.sort(key=lambda row: -row["last_seen"])
    selected = selected[:params["limit"]]
    projection = request["query"]["read"]["entries"][-1]["query"]["root"]["value_map"]["properties"]
    return [{key: row[key] for key in projection if key in row} for row in selected]


def check_response(result, oracle, ordered):
    actual = bench.normalized(result)
    if not ordered:
        key = lambda row: json.dumps(row, sort_keys=True)
        actual, oracle = sorted(actual, key=key), sorted(oracle, key=key)
    assert actual == oracle, (len(actual), len(oracle), actual[:2], oracle[:2])


def run(args):
    rows, edges = fixture(args.size, args.services)
    ports = [args.baseline, args.candidate]
    if not args.no_seed:
        for port in ports:
            prepare(port, rows, edges)
    requests = json.loads(FIXTURES.read_text())
    report = {"fixture_rows": len(rows), "fixture_edges": len(edges), "samples": args.samples, "cases": []}
    for name, original in requests.items():
        for window in (["broad", "narrow"] if "since_last_seen_us" in original["parameters"] else ["default"]):
            request = copy.deepcopy(original)
            if "limit" in request["parameters"] and window == "default":
                request["parameters"]["limit"] = len(rows) + 1
            if window != "default":
                request["parameters"]["since_last_seen_us"] = bench.CUTOFF + (0 if window == "broad" else len(rows) - 100)
            oracle = expected(name, request, rows, edges)
            times = {port: [] for port in ports}
            for sample in range(args.samples + 3):
                for port in ports if sample % 2 == 0 else reversed(ports):
                    result, elapsed = bench.request(port, request)
                    check_response(result, oracle, window != "default")
                    if sample >= 3:
                        times[port].append(elapsed)
            case = {"query": name, "window": window, "rows": len(oracle)}
            for label, port in zip(["baseline", "candidate"], ports):
                case[label] = {"p50_ms": statistics.median(times[port]) * 1000,
                               "p95_ms": bench.percentile(times[port], .95) * 1000, "max_ms": max(times[port]) * 1000}
            report["cases"].append(case)
            print(json.dumps(case), flush=True)
    if args.output:
        Path(args.output).write_text(json.dumps(report, indent=2) + "\n")
    return report


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", type=int, default=18250)
    parser.add_argument("--candidate", type=int, default=18251)
    parser.add_argument("--size", type=int, default=10000)
    parser.add_argument("--services", type=int, default=50)
    parser.add_argument("--samples", type=int, default=100)
    parser.add_argument("--no-seed", action="store_true")
    parser.add_argument("--output")
    run(parser.parse_args())
