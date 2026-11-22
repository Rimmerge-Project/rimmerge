// @vitest-environment node
import { readdirSync, readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

/**
 * `<i18n-t>` defaults to `scope="parent"`, and with no parent composer vue-i18n warns
 * "Not found parent scope" on every render. This app has no component-local messages, so every
 * `<i18n-t>` names the global scope.
 */
const SRC_DIR = `${import.meta.dirname}/`;
const OPENING_TAG = /<i18n-t\b[^>]*>/g;

function collectVueFiles(dir: string): string[] {
  const files: string[] = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    if (entry.isDirectory()) {
      files.push(...collectVueFiles(`${dir}${entry.name}/`));
    } else if (entry.name.endsWith(".vue")) {
      files.push(`${dir}${entry.name}`);
    }
  }
  return files;
}

/** A file's text without doc-comment lines, which may name the tag they explain. */
function withoutDocComments(source: string): string {
  return source
    .split("\n")
    .filter((line) => !/^\s*\*/.test(line))
    .join("\n");
}

function openingTags(file: string): string[] {
  return withoutDocComments(readFileSync(file, "utf8")).match(OPENING_TAG) ?? [];
}

describe("<i18n-t> scope guard", () => {
  it("finds the usages it guards, so an empty scan cannot pass", () => {
    const count = collectVueFiles(SRC_DIR).reduce((sum, file) => sum + openingTags(file).length, 0);

    expect(count).toBeGreaterThan(0);
  });

  it("gives every <i18n-t> an explicit scope", () => {
    const offenders: string[] = [];
    for (const file of collectVueFiles(SRC_DIR)) {
      for (const tag of openingTags(file)) {
        if (!/\sscope=/.test(tag)) {
          offenders.push(`${file}: ${tag.replace(/\s+/g, " ").slice(0, 80)}`);
        }
      }
    }

    expect(offenders).toEqual([]);
  });
});
