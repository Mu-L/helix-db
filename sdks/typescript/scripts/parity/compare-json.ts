import { readFile, readdir } from "node:fs/promises";
import { join, relative } from "node:path";
import { canonicalizeJson, parseJsonStructural, structuralJsonEqual } from "../../src/index.js";
import { rustGeneratedRoot, typescriptGeneratedRoot } from "./paths.js";

const rustFiles = await jsonFiles(rustGeneratedRoot);
const tsFiles = await jsonFiles(typescriptGeneratedRoot);

const rustSet = new Set(rustFiles);
const tsSet = new Set(tsFiles);
const missingInTs = rustFiles.filter((file) => !tsSet.has(file));
const missingInRust = tsFiles.filter((file) => !rustSet.has(file));
if (missingInTs.length || missingInRust.length) {
  throw new Error(
    [
      missingInTs.length ? `missing TypeScript fixtures:\n${missingInTs.join("\n")}` : "",
      missingInRust.length ? `missing Rust fixtures:\n${missingInRust.join("\n")}` : "",
    ]
      .filter(Boolean)
      .join("\n\n"),
  );
}

const mismatches: string[] = [];
for (const file of rustFiles) {
  const rustJson = await readFile(join(rustGeneratedRoot, file), "utf8");
  const tsJson = await readFile(join(typescriptGeneratedRoot, file), "utf8");
  if (!structuralJsonEqual(rustJson, tsJson)) {
    mismatches.push(
      `${file}\nRust: ${JSON.stringify(canonicalizeJson(parseJsonStructural(rustJson)))}\nTS:   ${JSON.stringify(canonicalizeJson(parseJsonStructural(tsJson)))}`,
    );
  }
}

if (mismatches.length) {
  throw new Error(`request JSON parity failed for ${mismatches.length} fixture(s):\n\n${mismatches.join("\n\n")}`);
}

console.log(`request JSON parity passed for ${rustFiles.length} fixture(s)`);

async function jsonFiles(root: string, dir = ""): Promise<string[]> {
  const entries = await readdir(join(root, dir), { withFileTypes: true });
  const files = await Promise.all(
    entries.map(async (entry) => {
      const rel = join(dir, entry.name);
      if (entry.isDirectory()) return jsonFiles(root, rel);
      if (entry.isFile() && entry.name.endsWith(".json")) return [rel];
      return [];
    }),
  );
  return files.flat().sort((a, b) => a.localeCompare(b));
}

void relative;
