#!/usr/bin/env node
// Fails when `apps/desktop/src/types/generated/*.ts` (the ts-rs bindings)
// don't match what `cargo test -p rimmerge-desktop` would regenerate from
// the Rust DTOs — the source of truth. Git-free by design (a checksum of
// the directory's contents, taken before and after regenerating), so it
// works the same whether or not the generated files are tracked, staged,
// or dirty for unrelated reasons.

import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { readdirSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const scriptsDir = path.dirname(fileURLToPath(import.meta.url));
const desktopDir = path.resolve(scriptsDir, "..");
const workspaceRoot = path.resolve(desktopDir, "../..");
const generatedDir = path.join(desktopDir, "src/types/generated");

/**
 * A single hash over every file's name and content in `dir`, order
 *-independent within the hash (names are sorted first) — a diff in any
 * generated file, an added file, or a removed one all change it.
 */
function hashDirectory(dir) {
  const hash = createHash("sha256");
  for (const fileName of readdirSync(dir).sort()) {
    hash.update(fileName);
    hash.update(readFileSync(path.join(dir, fileName)));
  }
  return hash.digest("hex");
}

const before = hashDirectory(generatedDir);

execFileSync("cargo", ["test", "-p", "rimmerge-desktop", "--lib"], {
  cwd: workspaceRoot,
  stdio: "inherit",
});

const after = hashDirectory(generatedDir);

if (before !== after) {
  console.error(
    "\napps/desktop/src/types/generated/*.ts drifted from the Rust DTOs.\n" +
      "`cargo test -p rimmerge-desktop` just regenerated them to match — review the diff and commit it.",
  );
  process.exit(1);
}

console.log("apps/desktop/src/types/generated/*.ts matches the Rust DTOs.");
