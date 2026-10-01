import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

/**
 * Tailwind v4 scans source text for literal class names at build time —
 * it never evaluates a template literal, so `` `line-clamp-${N}` ``
 * silently ships with no matching CSS rule at all (the class attribute
 * is still set in the DOM, so a Vitest test mounting the component and
 * reading `wrapper.classes()` can't tell the difference; only Tailwind's
 * own scanner, or a real browser computing the style, would). This guard
 * reads the raw source rather than the rendered output, so it fails the
 * moment the class goes dynamic again — the actual regression this file
 * once shipped.
 */
const SOURCE_PATH = `${import.meta.dirname}/ModDescription.vue`;

describe("ModDescription clamp class guard", () => {
  const source = readFileSync(SOURCE_PATH, "utf8");

  it("uses a literal `line-clamp-N` token Tailwind's scanner can see", () => {
    expect(source).toContain("line-clamp-12");
  });

  it("never rebuilds the clamp class from an interpolated template literal", () => {
    expect(source).not.toMatch(/`line-clamp-\$\{/);
  });
});
