#!/usr/bin/env python3

import argparse
import concurrent.futures
import json
import threading
import time
import urllib.error
import urllib.request

NODE_LABEL = "ConcurrentEvent"
EDGE_LABEL = "CONCURRENT_EVENT_LINK"
SEVERITY = "info"


def query(url, payload):
    request = urllib.request.Request(
        f"{url.rstrip('/')}/v2/query",
        data=json.dumps(payload).encode(),
        headers={"content-type": "application/json"},
        method="POST",
    )
    try:
        with urllib.request.urlopen(request, timeout=90) as response:
            return json.load(response)
    except urllib.error.HTTPError as error:
        body = error.read().decode(errors="replace")
        raise RuntimeError(f"query returned HTTP {error.code}: {body}") from error


def create_index(url, spec):
    response = query(
        url,
        {
            "request_type": "write",
            "query": {
                "write": {
                    "entries": [
                        {
                            "query": {
                                "name": "operation",
                                "root": {
                                    "create_index": {
                                        "spec": spec,
                                        "if_not_exists": True,
                                    }
                                },
                            }
                        }
                    ],
                    "returns": ["operation"],
                }
            },
        },
    )
    operation_id = response["operation"]["operation_id"]
    deadline = time.monotonic() + 30
    while True:
        status = query(
            url,
            {
                "request_type": "read",
                "query": {
                    "read": {
                        "entries": [
                            {
                                "query": {
                                    "name": "status",
                                    "root": {
                                        "get_index_operation": {
                                            "operation_id": operation_id
                                        }
                                    },
                                }
                            }
                        ],
                        "returns": ["status"],
                    }
                },
            },
        )["status"]["status"]
        if status == "succeeded":
            return
        if status not in ("queued", "running"):
            raise RuntimeError(f"index operation {operation_id} reached {status}")
        if time.monotonic() >= deadline:
            raise RuntimeError(f"index operation {operation_id} timed out")
        time.sleep(0.05)


def add_anchor(url):
    response = query(
        url,
        {
            "request_type": "write",
            "query": {
                "write": {
                    "entries": [
                        {
                            "query": {
                                "name": "anchor",
                                "root": {
                                    "id": {
                                        "input": {
                                            "add_n": {
                                                "label": "ConcurrentAnchor",
                                                "properties": [],
                                            }
                                        }
                                    }
                                },
                            }
                        }
                    ],
                    "returns": ["anchor"],
                }
            },
        },
    )
    return response["anchor"][0]


def insert_batch(url, anchor_id, offset, size):
    data = [{"key": f"event-{index}"} for index in range(offset, offset + size)]
    return query(
        url,
        {
            "request_type": "write",
            "query": {
                "write": {
                    "entries": [
                        {
                            "for_each": {
                                "param": "data",
                                "body": [
                                    {
                                        "query": {
                                            "name": "created",
                                            "root": {
                                                "add_n": {
                                                    "label": NODE_LABEL,
                                                    "properties": [
                                                        [
                                                            "severity",
                                                            {
                                                                "value": {
                                                                    "string": SEVERITY
                                                                }
                                                            },
                                                        ],
                                                        [
                                                            "key",
                                                            {"expr": {"param": "key"}},
                                                        ],
                                                    ],
                                                }
                                            },
                                        }
                                    },
                                    {
                                        "query": {
                                            "name": "linked",
                                            "root": {
                                                "add_e": {
                                                    "input": {
                                                        "nodes": {
                                                            "reference": {
                                                                "var": "created"
                                                            }
                                                        }
                                                    },
                                                    "label": EDGE_LABEL,
                                                    "to": {"ids": [anchor_id]},
                                                    "properties": [
                                                        [
                                                            "severity",
                                                            {
                                                                "value": {
                                                                    "string": SEVERITY
                                                                }
                                                            },
                                                        ]
                                                    ],
                                                }
                                            },
                                        }
                                    },
                                ],
                            }
                        }
                    ],
                    "returns": [],
                }
            },
            "parameters": {"data": data},
            "parameter_types": {"data": {"array": "object"}},
        },
    )


def delete_batch(url, ids):
    return query(
        url,
        {
            "request_type": "write",
            "query": {
                "write": {
                    "entries": [
                        {
                            "query": {
                                "name": "deleted",
                                "root": {
                                    "drop": {
                                        "input": {
                                            "nodes": {"reference": {"param": "ids"}}
                                        }
                                    }
                                },
                            }
                        }
                    ],
                    "returns": [],
                }
            },
            "parameters": {"ids": ids},
            "parameter_types": {"ids": {"array": "i64"}},
        },
    )


def label_predicate(label):
    return {
        "eq": {
            "left": {"property": "$label"},
            "right": {"constant": {"string": label}},
        }
    }


def indexed_predicate(label):
    return {
        "and": {
            "predicates": [
                label_predicate(label),
                {
                    "eq": {
                        "left": {"property": "severity"},
                        "right": {"constant": {"string": SEVERITY}},
                    }
                },
            ]
        }
    }


