import {
  BindingProjection,
  DateTime,
  defineParams,
  defineQueries,
  g,
  param,
  readBatch,
  registerRead,
  registerWrite,
  sub,
  writeBatch,
} from "../src/index.js";

const readParams = defineParams({
  tenant: param.string(),
  limit: param.i64(),
  createdAfter: param.dateTime(),
  scores: param.array(param.f64()),
  labels: param.object(param.string()),
});

const writeParams = defineParams({
  values: param.array(param.object(param.value())),
});

const queries = defineQueries({
  read: {
    find_users: registerRead((p) => readBatch().varAs("users", g().nWithLabel("User").limit(p.limit)), readParams),
  },
  write: {
    add_users: registerWrite((p) => writeBatch().varAs("users", g().addN("User", { payload: p.values })), writeParams),
  },
});

queries.call.find_users({
  tenant: "acme",
  limit: 10n,
  createdAfter: DateTime.fromMillis(0),
  scores: [1, 2],
  labels: { status: "active" },
});

queries.call.find_users({
  tenant: "acme",
  limit: 10,
  createdAfter: "2026-01-01T00:00:00Z",
  scores: [1, 2],
  labels: { status: "active" },
});

queries.call.add_users({ values: [{ id: 1, nested: { ok: true } }] });

function directFindUsers(p: typeof readParams) {
  return readBatch().varAs("users", g().nWithLabel("User").limit(p.limit));
}

const directBatch = directFindUsers(readParams);

directBatch.toJsonString();
directBatch.toDynamicRequest({ queryName: "find_users" });
directBatch.toDynamicRequest(readParams, {
  tenant: "acme",
  limit: 10n,
  createdAfter: DateTime.fromMillis(0),
  scores: [1, 2],
  labels: { status: "active" },
});
directBatch.toDynamicJson(
  readParams,
  {
    tenant: "acme",
    limit: 10,
    createdAfter: "2026-01-01T00:00:00Z",
    scores: [1, 2],
    labels: { status: "active" },
  },
  {
    queryName: "find_users",
  },
);
writeBatch()
  .varAs("users", g().addN("User", { payload: writeParams.values }))
  .toDynamicBytes(writeParams, {
    values: [{ id: 1, nested: { ok: true } }],
  });
readBatch().varAs("count", g().nWithLabel("User").count()).toDynamicJson();
readBatch()
  .varAs(
    "bindings",
    g()
      .nWithLabel("Service")
      .bind("service")
      .optional(sub().in("CREATES").bind("deployment"))
      .projectDistinctBindings([
        BindingProjection.binding("service", "$id", "service_id"),
        BindingProjection.coalesce(
          [BindingProjection.bindingRef("deployment", "$id"), BindingProjection.bindingRef("service", "$id")],
          "workload_id",
        ),
      ]),
  )
  .toDynamicJson();

// @ts-expect-error missing required parameters
queries.call.find_users({ tenant: "acme" });

// @ts-expect-error unknown parameter
queries.call.find_users({ tenant: "acme", limit: 10, createdAfter: 0, scores: [], labels: {}, extra: true });

// @ts-expect-error wrong nested object value type
queries.call.find_users({ tenant: "acme", limit: 10, createdAfter: 0, scores: [], labels: { status: 1 } });

// @ts-expect-error wrong nested array value type
queries.call.find_users({ tenant: "acme", limit: 10, createdAfter: 0, scores: ["bad"], labels: {} });

// @ts-expect-error missing direct dynamic request parameters
directBatch.toDynamicJson(readParams, { tenant: "acme" });

// @ts-expect-error unknown direct dynamic request parameter
directBatch.toDynamicJson(readParams, { tenant: "acme", limit: 10, createdAfter: 0, scores: [], labels: {}, extra: true });

// @ts-expect-error wrong direct dynamic request object parameter
directBatch.toDynamicJson(readParams, { tenant: "acme", limit: 10, createdAfter: 0, scores: [], labels: { status: 1 } });

// @ts-expect-error wrong direct dynamic request array parameter
directBatch.toDynamicJson(readParams, { tenant: "acme", limit: 10, createdAfter: 0, scores: ["bad"], labels: {} });

// @ts-expect-error write traversal is rejected by read batches
readBatch().varAs("created", g().addN("User", {}));
