import { readdirSync, readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

/**
 * Every color in the app must come from the semantic tokens in
 * `style.css`/`theme.ts` — never
 * the raw Tailwind palette. The pattern below is built from parts
 * (an optional variant/opacity shell around a utility prefix and a
 * generic hue shape) rather than a hardcoded list of hue names, so it
 * catches any hue Tailwind ships — not just the ones this app happened
 * to use when this guard was written — and can't accidentally contain a
 * literal substring that matches its own pattern.
 */
const UTILITY_PREFIXES = [
  "bg",
  "text",
  "border(?:-[lrtbxyse])?",
  "ring(?:-offset)?",
  "outline",
  "divide",
  "placeholder",
  "shadow",
  "decoration",
  "accent",
  "caret",
  "from",
  "via",
  "to",
  "fill",
  "stroke",
];

// A raw palette color is a lowercase hue word followed by a 2-3 digit
// Tailwind shade (50, 100, ..., 950), or one of the two shade-less
// colors `white`/`black`. This app's own tokens never take this shape:
// `surface-0/1/2` (single-digit index), `accent-soft`/`status-*-soft`
// (a trailing word, not digits), and `text-text-muted`/`border-l-accent`
// (a token name, not a hue+digits pair) all fail to match either half
// of this alternation.
const HUE_SHAPE = String.raw`[a-z]+-\d{2,3}|white|black`;

// Variant stacking (`dark:`, `hover:`, `dark:hover:`, ...) and a
// trailing opacity modifier (`/50`) are both optional and combine
// freely. The lookaround boundaries stop a match from starting or
// ending mid-identifier, so a longer hyphenated token name can't have a
// false-positive substring picked out of its middle.
const RAW_PALETTE_PATTERN = new RegExp(
  String.raw`(?<![\w-])(?:[\w-]+:)*(?:${UTILITY_PREFIXES.join("|")})-(?:${HUE_SHAPE})(?:\/\d{1,3})?(?![\w-])`,
);

// `theme.ts` is the one place raw hues are expected — it hands PrimeVue
// token *references* like `"{indigo.500}"`, which don't match the
// Tailwind-utility shape above, but is exempted by name anyway as the
// palette's one sanctioned home. The `surface-card`/`table-head`
// `@utility` names never need an
// exemption here — neither is a color and neither matches
// `RAW_PALETTE_PATTERN`.
const EXEMPT_FILES = new Set(["theme.ts"]);

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

describe("raw Tailwind palette guard", () => {
  const sourceFiles = collectSourceFiles(SRC_DIR);

  it("scans at least one source file", () => {
    // Guards the guard: an empty scan (a typo'd `SRC_DIR`, a build that
    // moved `src/` out from under `import.meta.dirname`) would make the
    // test below pass vacuously with zero files ever checked.
    expect(sourceFiles.length).toBeGreaterThan(0);
  });

  it("uses only semantic color tokens, never the raw palette", () => {
    const offenders: string[] = [];
    for (const filePath of sourceFiles) {
      const relativePath = filePath.slice(SRC_DIR.length);
      if (EXEMPT_FILES.has(relativePath)) {
        continue;
      }
      const content = readFileSync(filePath, "utf8");
      if (RAW_PALETTE_PATTERN.test(content)) {
        offenders.push(relativePath);
      }
    }
    expect(offenders).toEqual([]);
  });
});
