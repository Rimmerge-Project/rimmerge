import path from "node:path";

import { expect, type Page, test } from "@playwright/test";
import { loadScenario } from "../specs/support";
import { waitForQuietMain } from "./settle";

/**
 * Regenerates every screenshot the public docs embed, under
 * `docs/screenshots/` (see that folder's README): the README's dashboard
 * hero (light) and the Mods page walkthrough in `docs/desktop.md` (dark).
 * Each image is driven through the mock-IPC scenario into its state, then
 * settled (toasts gone, focus and hover cleared, animations finished) so a
 * second run writes byte-identical files. Run with
 * `RIMMERGE_DOCS_SCREENSHOTS=1 bun run e2e:screenshots -- docs.spec.ts`.
 */

const IS_ENABLED = process.env["RIMMERGE_DOCS_SCREENSHOTS"] === "1";
const SCREENSHOTS_DIR = path.resolve(import.meta.dirname, "../../../../docs/screenshots");
const MODS_PAGE_DIR = path.join(SCREENSHOTS_DIR, "mods-page");

/**
 * A dialog opens with keyboard focus on its close button, and blurring it did
 * not remove the ring, so the focus outline is hidden outright instead.
 */
const NO_FOCUS_RING_STYLE = "*, *::before, *::after { outline: none !important; }";

/** Loads the scenario in the given theme, hides focus rings, and answers the first-run notice. */
async function startScenario(page: Page, colorScheme: "light" | "dark"): Promise<void> {
  await page.emulateMedia({ colorScheme });
  await loadScenario(page);
  await page.addStyleTag({ content: NO_FOCUS_RING_STYLE });
  await page.getByTestId("welcome-card-keep").click();
  await expect(page.getByTestId("welcome-card")).toBeHidden();
}

/** Dismisses every toast and waits for its leave transition, so none is left in the frame. */
async function clearToasts(page: Page): Promise<void> {
  // Retried as a unit so a toast that arrives late is closed too.
  await expect(async () => {
    for (const closeButton of await page.locator(".p-toast-close-button").all()) {
      // A dialog's modal mask sits over the toast layer, so click through the DOM, not the pointer.
      await closeButton.dispatchEvent("click");
    }
    await expect(page.locator(".p-toast-message")).toHaveCount(0, { timeout: 1_000 });
  }).toPass({ timeout: 5_000 });
}

/** Moves the pointer to a corner of the shell with nothing interactive under it. */
async function parkPointer(page: Page): Promise<void> {
  const viewport = page.viewportSize();
  if (viewport === null) {
    throw new Error("docs screenshots need a fixed viewport");
  }
  await page.mouse.move(viewport.width - 1, viewport.height - 1);
}

/** Drops keyboard focus and hover, finishes animations, and waits for the page to stop changing. */
async function settle(page: Page): Promise<void> {
  await clearToasts(page);
  await page.evaluate(() => {
    if (document.activeElement instanceof HTMLElement) {
      document.activeElement.blur();
    }
  });
  await parkPointer(page);
  await waitForQuietMain(page);
  await page.evaluate(async () => {
    // Dialogs and their masks are portaled outside `main`, and a transition can start a frame
    // after the last DOM change: wait until two frames in a row see nothing animating.
    const nextFrame = (): Promise<void> =>
      new Promise((resolve) => requestAnimationFrame(() => resolve()));
    let calmFrames = 0;
    while (calmFrames < 2) {
      // Only a running, finite animation is waited for: a finished one lingers with its fill,
      // a paused or idle one never finishes, and an endless one (a spinner) belongs in no doc
      // image (it would show up when the PNGs are reviewed).
      const running = document
        .getAnimations()
        .filter(
          (animation) =>
            animation.playState === "running" &&
            animation.effect?.getComputedTiming().iterations !== Number.POSITIVE_INFINITY,
        );
      if (running.length === 0) {
        calmFrames += 1;
      } else {
        calmFrames = 0;
        await Promise.allSettled(running.map((animation) => animation.finished));
      }
      await nextFrame();
    }
  });
}

async function capture(page: Page, filePath: string): Promise<void> {
  await settle(page);
  // Playwright creates the file's parent directories.
  await page.screenshot({ path: filePath, animations: "disabled" });
}

async function selectModRow(page: Page, modId: string): Promise<void> {
  await page.getByTestId(`mod-row-select-${modId}`).locator('input[type="checkbox"]').check();
}