def read_state(url, anchor_id):
    response = query(
        url,
        {
            "request_type": "read",
            "query": {
                "read": {
                    "entries": [
                        {
                            "query": {
                                "name": "node_ids",
                                "root": {
                                    "id": {
                                        "input": {
                                            "nodes_where": {
                                                "predicate": label_predicate(NODE_LABEL)
                                            }
                                        }
                                    }
                                },
                            }
                        },
                        {
                            "query": {
                                "name": "node_index_count",
                                "root": {
                                    "count": {
                                        "input": {
                                            "nodes_where": {
                                                "predicate": indexed_predicate(
                                                    NODE_LABEL
                                                )
                                            }
                                        }
                                    }
                                },
                            }
                        },
                        {
                            "query": {
                                "name": "edge_label_count",
                                "root": {
                                    "count": {
                                        "input": {
                                            "edges_where": {
                                                "predicate": label_predicate(EDGE_LABEL)
                                            }
                                        }
                                    }
                                },
                            }
                        },
                        {
                            "query": {
                                "name": "edge_index_count",
                                "root": {
                                    "count": {
                                        "input": {
                                            "edges_where": {
                                                "predicate": indexed_predicate(
                                                    EDGE_LABEL
                                                )
                                            }
                                        }
                                    }
                                },
                            }
                        },
                        {
                            "query": {
                                "name": "adjacency_count",
                                "root": {
                                    "count": {
                                        "input": {
                                            "in_e": {
                                                "input": {
                                                    "nodes": {
                                                        "reference": {
                                                            "ids": [anchor_id]
                                                        }
                                                    }
                                                },
                                                "label": EDGE_LABEL,
                                            }
                                        }
                                    }
                                },
                            }
                        },
                        {
                            "query": {
                                "name": "anchor_count",
                                "root": {
                                    "count": {
                                        "input": {
                                            "nodes_where": {
                                                "predicate": {
                                                    "eq": {
                                                        "left": {"property": "$label"},
                                                        "right": {
                                                            "constant": {
                                                                "string": "ConcurrentAnchor"
                                                            }
                                                        },
                                                    }
                                                }
                                            }
                                        }
                                    }
                                },
                            }
                        },
                    ],
                    "returns": [
                        "node_ids",
                        "node_index_count",
                        "edge_label_count",
                        "edge_index_count",
                        "adjacency_count",
                        "anchor_count",
                    ],
                }
            },
        },
    )
    return (
        response["node_ids"],
        response["node_index_count"],
        response["edge_label_count"],
        response["edge_index_count"],
        response["adjacency_count"],
        response["anchor_count"],
    )


def run_concurrently(tasks):
    started = time.monotonic()
    with concurrent.futures.ThreadPoolExecutor(max_workers=len(tasks)) as executor:
        barrier = threading.Barrier(len(tasks))

        def run(task):
            barrier.wait()
            return task()

        futures = [executor.submit(run, task) for task in tasks]
        for future in futures:
            future.result()
    return time.monotonic() - started


def chunks(values, count):
    size = len(values) // count
    return [values[index * size : (index + 1) * size] for index in range(count)]


def assert_state(url, anchor_id, expected):
    counts = read_state(url, anchor_id)
    node_ids, node_index, edge_label, edge_index, adjacency, anchor_count = counts
    if (
        len(node_ids) != expected
        or node_index != expected
        or edge_label != expected
        or edge_index != expected
        or adjacency != expected
        or anchor_count != 1
    ):
        raise RuntimeError(
            "unexpected graph/index state: "
            f"node_label={len(node_ids)} node_index={node_index} "
            f"edge_label={edge_label} edge_index={edge_index} "
            f"adjacency={adjacency} anchor={anchor_count}, "
            f"expected memberships={expected} anchor=1"
        )
    if len(set(node_ids)) != len(node_ids):
        raise RuntimeError("node equality index returned duplicate IDs")
    return node_ids


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--url", required=True)
    parser.add_argument("--clients", type=int, default=8)
    parser.add_argument("--batch-size", type=int, default=128)
    args = parser.parse_args()
    if args.clients < 4 or args.clients % 2:
        parser.error("--clients must be an even number of at least four")
    if args.batch_size < 1:
        parser.error("--batch-size must be positive")

    create_index(
        args.url,
        {
            "node_equality": {
                "label": NODE_LABEL,
                "property": "severity",
                "unique": False,
            }
        },
    )
    create_index(
        args.url,
        {"edge_equality": {"label": EDGE_LABEL, "property": "severity"}},
    )
    anchor_id = add_anchor(args.url)

    expected = args.clients * args.batch_size
    insert_seconds = run_concurrently(
        [
            lambda index=index: insert_batch(
                args.url,
                anchor_id,
                index * args.batch_size,
                args.batch_size,
            )
            for index in range(args.clients)
        ]
    )
    node_ids = assert_state(args.url, anchor_id, expected)

    half = args.clients // 2
    deleted_groups = chunks(node_ids[: expected // 2], half)
    mixed_tasks = [
        lambda ids=ids: delete_batch(args.url, ids) for ids in deleted_groups
    ] + [
        lambda index=index: insert_batch(
            args.url,
            anchor_id,
            expected + index * args.batch_size,
            args.batch_size,
        )
        for index in range(half)
    ]
    mixed_seconds = run_concurrently(mixed_tasks)
    node_ids = assert_state(args.url, anchor_id, expected)

    delete_seconds = run_concurrently(
        [
            lambda ids=ids: delete_batch(args.url, ids)
            for ids in chunks(node_ids, args.clients)
        ]
    )
    assert_state(args.url, anchor_id, 0)

    print(
        json.dumps(
            {
                "clients": args.clients,
                "batch_size": args.batch_size,
                "members_per_phase": expected,
                "insert_seconds": round(insert_seconds, 3),
                "mixed_seconds": round(mixed_seconds, 3),
                "delete_seconds": round(delete_seconds, 3),
                "failures": 0,
                "final_nodes": 0,
                "final_edges": 0,
                "anchor_nodes": 1,
            },
            sort_keys=True,
        )
    )


if __name__ == "__main__":
    main()
