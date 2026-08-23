#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const DOCS_ROOT = path.resolve(__dirname, '..');
const SPEC_PATH = path.join(DOCS_ROOT, 'openapi.json');
const ROUTER_PATH = path.resolve(DOCS_ROOT, '../crates/server/src/http.rs');

const errors = [];
let spec;

try {
  spec = JSON.parse(fs.readFileSync(SPEC_PATH, 'utf8'));
} catch (error) {
  console.error(`openapi.json is not valid JSON: ${error.message}`);
  process.exit(1);
}

if (spec.openapi !== '3.1.0') {
  errors.push('openapi.json: openapi must be exactly 3.1.0');
}
if (!spec.info?.title?.includes('HelixDB')) {
  errors.push('openapi.json: info.title must include HelixDB');
}

const router = fs.readFileSync(ROUTER_PATH, 'utf8');
const routerOperations = new Set(
  [...router.matchAll(/\.route\("([^"]+)",\s*(get|post)\(/g)].map(
    ([, route, method]) => `${method} ${route}`,
  ),
);
const documentedOperations = new Set(
  Object.entries(spec.paths ?? {}).flatMap(([route, pathItem]) =>
    ['get', 'post', 'put', 'patch', 'delete'].flatMap((method) =>
      pathItem[method] ? [`${method} ${route}`] : [],
    ),
  ),
);

for (const operation of routerOperations) {
  if (!documentedOperations.has(operation)) {
    errors.push(`openapi.json: missing server operation ${operation}`);
  }
}
for (const operation of documentedOperations) {
  if (!routerOperations.has(operation)) {
    errors.push(`openapi.json: documents nonexistent server operation ${operation}`);
  }
}

const operationIds = [];
for (const [route, pathItem] of Object.entries(spec.paths ?? {})) {
  for (const method of ['get', 'post', 'put', 'patch', 'delete']) {
    const operation = pathItem[method];
    if (!operation) continue;
    if (!operation.operationId) {
      errors.push(`openapi.json: ${method.toUpperCase()} ${route} needs operationId`);
    } else {
      operationIds.push(operation.operationId);
    }
    if (!operation.responses || Object.keys(operation.responses).length === 0) {
      errors.push(`openapi.json: ${method.toUpperCase()} ${route} needs responses`);
    }
  }
}
if (new Set(operationIds).size !== operationIds.length) {
  errors.push('openapi.json: operationId values must be unique');
}

const querySchema = spec.components?.schemas?.QueryRequest;
if (!querySchema?.oneOf || querySchema.oneOf.length !== 2) {
  errors.push('openapi.json: QueryRequest must close over read and write variants');
}
const examples = spec.paths?.['/v2/query']?.post?.requestBody?.content?.[
  'application/json'
]?.examples;
if (!examples?.read?.value || !examples?.write?.value) {
  errors.push('openapi.json: /v2/query needs read and write request examples');
}

if (errors.length > 0) {
  for (const error of errors) console.error(error);
  process.exit(1);
}

console.log(
  `openapi.json matches ${routerOperations.size} HelixDB HTTP server operations.`,
);
