import path from "node:path";

import { expect, type Locator, type Page, test } from "@playwright/test";
// Type-only: pulls in `Window.__MODS_CONFIG_FILE__`'s ambient declaration.
import type {} from "../../src/e2e-mock-bootstrap";
import type { ModListDto } from "../../src/types/generated/ModListDto";
import { addToGroup, createGroup, node, WIDE } from "../specs/organization-support";
import { loadScenario, modsPane } from "../specs/support";
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
const SAVES_FOLDER = "C:/Users/example/Saves";
const OTHER_PROFILE = "C:/Profiles/other";
/** The other profile's `organization.json`, as the mock reads it for the copy dialog. */
const COPY_SOURCES = {
  [OTHER_PROFILE]: {
    kind: "organization",
    groups: [
      { id: 1, name: "weapons", parent: null, color: "red", members: ["mod.003", "mod.004"] },
      { id: 2, name: "Melee", parent: 1, color: null, members: ["mod.008"] },
      { id: 3, name: "Armor", parent: null, color: "teal", members: ["mod.010"] },
    ],
    modColors: { "mod.003": "pink" },
  },
};

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

/** Captures one element (settled first), so a doc image can show just the part it explains. */
async function captureLocator(page: Page, target: Locator, filePath: string): Promise<void> {
  await settle(page);
  await target.screenshot({ path: filePath, animations: "disabled" });
}

/** Captures the smallest rectangle holding every given element, with a little air around it. */
async function captureAround(
  page: Page,
  targets: readonly Locator[],
  filePath: string,
): Promise<void> {
  await settle(page);
  const boxes = await Promise.all(targets.map((target) => target.boundingBox()));
  const found = boxes.filter((box) => box !== null);
  if (found.length !== targets.length) {
    throw new Error("every element of a cropped capture must be on screen");
  }
  const padding = 12;
  const left = Math.max(0, Math.min(...found.map((box) => box.x)) - padding);
  const top = Math.max(0, Math.min(...found.map((box) => box.y)) - padding);
  const viewport = page.viewportSize();
  if (viewport === null) {
    throw new Error("docs screenshots need a fixed viewport");
  }
  const right = Math.min(
    viewport.width,
    Math.max(...found.map((box) => box.x + box.width)) + padding,
  );
  const bottom = Math.min(
    viewport.height,
    Math.max(...found.map((box) => box.y + box.height)) + padding,
  );
  await page.screenshot({
    path: filePath,
    animations: "disabled",
    clip: { x: left, y: top, width: right - left, height: bottom - top },
  });
}

/** The ids the session holds active now, in order. */
async function activeModIds(page: Page): Promise<string[]> {
  const list = await page.evaluate(() =>
    window.__TAURI_INTERNALS__.invoke("list_working_mods", {
      query: { search: null, tag: null, source: null, sort: null },
    }),
  );
  return (list as ModListDto).items.map((row) => row.modId);
}

async function selectModRow(page: Page, modId: string): Promise<void> {
  await page.getByTestId(`mod-row-select-${modId}`).locator('input[type="checkbox"]').check();
}

