import { copyFile, readFile, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const [packageDirectoryArgument] = process.argv.slice(2);
if (packageDirectoryArgument === undefined) {
  throw new Error(
    "usage: node finalize-node-package.mjs <generated-package-directory>",
  );
}

const scriptDirectory = dirname(fileURLToPath(import.meta.url));
const bindingsDirectory = resolve(scriptDirectory, "..");
const repositoryRoot = resolve(bindingsDirectory, "../..");
const packageDirectory = resolve(packageDirectoryArgument);
const packagePath = resolve(packageDirectory, "package.json");
const packageJson = JSON.parse(await readFile(packagePath, "utf8"));

if (packageJson.name !== "@helix-db/helix-db-embedded") {
  throw new Error(
    `unexpected generated package name: ${String(packageJson.name)}`,
  );
}

Object.assign(packageJson, {
  version: "0.3.3",
  description:
    "Embedded HelixDB runtime and native graph algorithms for the HelixDB JavaScript SDK",
  license: "Apache-2.0",
  homepage: "https://github.com/HelixDB/helix-db",
  repository: {
    type: "git",
    url: "git+https://github.com/HelixDB/helix-db.git",
    directory: "bindings/uniffi",
  },
  bugs: {
    url: "https://github.com/HelixDB/helix-db/issues",
  },
  keywords: ["helixdb", "graph", "vector", "database", "embedded"],
  engines: {
    node: ">=20.0.0",
  },
  types: "./index.d.ts",
  exports: {
    ".": {
      types: "./index.d.ts",
      import: "./index.js",
      default: "./index.js",
    },
  },
  files: ["*.js", "*.d.ts", "runtime", "prebuilds", "README.md", "LICENSE"],
  dependencies: {
    koffi: "3.0.2",
  },
  publishConfig: {
    access: "public",
  },
});

await Promise.all([
  writeFile(packagePath, `${JSON.stringify(packageJson, null, 2)}\n`),
  copyFile(
    resolve(bindingsDirectory, "README.node.md"),
    resolve(packageDirectory, "README.md"),
  ),
  copyFile(
    resolve(repositoryRoot, "LICENSE"),
    resolve(packageDirectory, "LICENSE"),
  ),
]);
