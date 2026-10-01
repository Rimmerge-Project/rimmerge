import { readdirSync, readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

/**
 * The mod info panel renders untrusted content: a mod's own `About.xml`
 * description (BBCode/Unity rich-text runs) is never HTML, and rendering
 * it with `v-html` would be an XSS hole the moment a description
 * contained a literal `<img onerror=...>`-style string. `ModDescription.vue`
 * renders every run as text — this guard keeps a future edit (here or in
 * any sibling component under `components/mods/`) from reintroducing
 * `v-html` as a shortcut for "rich" formatting.
 */
// `v-html=` (the directive always takes a value) — not a bare mention
// of "v-html", which would also match this file's own doc comments and
// `ModDescription.vue`'s comment explaining why it avoids the directive.
const V_HTML = /v-html=/;
const SRC_DIR = `${import.meta.dirname}/`;

function collectVueFiles(dir: string): string[] {
  const files: string[] = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    if (entry.isDirectory()) {
      files.push(...collectVueFiles(`${dir}${entry.name}/`));
      continue;
    }
    if (entry.name.endsWith(".vue")) {
      files.push(`${dir}${entry.name}`);
    }
  }
  return files;
}

describe("mod info panel v-html guard", () => {
  it("no component under components/mods/ uses v-html", () => {
    const offenders = collectVueFiles(SRC_DIR)
      .filter((path) => V_HTML.test(readFileSync(path, "utf8")))
      .map((path) => path.slice(SRC_DIR.length));

    expect(offenders).toEqual([]);
  });
});
