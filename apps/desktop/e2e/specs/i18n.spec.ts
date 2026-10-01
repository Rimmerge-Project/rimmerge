import { expect, type Page, test } from "@playwright/test";
// Type-only: pulls in `Window.__E2E_MOCK_IPC__`'s ambient declaration.
import type {} from "../../src/e2e-mock-bootstrap";
import { LOCALE_OPTIONS } from "../../src/i18n/locales";
import { loadScenario } from "./support";

/**
 * The mock tier's own locale coverage: the picker mechanism itself, that
 * switching to `zh-CN`/`pt-BR` renders translated text app-wide, and that
 * every other locale works at whatever stage its translation is at (an
 * empty catalogue reads as English, never as raw keys).
 */

const RAW_KEY_PATTERN = /^[a-z]+(\.[a-zA-Z_]+)+$/;

async function expectNoRawKeyLines(page: Page): Promise<void> {
  await page.getByTestId("nav-dashboard").click();
  const bodyText = await page.locator("body").innerText();
  for (const line of bodyText.split("\n")) {
    expect(line.trim(), `looks like a raw i18n key: "${line}"`).not.toMatch(RAW_KEY_PATTERN);
  }
  await page.getByTestId("nav-settings").click();
}

test.describe("language picker", () => {
  test("defaults to English, with no raw key visible", async ({ page }) => {
    await loadScenario(page);

    await expect(page.getByTestId("nav-dashboard")).toHaveText("Dashboard");
    await expect(page.locator("html")).toHaveAttribute("lang", "en");

    const bodyText = await page.locator("body").innerText();
    for (const line of bodyText.split("\n")) {
      expect(line.trim(), `looks like a raw i18n key: "${line}"`).not.toMatch(RAW_KEY_PATTERN);
    }
  });

  test("switching to 简体中文 updates the picker immediately, with no reload, and persists", async ({
    page,
  }) => {
    await loadScenario(page);
    await page.getByTestId("nav-settings").click();

    const select = page.getByTestId("language-picker-select");
    await select.click();
    await page.getByRole("option", { name: "简体中文 (preview)" }).click();

    // Applies immediately — no navigation, no reload marker reset.
    await expect(page.locator("html")).toHaveAttribute("lang", "zh-CN");
    await expect(page.getByTestId("language-picker-select")).toContainText("简体中文");
    // The preview note's own issues link is a real button, not a printed
    // URL — it opens through the backend opener like every other
    // external link in the app.
    await expect(page.getByTestId("language-picker-issues-link")).toBeVisible();

    // The shell nav is fully translated too (not just the picker itself).
    await expect(page.getByTestId("nav-dashboard")).toHaveText("概览");

    await page.reload();
    await expect(page.locator("html")).toHaveAttribute("lang", "zh-CN");
    await expect(page.getByTestId("language-picker-select")).toContainText("简体中文");
  });

  test("switching to Deutsch shows its own preview note, and back to English removes it", async ({
    page,
  }) => {
    await loadScenario(page);
    await page.getByTestId("nav-settings").click();

    const select = page.getByTestId("language-picker-select");
    await select.click();
    await page.getByRole("option", { name: "Deutsch (preview)" }).click();

    await expect(page.locator("html")).toHaveAttribute("lang", "de");
    await expect(page.getByTestId("language-picker-preview-note")).toContainText(
      "Wenn dir eine falsche Übersetzung auffällt",
    );

    await select.click();
    await page.getByRole("option", { name: "English", exact: true }).click();

    await expect(page.locator("html")).toHaveAttribute("lang", "en");
    await expect(page.getByTestId("language-picker-preview-note")).toBeHidden();
  });

  test("a reviewed locale (Português (Brasil)) has no preview label or note", async ({ page }) => {
    await loadScenario(page);
    await page.getByTestId("nav-settings").click();

    await page.getByTestId("language-picker-select").click();
    await page.getByRole("option", { name: "Português (Brasil)", exact: true }).click();

    await expect(page.locator("html")).toHaveAttribute("lang", "pt-BR");
    await expect(page.getByTestId("language-picker-preview-note")).toBeHidden();
  });

  test("the Setup page's own picker applies before a project is loaded", async ({ page }) => {
    await page.goto("/setup");

    const select = page.getByTestId("language-picker-select");
    await select.click();
    await page.getByRole("option", { name: "简体中文 (preview)" }).click();

    await expect(page.locator("html")).toHaveAttribute("lang", "zh-CN");
    await expect(page.getByTestId("language-picker-select")).toContainText("简体中文");
  });

  /**
   * Cheap, whole-nav-tree coverage: no uncaught exception while
   * switching locale and touring every route — the way a broken
   * `MessageDescriptor` (a stale key, a missing param) would actually
   * surface. This suite's own fixture data includes plenty of
   * dot-separated mod ids (`cabin.furniture.mod`) and filenames
   * (`rimmerge.json`) indistinguishable from a raw i18n key by shape
   * alone, which is why this checks for a thrown error rather than
   * reusing `RAW_KEY_PATTERN` broadly — the narrower page-specific check
   * above already covers that shape on the one route it visits.
   * `zh-CN`/`pt-BR` are now fully translated, so a real per-locale
   * screenshot-and-layout pass (overflow, wrapping, CJK font fallback)
   * is a worthwhile follow-up this test doesn't replace.
   */
  const NAV_ROUTES = [
    "nav-dashboard",
    "nav-inbox",
    "nav-merge-mod",
    "nav-patches",
    "nav-assignments",
    "nav-startup",
    "nav-order",
    "nav-rules",
    "nav-mods",
    "nav-settings",
  ] as const;

  for (const option of LOCALE_OPTIONS.filter((candidate) => candidate.locale !== "en")) {
    const locale = option.locale;
    test(`every nav route survives ${locale} with no uncaught page error`, async ({ page }) => {
      const pageErrors: Error[] = [];
      page.on("pageerror", (error) => pageErrors.push(error));

      await loadScenario(page);
      await page.getByTestId("nav-settings").click();
      await page.getByTestId("language-picker-select").click();
      const optionName = option.isPreview ? `${option.nativeName} (preview)` : option.nativeName;
      await page.getByRole("option", { name: optionName, exact: true }).click();
      await expect(page.locator("html")).toHaveAttribute("lang", locale);
      await expectNoRawKeyLines(page);

      for (const navTestId of NAV_ROUTES) {
        await page.getByTestId(navTestId).click();
        // Something must actually be on the page — a route that failed
        // to mount at all (a thrown error before the shell's own error
        // boundary catches it) leaves an empty `<main>` a plain
        // `toBeVisible()` on the nav link wouldn't catch.
        await expect(page.locator("main")).not.toBeEmpty();
      }

      expect(pageErrors, `uncaught page error(s) under ${locale}`).toEqual([]);
    });
  }
});
