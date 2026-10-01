import { readdirSync, readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

/**
 * PrimeVue's `ProgressBar` renders its label unrounded and animates the
 * fill over a full second — see `components/base/BaseProgressBar.vue`,
 * which fixes both. A second direct mount would silently bring back the
 * unrounded label and the slow fill, so only that wrapper may
 * import it.
 */
const PROGRESSBAR_IMPORT = /from\s+["']primevue\/progressbar["']/;
const WRAPPER = "components/base/BaseProgressBar.vue";
const SRC_DIR = `${import.meta.dirname}/`;

function collectSourceFiles(dir: string): string[] {
  const files: string[] = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    if (entry.isDirectory()) {
      files.push(...collectSourceFiles(`${dir}${entry.name}/`));
      continue;
    }
    if (entry.name.endsWith(".vue") || entry.name.endsWith(".ts")) {
      files.push(`${dir}${entry.name}`);
    }
  }
  return files;
}

describe("ProgressBar import guard", () => {
  const sourceFiles = collectSourceFiles(SRC_DIR);

  it("finds the one file that is allowed to import it", () => {
    // Guards the guard: a moved or renamed wrapper would otherwise make
    // the test below pass with nothing ever checked.
    const wrapper = sourceFiles.find((path) => path.endsWith(WRAPPER));
    expect(wrapper).toBeDefined();
    expect(PROGRESSBAR_IMPORT.test(readFileSync(wrapper ?? "", "utf8"))).toBe(true);
  });

  it("is imported nowhere but BaseProgressBar", () => {
    const offenders = sourceFiles
      .filter((path) => !path.endsWith(WRAPPER))
      .filter((path) => PROGRESSBAR_IMPORT.test(readFileSync(path, "utf8")))
      .map((path) => path.slice(SRC_DIR.length));

    expect(offenders).toEqual([]);
  });
});