async function openMods(page: Page): Promise<void> {
  await page.getByTestId("nav-mods").click();
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
    await openMods(page);
    await capture(page, path.join(MODS_PAGE_DIR, "lists.png"));

    await selectModRow(page, "mod.902");
    await selectModRow(page, "mod.905");
    await capture(page, path.join(MODS_PAGE_DIR, "inactive-selection.png"));

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
    await modsPane(page, "active").getByTestId("mods-search").fill("mod.90");
    await selectModRow(page, "mod.906");
    await page.getByTestId("deactivate-selected-button").click();
    await expect(page.getByTestId("deactivate-plan-dependents")).toBeVisible();
    await capture(page, path.join(MODS_PAGE_DIR, "deactivate-dialog.png"));

    await page.getByTestId("deactivate-confirm-submit").click();
    await expect(page.getByTestId("pending-changes-unscanned-text")).toBeVisible();
    await expect(page.getByTestId("pending-changes-unapplied-text")).toBeVisible();
    await capture(page, path.join(MODS_PAGE_DIR, "pending-changes-banner-two-lines.png"));
  });

  test("mods page: moving a mod leaves an order to apply", async ({ page }) => {
    await startScenario(page, "dark");
    await openMods(page);
    await page.getByTestId("mod-row-menu-mod.004").click();
    await page.getByTestId("mod-move-top").click();
    await expect(page.getByTestId("order-not-in-file")).toBeVisible();
    await expect(page.getByTestId("order-switched-note")).toBeVisible();
    await capture(page, path.join(MODS_PAGE_DIR, "moved-order.png"));
  });

  test("mods page: the Apply dialog before and after a rescan", async ({ page }) => {
    await startScenario(page, "dark");
    await openMods(page);
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

  test("mods page: the sort toggle, with a criterion other than the default active", async ({
    page,
  }) => {
    await startScenario(page, "dark");
    await openMods(page);
    const activePane = modsPane(page, "active");
    await activePane.getByTestId("mods-sort-name").click();
    await expect(activePane.getByTestId("mods-sort-name")).toHaveAccessibleName(/Sorted by name/);

    await captureLocator(
      page,
      activePane.getByTestId("mods-sort-bar"),
      path.join(MODS_PAGE_DIR, "sort-toggle.png"),
    );
  });

  test("mods page: Added and Removed marks after the mod names", async ({ page }) => {
    await startScenario(page, "dark");
    await openMods(page);
    const activeIds = await activeModIds(page);
    // Mod 8 is a plain mid-list mod the scenario lets go without a dependents warning.
    const removedId = "mod.008";
    expect(activeIds).toContain(removedId);

    await selectModRow(page, "mod.905");
    await page.getByTestId("activate-selected-button").click();
    await page.getByTestId("activate-confirm-submit").click();
    await expect(page.getByTestId("activate-confirm-dialog")).toBeHidden();

    await modsPane(page, "active").getByTestId("mods-search").fill(removedId);
    await selectModRow(page, removedId);
    await page.getByTestId("deactivate-selected-button").click();
    await page.getByTestId("deactivate-confirm-submit").click();
    await expect(page.getByTestId("deactivate-confirm-dialog")).toBeHidden();

    // Narrow each list to its own marked row, so both marks are in the frame.
    await modsPane(page, "active").getByTestId("mods-search").fill("mod.905");
    await modsPane(page, "inactive").getByTestId("mods-search").fill(removedId);
    await expect(
      modsPane(page, "active").getByTestId("mod-row-mod.905").getByTestId("membership-badge"),
    ).toContainText("Added");
    await expect(
      modsPane(page, "inactive")
        .getByTestId(`mod-row-${removedId}`)
        .getByTestId("membership-badge"),
    ).toContainText("Removed");
    // The unfiltered inactive list already shows the Removed row, so wait for the
    // filtered answer itself before taking the picture.
    await expect(modsPane(page, "inactive").locator('[data-testid^="mod-row-mod."]')).toHaveCount(
      1,
    );
    await capture(page, path.join(MODS_PAGE_DIR, "membership-marks.png"));
  });

  test("mods page: the saved list was changed outside Rimmerge", async ({ page }) => {
    await startScenario(page, "dark");
    await openMods(page);
    const before = await activeModIds(page);

    // What another program does: writes the file, then the window comes back into focus.
    await page.evaluate(
      (outside) => {
        window.__MODS_CONFIG_FILE__ = { status: "ok", activeMods: outside };
        window.dispatchEvent(new Event("blur"));
        window.dispatchEvent(new Event("focus"));
      },
      [...before.filter((modId) => modId !== "mod.008"), "mod.906"],
    );
    await expect(page.getByTestId("file-changed-outside-text")).toContainText("(+1, -1)");
    await capture(page, path.join(MODS_PAGE_DIR, "outside-change-note.png"));
  });

  test("mods page: a moved order is not your active order yet", async ({ page }) => {
    await startScenario(page, "dark");
    await openMods(page);
    await page.getByTestId("mod-row-menu-mod.004").click();
    await page.getByTestId("mod-move-top").click();
    await expect(page.getByTestId("order-not-in-file-revert")).toBeVisible();

    await captureAround(
      page,
      [page.getByTestId("order-not-in-file"), page.getByTestId("order-switched-note")],
      path.join(MODS_PAGE_DIR, "order-not-applied-note.png"),
    );
  });

  test("mods page: the Groups panel and the bulk actions over a selection", async ({ page }) => {
    // The Groups panel is a column from 1,800 pixels wide, so these two images are wider.
    await page.setViewportSize(WIDE);
    await startScenario(page, "dark");
    await openMods(page);
    await createGroup(page, "Weapons");
    await createGroup(page, "Armor");
    await addToGroup(page, "active", "mod.004", "Weapons");
    await addToGroup(page, "active", "mod.008", "Weapons");
    await addToGroup(page, "active", "mod.010", "Armor");
    await node(page, "Weapons").click();
    await expect(page.getByTestId("mods-bulk-bar-active")).toHaveCount(0);

    await selectModRow(page, "mod.004");
    await selectModRow(page, "mod.008");
    await expect(page.getByTestId("mods-bulk-bar-active")).toBeVisible();
    await capture(page, path.join(MODS_PAGE_DIR, "groups-and-bulk-actions.png"));

    await page.getByTestId("bulk-add-to-group-active").click();
    await expect(page.getByTestId("bulk-groups-dialog")).toBeVisible();
    await capture(page, path.join(MODS_PAGE_DIR, "bulk-add-to-group-dialog.png"));
  });

  test("mods page: the Groups view with folders, colours and a search", async ({ page }) => {
    await startScenario(page, "dark");
    await openMods(page);
    await page.evaluate(async () => {
      const invoke = window.__TAURI_INTERNALS__.invoke;
      await invoke("create_group", {
        request: { name: "Weapons", parent: null, modIds: ["mod.004", "mod.008"] },
      });
      await invoke("create_group", {
        request: { name: "Armor", parent: null, modIds: ["mod.010", "mod.012"] },
      });
      await invoke("set_group_color", { request: { id: 1, color: "red" } });
      await invoke("set_group_color", { request: { id: 2, color: "teal" } });
      await invoke("set_mod_colors", { request: { modIds: ["mod.012"], color: "blue" } });
    });
    const activePane = modsPane(page, "active");
    await activePane.getByTestId("mods-view-groups").click();
    await expect(activePane.getByTestId("mod-groups-table")).toBeVisible();
    await expect(activePane.getByTestId("mod-entry-1-mod.004")).toBeVisible();
    await capture(page, path.join(MODS_PAGE_DIR, "groups-view.png"));

    await activePane.getByTestId("mods-search").fill("d 1");
    await expect(activePane.getByTestId("mod-entry-2-mod.012")).toBeVisible();
    await expect(
      activePane.getByTestId("mod-entry-2-mod.012").getByTestId("search-match"),
    ).toHaveText("d 1");
    await captureLocator(page, activePane, path.join(MODS_PAGE_DIR, "search-highlight.png"));
  });

  test("mods page: copying groups from another profile, the preview", async ({ page }) => {
    // The copy button lives in the Groups panel, a column from 1,800 pixels wide.
    await page.setViewportSize(WIDE);
    await page.addInitScript((sources) => {
      window.__COPY_PROFILE_SOURCES__ = sources as NonNullable<
        typeof window.__COPY_PROFILE_SOURCES__
      >;
    }, COPY_SOURCES);
    await startScenario(page, "dark");
    await openMods(page);
    await createGroup(page, "Weapons");
    await page.evaluate((folder) => {
      window.__PICK_FOLDER_RESULT__ = folder;
    }, OTHER_PROFILE);
    await page.getByTestId("group-copy-profile").click();
    await expect(page.getByTestId("copy-profile-dialog")).toBeVisible();
    await page.getByTestId("copy-choose").click();
    await expect(page.getByTestId("copy-preview")).toContainText("2 groups added");
    await capture(page, path.join(MODS_PAGE_DIR, "copy-groups-preview.png"));
  });

  test("apply: the file was changed outside Rimmerge, the confirm section", async ({ page }) => {
    await startScenario(page, "dark");
    await openMods(page);
    const ids = await activeModIds(page);
    await page.evaluate(
      (next) => {
        window.__MODS_CONFIG_FILE__ = { status: "ok", activeMods: next };
      },
      [...ids, "mod.905"],
    );
    await page.getByTestId("shell-apply-button").click();
    await page.getByTestId("apply-dialog-submit").click();
    await expect(page.getByTestId("apply-confirm-outside-text")).toContainText("(+1, -0)");
    await capture(page, path.join(MODS_PAGE_DIR, "apply-outside-change-confirm.png"));
  });

  test("dashboard: the note on pages other than Mods when the file changed outside", async ({
    page,
  }) => {
    await startScenario(page, "dark");
    const ids = await activeModIds(page);
    await page.evaluate(
      (next) => {
        window.__MODS_CONFIG_FILE__ = { status: "ok", activeMods: next };
        window.dispatchEvent(new Event("blur"));
        window.dispatchEvent(new Event("focus"));
      },
      [...ids, "mod.905"],
    );
    await expect(page.getByTestId("outside-change-hint")).toBeVisible();
    await captureLocator(
      page,
      page.getByTestId("outside-change-hint"),
      path.join(MODS_PAGE_DIR, "outside-change-hint.png"),
    );
  });

  test("order page: importing a load order from a save", async ({ page }) => {
    await startScenario(page, "dark");
    await page.evaluate((folder) => {
      window.__SAVES__ = {
        folder,
        saves: [
          {
            path: `${folder}/Colony of Example.rws`,
            name: "Colony of Example",
            savedAt: 1_800_000_000,
          },
          { path: `${folder}/Second try.rws`, name: "Second try", savedAt: 1_790_000_000 },
          { path: `${folder}/Older colony.rws`, name: "Older colony", savedAt: 1_700_000_000 },
        ],
        omitted: 0,
      };
    }, SAVES_FOLDER);
    await page.getByTestId("nav-order").click();
    await page.getByTestId("order-import-button").click();
    await page.getByTestId("order-import-save").click();
    await expect(page.getByTestId("save-import-dialog")).toBeVisible();
    await expect(page.getByTestId("save-import-name")).toHaveCount(3);
    await capture(page, path.join(SCREENSHOTS_DIR, "order-page", "import-from-save.png"));
  });
});
