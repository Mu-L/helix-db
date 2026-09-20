#!/usr/bin/env python3
"""Check early-termination value contracts against a seeded smoke-test server."""
import json
import sys
import urllib.error
import urllib.request


def query(port, root):
    payload = {"request_type": "read", "query": {"read": {
        "entries": [{"query": {"name": "result", "root": root}}],
        "returns": ["result"],
    }}}
    req = urllib.request.Request(
        f"http://127.0.0.1:{port}/v2/query",
        data=json.dumps(payload).encode(),
        headers={"Content-Type": "application/json"},
    )
    with urllib.request.urlopen(req, timeout=30) as response:
        return json.load(response)["result"]


def check(port):
    source = {"has_label": {"input": {"nodes": {"reference": "all"}},
                            "label": "DockerImageSmokeUser"}}
    count = query(port, {"count": {"input": source}})
    ids = query(port, {"id": {"input": source}})
    assert ids, "smoke fixture must contain nodes"
    for matches in (True, False):
        condition = {"eq": {"left": {"property": "scenario"},
                            "right": {"constant": {"string":
                                "docker-image-smoke" if matches else "absent"}}}}
        for projection, expected in (("count", [count]), ("id", ids[:1])):
            child = {"root": {projection: {"input": "context"}}}
            for has_else in (False, True):
                branch = {"input": source, "condition": condition, "then_traversal": child}
                if has_else:
                    branch["else_traversal"] = child
                root = {"limit": {"input": {"choose": branch}, "count": {"literal": 1}}}
                actual = query(port, root)
                assert actual == (expected if matches or has_else else None), (root, actual)

    # Label must reject scalar input even when the scalar sequence is empty.
    empty = {"has_label": {"input": {"nodes": {"reference": "all"}},
                           "label": "MissingEarlyTerminationFixture"}}
    for input_rows in (empty, source):
        root = {"limit": {"input": {"label": {"input": {"id": {"input": input_rows}}}},
                          "count": {"literal": 1}}}
        try:
            query(port, root)
        except urllib.error.HTTPError as error:
            body = json.loads(error.read())
            assert body.get("error") == "invalid_query", body
            assert "scalar terminal input" in body.get("msg", ""), body
        else:
            raise AssertionError("Label accepted a scalar input")
    print("early-termination conditional values and input type contracts passed")


if __name__ == "__main__":
    check(int(sys.argv[1]))
