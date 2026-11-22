import { expect, type Locator, type Page } from "@playwright/test";

import { LOCALE_OPTIONS, type SupportedLocale } from "../../src/i18n/locales";
import type { ListSortKeyDto } from "../../src/types/generated/ListSortKeyDto";
import { installScenario } from "../fixtures/scenario";

/**
 * Installs the full mock-IPC scenario and loads it through the setup
 * page's real flow (never skips straight to `/`), landing on the
 * dashboard — the same starting point every spec in this tier needs.
 */
export async function loadScenario(page: Page): Promise<void> {
  await page.addInitScript(installScenario);
  await page.goto("/");
  await page.getByTestId("load-project-button").click();
  await page.waitForURL(/\/$/);
}

/** One of the Mods page's two side-by-side lists, to scope a locator to its own controls and rows. */
export function modsPane(page: Page, kind: "active" | "inactive"): Locator {
  return page.getByTestId(`mods-pane-${kind}`);
}

/**
 * Switches the UI language through the Settings picker, as a user does, and leaves the Settings
 * page open (callers navigate on from there). A locale whose translation is not reviewed yet is
 * named with "(preview)" in the picker; `en` is the default and needs no change.
 */
export async function chooseLocale(page: Page, locale: SupportedLocale): Promise<void> {
  if (locale === "en") {
    return;
  }
  const option = LOCALE_OPTIONS.find((candidate) => candidate.locale === locale);
  if (option === undefined) {
    throw new Error(`no language picker row for ${locale}`);
  }
  const name = option.isPreview ? `${option.nativeName} (preview)` : option.nativeName;
  await page.getByTestId("nav-settings").click();
  await page.getByTestId("language-picker-select").click();
  await page.getByRole("option", { name, exact: true }).click();
  await expect(page.locator("html")).toHaveAttribute("lang", locale);
}

/** The criteria each list offers, in display order (`utils/modListSort.ts`, which this tier cannot import). */
const SORT_KEYS: Record<"active" | "inactive", readonly ListSortKeyDto[]> = {
  active: ["loadOrder", "name", "packageId", "lastUpdated"],
  inactive: ["name", "packageId", "lastUpdated"],
};

/**
 * Sorts a Mods list by one criterion, ascending, through whichever form the sort bar is in: the
 * criterion's button, or, once the bar has collapsed to a select (a narrow pane, or long words),
 * its option. The option is picked by position, so it works in any language. Choosing the
 * criterion that is already active does nothing in either form (the button would flip it).
 */
export async function chooseSort(
  page: Page,
  kind: "active" | "inactive",
  key: ListSortKeyDto,
): Promise<void> {
  const bar = modsPane(page, kind).getByTestId("mods-sort-bar");
  await expect(bar).toBeVisible();
  const select = bar.getByTestId("mods-sort-select");
  if (!(await select.isVisible())) {
    const button = bar.getByTestId(`mods-sort-${key}`);
    // The active button says so in `data-active` (its name is localized, so it cannot be matched on).
    if ((await button.getAttribute("data-active")) !== "true") {
      await button.click();
    }
    return;
  }
  const index = SORT_KEYS[kind].indexOf(key);
  await select.click();
  await page.getByRole("option").nth(index).click();
}
