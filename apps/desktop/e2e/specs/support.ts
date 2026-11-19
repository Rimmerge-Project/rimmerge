import type { Locator, Page } from "@playwright/test";

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
