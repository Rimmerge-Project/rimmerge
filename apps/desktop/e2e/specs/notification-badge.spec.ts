import { expect, type Page, test } from "@playwright/test";
// Type-only: pulls in `Window.__E2E_MOCK_IPC__`'s ambient declaration.
import type {} from "../../src/e2e-mock-bootstrap";
import { installScenario } from "../fixtures/scenario";

/** When set, each case also writes its screenshot here (for a human to look at). */
const SHOT_DIR = process.env["BADGE_SHOT_DIR"];

/**
 * Replaces the scenario's `list_notifications` with `count` distinct
 * update-available notices, so the bell's count badge has something to
 * draw. Runs after `installScenario` (init scripts run in order) and
 * mutates the same fixtures object `installMockIpc` reads from.
 */
async function seedNotifications(page: Page, count: number): Promise<void> {
  await page.addInitScript(installScenario);
  await page.addInitScript((total: number) => {
    const fixtures = window.__E2E_MOCK_IPC__;
    if (!fixtures) {
      throw new Error("scenario fixtures missing");
    }
    fixtures["list_notifications"] = () =>
      Array.from({ length: total }, (_, index) => ({
        key: { kind: "updateAvailable", fingerprint: `9.0.${index}` },
        severity: "info",
        actions: ["showReleasePage", "openSettings"],
        dismissal: "occurrence",
        data: {
          kind: "updateAvailable",
          running: "1.0.0",
          latestVersion: `9.0.${index}`,
          latestPublishedAt: "2026-01-01T00:00:00Z",
        },
      }));
  }, count);
  await page.goto("/");
  await page.getByTestId("load-project-button").click();
  await page.waitForURL(/\/$/);
}

/**
 * True when the badge's box lies inside every ancestor that clips its
 * overflow -- the exact failure a clipped badge shows.
 */
async function isBadgeUnclipped(page: Page): Promise<boolean> {
  return page.evaluate(() => {
    const badge = document.querySelector('[data-testid="notification-bell-badge"]');
    if (!badge) {
      return false;
    }
    const box = badge.getBoundingClientRect();
    for (let node = badge.parentElement; node; node = node.parentElement) {
      const overflow = getComputedStyle(node);
      const clips = [overflow.overflowX, overflow.overflowY].some((value) => value !== "visible");
      if (!clips) {
        continue;
      }
      const clip = node.getBoundingClientRect();
      const inside =
        box.left >= clip.left - 0.5 &&
        box.right <= clip.right + 0.5 &&
        box.top >= clip.top - 0.5 &&
        box.bottom <= clip.bottom + 0.5;
      if (!inside) {
        return false;
      }
    }
    return true;
  });
}

test.describe("notification badge", () => {
  for (const scheme of ["light", "dark"] as const) {
    for (const count of [1, 12, 120]) {
      test(`the ${count}-notification count badge is not clipped (${scheme})`, async ({ page }) => {
        await page.emulateMedia({ colorScheme: scheme });
        await seedNotifications(page, count);

        const badge = page.getByTestId("notification-bell-badge");
        // The badge caps at 99+ (`NotificationBell.vue`).
        await expect(badge).toHaveText(count > 99 ? "99+" : String(count));
        if (SHOT_DIR) {
          await page
            .getByRole("complementary")
            .screenshot({ path: `${SHOT_DIR}/badge-${count}-${scheme}.png` });
        }

        expect(await isBadgeUnclipped(page)).toBe(true);
      });
    }
  }
});
