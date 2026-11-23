// @vitest-environment node
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

/** WCAG 2.x text contrast for body-size text. */
const MIN_RATIO = 4.5;

const css = readFileSync(fileURLToPath(new URL("./style.css", import.meta.url)), "utf8");

/** The `--name: #hex;` declarations of the block that starts at `marker`. */
function declarationsAfter(marker: string): Map<string, string> {
  const start = css.indexOf(marker);
  const end = css.indexOf("\n}", start);
  return new Map(
    [...css.slice(start, end).matchAll(/--([a-z0-9-]+):\s*(#[0-9a-fA-F]{6})\s*;/g)].map((found) => [
      found[1] ?? "",
      found[2] ?? "",
    ]),
  );
}

function luminance(hex: string): number {
  const channels = [1, 3, 5].map((offset) => {
    const value = Number.parseInt(hex.slice(offset, offset + 2), 16) / 255;
    return value <= 0.03928 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * (channels[0] ?? 0) + 0.7152 * (channels[1] ?? 0) + 0.0722 * (channels[2] ?? 0);
}

function ratio(first: string, second: string): number {
  const [lighter = 0, darker = 0] = [luminance(first), luminance(second)].sort(
    (left, right) => right - left,
  );
  return (lighter + 0.05) / (darker + 0.05);
}

const themes = {
  light: declarationsAfter(":root {\n  color-scheme"),
  dark: declarationsAfter("@media (prefers-color-scheme: dark) {\n  :root {"),
};

/** WCAG 2.x non-text contrast is 3:1; the highlight is an extra cue next to the underline, so it only
 *  has to be told apart from the row, which 1.5:1 does. */
const MIN_BACKGROUND_RATIO = 1.5;
const ROW_BACKGROUNDS = ["surface-0", "surface-1", "surface-2", "accent-soft"] as const;

describe("search match highlight", () => {
  it("has its own token in each theme, mapped for Tailwind", () => {
    expect(css).toMatch(/--color-search-match:\s*var\(--search-match\);/);
    for (const tokens of Object.values(themes)) {
      expect(tokens.get("search-match")).toMatch(/^#[0-9a-fA-F]{6}$/);
    }
  });

  for (const [theme, tokens] of Object.entries(themes)) {
    it(`keeps ${MIN_RATIO}:1 for row text on the highlight in the ${theme} theme`, () => {
      const highlight = tokens.get("search-match");
      const text = tokens.get("text");
      expect(highlight, "--search-match is declared").toBeDefined();
      expect(text, "--text is declared").toBeDefined();
      expect(ratio(text ?? "", highlight ?? "")).toBeGreaterThanOrEqual(MIN_RATIO);
    });

    for (const background of ROW_BACKGROUNDS) {
      it(`stands out from ${background} by ${MIN_BACKGROUND_RATIO}:1 in the ${theme} theme`, () => {
        const highlight = tokens.get("search-match");
        const surface = tokens.get(background);
        expect(highlight, "--search-match is declared").toBeDefined();
        expect(surface, `--${background} is declared`).toBeDefined();
        expect(ratio(highlight ?? "", surface ?? "")).toBeGreaterThanOrEqual(MIN_BACKGROUND_RATIO);
      });
    }
  }
});
