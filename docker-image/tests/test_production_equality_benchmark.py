"""Independent result oracles for recovered customer traversals."""
import copy
import json
import unittest

import production_equality_benchmark as benchmark


class ProductionEqualityBenchmarkTests(unittest.TestCase):
    def test_traversals_deduplicate_paths_and_exclude_foreign_tenants(self):
        rows, edges = benchmark.fixture(100, 2)
        requests = json.loads(benchmark.FIXTURES.read_text())
        services = benchmark.expected("service_workload_map", requests["service_workload_map"], rows, edges)
        self.assertEqual(services, [
            {"service_id": f"graph-{i}-svc", "workload_id": f"graph-{i}-{suffix}", "workload_type": kind}
            for i in range(2) for suffix, kind in [("dep", "deployment"), ("ds", "daemonset"), ("sts", "statefulset")]
        ])
        covered = benchmark.expected("covered_workloads", requests["covered_workloads"], rows, edges)
        self.assertEqual(covered, [{"wid": f"graph-{i}-{suffix}"} for i in range(2) for suffix in ["dep", "sts"]])
        absent = copy.deepcopy(requests["covered_workloads"])
        absent["parameters"]["tval"] = "absent"
        self.assertEqual(benchmark.expected("covered_workloads", absent, rows, edges), [])

    def test_complete_oracle_rejects_missing_duplicate_extra_and_foreign_rows(self):
        expected = [{"wid": "a"}, {"wid": "b"}]
        benchmark.check_response({"q": list(reversed(expected))}, expected, False)
        for actual in [expected[:1], expected + expected[:1], expected + [{"wid": "foreign"}],
                       [{"wid": "a", "extra": True}, {"wid": "b"}]]:
            with self.subTest(actual=actual), self.assertRaises(AssertionError):
                benchmark.check_response({"q": actual}, expected, False)
        with self.assertRaises(AssertionError):
            benchmark.check_response({"q": list(reversed(expected))}, expected, True)
        benchmark.check_response({"q": []}, [], True)

    def test_ordered_oracle_applies_cutoff_before_limit_and_keeps_full_projection(self):
        rows, edges = benchmark.fixture(100, 0)
        request = json.loads(benchmark.FIXTURES.read_text())["find_resource_dedup_keys_by_type"]
        request["parameters"].update(limit=2, since_last_seen_us=benchmark.bench.CUTOFF + 80)
        actual = benchmark.expected("find_resource_dedup_keys_by_type", request, rows, edges)
        self.assertEqual([row["id"] for row in actual], ["resource-00000095", "resource-00000090"])
        self.assertTrue(all(set(row) == {"id", "cluster_id", "namespace", "name", "last_seen"} for row in actual))
        request["parameters"]["limit"] = 0
        self.assertEqual(benchmark.expected("find_resource_dedup_keys_by_type", request, rows, edges), [])
