// @vitest-environment node
import { readdirSync, readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { flattenMessages, isMetadataKey } from "@/i18n/localeMessages";
import en from "@/locales/en.json";

const enMessages = flattenMessages(en);

/**
 * Every quoted run of text in `sourceText`, as a set: the text between a matching pair of `"`
 * or `'` that holds no quote or newline itself. The lookahead makes the scan overlapping, so a
 * quote that closes one run can open the next (`"a"b.c"` yields both `a` and `b.c`). For a key
 * with no quote or newline in it, `set.has(key)` is exactly
 * `text.includes('"' + key + '"') || text.includes("'" + key + "'")`, at one pass over the text
 * instead of two substring scans per key.
 */
function quotedLiterals(sourceText: string): Set<string> {
  const literals = new Set<string>();
  for (const match of sourceText.matchAll(/(?=(["'])([^"'\n]*)\1)/g)) {
    const literal = match[2];
    if (literal === undefined) {
      throw new Error("invariant: the pattern always captures the quoted text");
    }
    literals.add(literal);
  }
  return literals;
}

describe("quotedLiterals", () => {
  it("finds double- and single-quoted runs", () => {
    expect([...quotedLiterals(`t("a.b") + t('c.d')`)].sort()).toEqual(["a.b", "c.d"]);
  });

  it("lets a closing quote open the next run", () => {
    expect(quotedLiterals(`"a"b.c"`).has("b.c")).toBe(true);
  });

  it("does not pair a double quote with a single quote", () => {
    expect(quotedLiterals(`"a.b'`).has("a.b")).toBe(false);
  });

  it("finds a run inside a longer line of text", () => {
    expect(quotedLiterals("say 'x.y' now").has("x.y")).toBe(true);
  });

  it("does not span a newline", () => {
    expect(quotedLiterals(`"a\nb"`).size).toBe(0);
  });
});

describe("locale catalogue unused-key check", () => {
  const SRC_DIR = `${import.meta.dirname}/../`;
  const EXCLUDED_DIR_SEGMENTS = ["/locales/", "/types/generated/"];

  function collectSourceFiles(dir: string): string[] {
    const files: string[] = [];
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const full = `${dir}${entry.name}`;
      if (entry.isDirectory()) {
        files.push(...collectSourceFiles(`${full}/`));
        continue;
      }
      if (
        (entry.name.endsWith(".ts") || entry.name.endsWith(".vue")) &&
        !entry.name.endsWith(".test.ts") &&
        !EXCLUDED_DIR_SEGMENTS.some((segment) => full.includes(segment))
      ) {
        files.push(full);
      }
    }
    return files;
  }

  it("every en key is referenced as a literal somewhere in src", () => {
    const sourceText = collectSourceFiles(SRC_DIR)
      .map((path) => readFileSync(path, "utf8"))
      .join("\n");
    const literals = quotedLiterals(sourceText);

    const unused = [...enMessages.keys()].filter((key) => {
      // `primevue.*` is never looked up key-by-key through `t()` — its
      // whole subtree is bulk-copied onto PrimeVue's own reactive
      // `config.locale` by `i18n.ts`'s `setAppLocale` (see
      // `messages.primevue`, a property access, not a per-key string
      // literal), so this check does not apply to it.
      if (isMetadataKey(key) || key.startsWith("primevue.")) {
        return false;
      }
      // Guards the set lookup: a key with a quote in it would need the substring scan.
      expect(key, "an en key with a quote or newline needs the substring scan").not.toMatch(
        /["'\n]/,
      );
      return !literals.has(key);
    });

    expect(unused).toEqual([]);
  });
});
