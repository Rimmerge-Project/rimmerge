import { readdirSync, readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

/**
 * Pinia Colada's `invalidateQueries` rejects when any refetch fails, so a call whose promise
 * is dropped is an unhandled rejection and one a mutation returns fails an already-committed
 * write. Only `queries/invalidation.ts` may call it; everything else goes through
 * `startInvalidation` (background) or `awaitInvalidation` (the refetch must land first).
 */
// The bare name, so destructuring (`const { invalidateQueries } = cache`) and an optional
// call (`cache.invalidateQueries?.()`) are caught as well as `cache.invalidateQueries(`.
const RAW_CALL = /\binvalidateQueries\b/;
const HELPER = "queries/invalidation.ts";
const SRC_DIR = `${import.meta.dirname}/`;

function collectSourceFiles(dir: string): string[] {
  const files: string[] = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    if (entry.isDirectory()) {
      files.push(...collectSourceFiles(`${dir}${entry.name}/`));
      continue;
    }
    const isSource = entry.name.endsWith(".vue") || entry.name.endsWith(".ts");
    if (isSource && !entry.name.endsWith(".test.ts")) {
      files.push(`${dir}${entry.name}`);
    }
  }
  return files;
}

/** Code only: a comment may name the call it explains, even on the same line as code. */
function codeLines(source: string): string[] {
  return source
    .split("\n")
    .filter((line) => !/^\s*\*/.test(line))
    .map((line) => line.replace(/\/\*.*?\*\//g, "").replace(/\/\/.*$/, ""));
}

describe("invalidateQueries call guard", () => {
  const sourceFiles = collectSourceFiles(SRC_DIR);

  it("finds the one file that is allowed to call it", () => {
    // Guards the guard: a moved helper would otherwise make the test below
    // pass with nothing ever checked.
    const helper = sourceFiles.find((path) => path.endsWith(HELPER));
    expect(helper).toBeDefined();
    expect(codeLines(readFileSync(helper ?? "", "utf8")).some((line) => RAW_CALL.test(line))).toBe(
      true,
    );
  });

  it("is called nowhere but the invalidation helpers", () => {
    const offenders = sourceFiles
      .filter((path) => !path.endsWith(HELPER))
      .filter((path) => codeLines(readFileSync(path, "utf8")).some((line) => RAW_CALL.test(line)))
      .map((path) => path.slice(SRC_DIR.length));

    expect(offenders).toEqual([]);
  });
});
