import assert from "node:assert/strict";
import { createServer, type IncomingMessage, type Server } from "node:http";
import { AddressInfo } from "node:net";
import { Client, DynamicQueryRequest, HelixError, SourcePredicate, g, readBatch } from "../src/index.js";

interface CapturedRequest {
  method: string;
  path: string;
  headers: IncomingMessage["headers"];
  body: string;
}

interface CaptureServer {
  base: string;
  captured: Promise<CapturedRequest>;
  close: () => Promise<void>;
}

/**
 * Spawn a one-shot HTTP server on a random port that captures the first request
 * and replies with the supplied status/body. Analogue of the Rust
 * `spawn_capture_server` helper in `lib.rs`.
 */
function spawnCaptureServer(response: { status?: number; body?: string } = {}): Promise<CaptureServer> {
  return new Promise((resolveServer) => {
    const server: Server = createServer((req, res) => {
      const chunks: Buffer[] = [];
      req.on("data", (chunk: Buffer) => chunks.push(chunk));
      req.on("end", () => {
        resolveCaptured({
          method: req.method ?? "",
          path: req.url ?? "",
          headers: req.headers,
          body: Buffer.concat(chunks).toString("utf8"),
        });
        res.writeHead(response.status ?? 200, { "Content-Type": "application/json" });
        res.end(response.body ?? "{}");
      });
    });

    let resolveCaptured!: (value: CapturedRequest) => void;
    const captured = new Promise<CapturedRequest>((resolve) => {
      resolveCaptured = resolve;
    });

    server.listen(0, "127.0.0.1", () => {
      const { port } = server.address() as AddressInfo;
      resolveServer({
        base: `http://127.0.0.1:${port}`,
        captured,
        close: () => new Promise<void>((resolve) => server.close(() => resolve())),
      });
    });
  });
}

function sampleRequest(): DynamicQueryRequest {
  return DynamicQueryRequest.read(
    readBatch()
      .varAs("user", g().nWhere(SourcePredicate.eq("username", "alice")))
      .returning(["user"]),
  );
}

// ---- Client construction ----------------------------------------------------

{
  const client = new Client();
  assert.equal(client.baseUrl, "http://localhost:6969/");
}

{
  const client = new Client("https://cluster.helix-db.com");
  assert.equal(client.baseUrl, "https://cluster.helix-db.com/");
}

assert.throws(
  () => new Client("not a url"),
  (error: unknown) => error instanceof HelixError && error.kind === "InvalidUrl",
);

// ---- Request routing + headers (dynamic) ------------------------------------

{
  const server = await spawnCaptureServer();
  const client = new Client(server.base).withApiKey("hx_secret");
  const result = await client.query<Record<string, unknown>>().warmOnly().writerOnly().dynamic(sampleRequest()).send();

  const req = await server.captured;
  await server.close();

  assert.equal(req.method, "POST");
  assert.equal(req.path, "/v1/query");
  assert.equal(req.headers["content-type"], "application/json");
  assert.equal(req.headers["authorization"], "Bearer hx_secret");
  assert.equal(req.headers["x-helix-warm"], "true");
  assert.equal(req.headers["x-helix-require-writer"], "true");
  assert.equal(req.body, sampleRequest().toJsonString());
  assert.deepEqual(result, {});
}

// ---- Request routing (stored) + durability header ---------------------------

{
  const server = await spawnCaptureServer({ body: '{"ok":true}' });
  const client = new Client(server.base);
  const result = await client
    .query<Record<string, unknown>>()
    .shouldAwaitDurability(false)
    .body({ name: "alice" })
    .stored("add_user")
    .send();

  const req = await server.captured;
  await server.close();

  assert.equal(req.path, "/v1/query/add_user");
  assert.equal(req.headers["x-helix-await-durable"], "false");
  assert.equal(req.headers["authorization"], undefined);
  assert.equal(req.body, '{"name":"alice"}');
  assert.deepEqual(result, { ok: true });
}

// ---- Non-200 response surfaces a remote error -------------------------------

{
  const server = await spawnCaptureServer({ status: 500, body: "boom" });
  const client = new Client(server.base);
  await assert.rejects(
    client.query().stored("add_user").send(),
    (error: unknown) => error instanceof HelixError && error.kind === "Remote" && error.details === "boom",
  );
  await server.close();
}

// ---- Unreachable server surfaces an actionable network error ----------------

{
  const client = new Client("http://127.0.0.1:1");
  await assert.rejects(
    client.query().stored("add_user").send(),
    (error: unknown) =>
      error instanceof HelixError &&
      error.kind === "Network" &&
      error.message.includes("http://127.0.0.1:1/v1/query/add_user") &&
      error.message.includes("helix start"),
  );
}

console.log("client.test.ts passed");