async function openInactiveTab(page: Page): Promise<void> {
  await page.getByTestId("nav-mods").click();
  await page.getByTestId("mods-tab-inactive").click();
  await expect(page.getByTestId("mod-row-select-mod.905")).toBeVisible();
}

/** Activates Inactive Mod 5 with its dependency (Inactive Mod 6), leaving the active set stale. */
async function activateWithDependency(page: Page): Promise<void> {
  await selectModRow(page, "mod.905");
  await page.getByTestId("activate-with-dependencies-checkbox").locator("input").check();
  await page.getByTestId("activate-selected-button").click();
  await page.getByTestId("activate-confirm-submit").click();
  await expect(page.getByTestId("pending-changes-banner")).toBeVisible();
}

test.describe("docs screenshots", () => {
  test.skip(!IS_ENABLED, "set RIMMERGE_DOCS_SCREENSHOTS=1 to regenerate the docs screenshots");

  test("dashboard hero: rules imported, Suggested selected, Apply is the current step", async ({
    page,
  }) => {
    await startScenario(page, "light");

    await page.getByTestId("guide-rules-button").click();
    await expect(page.getByTestId("guide-step-rules")).toHaveAttribute("data-status", "done");
    await expect(page.getByTestId("guide-step-apply")).toHaveAttribute("data-status", "current");
    await expect(page.getByTestId("order-source-suggested")).toHaveAttribute(
      "aria-checked",
      "true",
    );

    await capture(page, path.join(SCREENSHOTS_DIR, "dashboard-suggested.png"));
  });

  test("mods page: select, activate, pending banners, deactivate", async ({ page }) => {
    await startScenario(page, "dark");
    await page.getByTestId("nav-mods").click();
    await expect(page.getByTestId("mod-table")).toBeVisible();
    await capture(page, path.join(MODS_PAGE_DIR, "active-tab.png"));

    await openInactiveTab(page);
    await selectModRow(page, "mod.902");
    await selectModRow(page, "mod.905");
    await capture(page, path.join(MODS_PAGE_DIR, "inactive-tab-selection.png"));

    await page.getByTestId("activate-with-dependencies-checkbox").locator("input").check();
    await page.getByTestId("activate-selected-button").click();
    await expect(page.getByTestId("activate-confirm-dialog")).toBeVisible();
    await capture(page, path.join(MODS_PAGE_DIR, "activate-dialog.png"));

    await page.getByTestId("activate-confirm-submit").click();
    await expect(page.getByTestId("pending-changes-banner")).toBeVisible();
    await capture(page, path.join(MODS_PAGE_DIR, "pending-changes-banner.png"));

    // Rescan folds the activations in; deactivating one of them afterwards
    // stacks a second kind of pending change into the same banner.
    await page.getByTestId("pending-changes-rescan-button").click();
    await expect(page.getByTestId("pending-changes-banner")).toBeHidden();
    await page.getByTestId("mods-tab-active").click();
    await page.getByTestId("mods-search").fill("mod.90");
    await selectModRow(page, "mod.906");
    await page.getByTestId("deactivate-selected-button").click();
    await expect(page.getByTestId("deactivate-plan-dependents")).toBeVisible();
    await capture(page, path.join(MODS_PAGE_DIR, "deactivate-dialog.png"));

    await page.getByTestId("deactivate-confirm-submit").click();
    await expect(page.getByTestId("pending-changes-unscanned-text")).toBeVisible();
    await expect(page.getByTestId("pending-changes-unapplied-text")).toBeVisible();
    await capture(page, path.join(MODS_PAGE_DIR, "pending-changes-banner-two-lines.png"));
  });

  test("mods page: the Apply dialog before and after a rescan", async ({ page }) => {
    await startScenario(page, "dark");
    await openInactiveTab(page);
    await activateWithDependency(page);

    await page.getByTestId("shell-apply-button").click();
    await expect(page.getByTestId("apply-dialog-stale-active-set")).toBeVisible();
    await capture(page, path.join(MODS_PAGE_DIR, "apply-dialog-stale-warning.png"));

    await page.getByTestId("apply-dialog-rescan-button").click();
    await expect(page.getByTestId("apply-dialog-stale-active-set")).toBeHidden();
    await expect(page.getByTestId("apply-dialog-unapplied-active-changes")).toBeVisible();
    await capture(page, path.join(MODS_PAGE_DIR, "apply-dialog-after-rescan.png"));

    await page.keyboard.press("Escape");
    await expect(page.getByTestId("apply-dialog")).toBeHidden();
    await capture(page, path.join(MODS_PAGE_DIR, "mods-page-after-rescan.png"));
  });
});
