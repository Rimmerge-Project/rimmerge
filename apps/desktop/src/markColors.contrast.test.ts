// @vitest-environment node
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

import { COLOR_TOKENS } from "@/utils/organization";

/** WCAG 2.x non-text contrast: a swatch or stripe is a graphical object, so 3:1. */
const MIN_RATIO = 3;
const BACKGROUNDS = ["surface-0", "surface-1", "surface-2", "accent-soft"] as const;

const css = readFileSync(fileURLToPath(new URL("./style.css", import.meta.url)), "utf8");

/** The `--name: #hex;` declarations of the first block that starts at `marker`. */
function declarationsAfter(marker: string): Map<string, string> {
  const start = css.indexOf(marker);
  const end = css.indexOf("\n}", start);
  const block = css.slice(start, end);
  return new Map(
    [...block.matchAll(/--([a-z0-9-]+):\s*(#[0-9a-fA-F]{6})\s*;/g)].map((found) => [
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

describe("mark colour tokens", () => {
  for (const [theme, tokens] of Object.entries(themes)) {
    for (const color of COLOR_TOKENS) {
      it(`${color} keeps ${MIN_RATIO}:1 against every row and panel background in the ${theme} theme`, () => {
        const mark = tokens.get(`mark-${color}`);
        expect(mark, `--mark-${color} is declared`).toBeDefined();
        for (const background of BACKGROUNDS) {
          const surface = tokens.get(background);
          expect(surface, `--${background} is declared`).toBeDefined();
          expect(
            ratio(mark ?? "", surface ?? ""),
            `${color} on ${background}`,
          ).toBeGreaterThanOrEqual(MIN_RATIO);
        }
      });
    }
  }
});
