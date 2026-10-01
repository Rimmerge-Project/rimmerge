import type { Page } from "@playwright/test";

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
