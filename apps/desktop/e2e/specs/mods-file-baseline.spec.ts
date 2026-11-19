import { expect, type Locator, type Page, test } from "@playwright/test";
// Type-only: pulls in `Window.__E2E_MOCK_IPC__`'s ambient declaration.
import type {} from "../../src/e2e-mock-bootstrap";
import type { ModListDto } from "../../src/types/generated/ModListDto";
import { loadScenario, modsPane } from "./support";

const NO_QUERY = { search: null, tag: null, source: null, sort: null };
const OUTSIDE = "Your active mod list was changed outside Rimmerge";

type FileHook = NonNullable<Window["__MODS_CONFIG_FILE__"]>;

async function openMods(page: Page): Promise<void> {
  await page.getByTestId("nav-mods").click();
  await expect(modsPane(page, "active").getByTestId("mod-table")).toBeVisible();
}

/** The ids the session holds active now (the file's list too, until something changes it). */
async function activeIds(page: Page): Promise<string[]> {
  const list = await page.evaluate(
    (query) => window.__TAURI_INTERNALS__.invoke("list_working_mods", { query }),
    NO_QUERY,
  );
  return (list as ModListDto).items.map((row) => row.modId);
}

/** What another program does: writes the file, then the window comes back into focus. */
async function writeFileThenFocus(page: Page, file: FileHook): Promise<void> {
  await page.evaluate((next) => {
    window.__MODS_CONFIG_FILE__ = next;
    window.dispatchEvent(new Event("blur"));
    window.dispatchEvent(new Event("focus"));
  }, file);
}

async function activate(page: Page, modId: string): Promise<void> {
  await modsPane(page, "inactive").getByTestId("mods-search").fill(modId);
  await modsPane(page, "inactive").getByTestId(`mod-row-select-${modId}`).locator("input").check();
  await page.getByTestId("activate-selected-button").click();
  await page.getByTestId("activate-confirm-submit").click();
  await expect(page.getByTestId("activate-confirm-dialog")).toBeHidden();
}

async function deactivate(page: Page, modId: string): Promise<void> {
  await modsPane(page, "active").getByTestId("mods-search").fill(modId);
  await modsPane(page, "active").getByTestId(`mod-row-select-${modId}`).locator("input").check();
  await page.getByTestId("deactivate-selected-button").click();
  await page.getByTestId("deactivate-confirm-submit").click();
  await expect(page.getByTestId("deactivate-confirm-dialog")).toBeHidden();
}

/** Leaving for the project page, as the window close does: the unsaved-work confirmation, if any. */
async function tryLeaveForSetup(page: Page): Promise<void> {
  // The history is setup, dashboard, Mods: two steps back is the project page.
  await page.evaluate(() => window.history.go(-2));
}

/** Nothing here counts an outside edit as the user's own unsaved work. */
async function expectNoOwnWorkNotes(page: Page): Promise<void> {
  await expect(page.getByTestId("pending-changes-banner")).toHaveCount(0);
  await expect(page.getByTestId("order-not-in-file")).toHaveCount(0);
}

/**
 * A row's mark. The lists are virtualized, so a row far down is only in the DOM once a search
 * narrows the list to it: search for its id first.
 */
async function badgeIn(page: Page, pane: "active" | "inactive", modId: string): Promise<Locator> {
  await modsPane(page, pane).getByTestId("mods-search").fill(modId);
  return modsPane(page, pane).getByTestId(`mod-row-${modId}`).getByTestId("membership-badge");
}

test.describe("pending markers against the file", () => {
  test.beforeEach(async ({ page }) => {
    await loadScenario(page);
    await openMods(page);
  });

  test("an activation is Added, a deactivation is Removed, and undoing either clears the mark", async ({
    page,
  }) => {
    await expect(page.getByTestId("membership-badge")).toHaveCount(0);

    await activate(page, "mod.905");
    await expect(await badgeIn(page, "active", "mod.905")).toContainText("Added");

    await deactivate(page, "mod.905");
    await expect(modsPane(page, "inactive").getByTestId("mod-row-mod.905")).toBeVisible();
    await expect(page.getByTestId("membership-badge")).toHaveCount(0);

    const second = (await activeIds(page))[1];
    expect(second).toBeDefined();
    await deactivate(page, second as string);
    await expect(await badgeIn(page, "inactive", second as string)).toContainText("Removed");

    await activate(page, second as string);
    await expect(await badgeIn(page, "active", second as string)).toHaveCount(0);
    await expect(await badgeIn(page, "inactive", second as string)).toHaveCount(0);
  });

  test("a mod shows its source badge and its mark together", async ({ page }) => {
    await activate(page, "mod.905");
    await modsPane(page, "active").getByTestId("mods-search").fill("mod.905");

    const row = modsPane(page, "active").getByTestId("mod-row-mod.905");
    await expect(row.getByTestId("source-badge")).toBeVisible();
    await expect(row.getByTestId("membership-badge")).toBeVisible();
  });

  test("an outside edit is noticed on focus: marks recalculate, the notice says what changed, pending edits stay", async ({
    page,
  }) => {
    const before = await activeIds(page);
    await activate(page, "mod.905");
    await expect(await badgeIn(page, "active", "mod.905")).toBeVisible();

    // Someone else adds mod.906 to the file and removes the last active mod.
    const removed = before.at(-1) as string;
    const outside = [...before.slice(0, -1), "mod.906"];
    await writeFileThenFocus(page, { status: "ok", activeMods: outside });

    await expect(page.getByTestId("file-changed-outside-text")).toHaveText(
      `${OUTSIDE} (+1, -1). Your pending changes are kept. Applying would overwrite those outside changes.`,
    );
    // The file now lacks mod.905 and lists mod.906; the session's own edits are untouched.
    await expect(await badgeIn(page, "active", "mod.905")).toContainText("Added");
    await expect(await badgeIn(page, "inactive", "mod.906")).toContainText("Removed");
    await expect(await badgeIn(page, "active", removed)).toContainText("Added");
    expect(await activeIds(page)).toEqual([...before, "mod.905"]);
    await expect(page.getByTestId("file-changed-outside-text")).not.toContainText("ModsConfig.xml");
  });

  test("an outside reorder raises its own note, with no count, and no marks", async ({ page }) => {
    const before = await activeIds(page);
    const reordered = [before[0], before[2], before[1], ...before.slice(3)] as string[];

    await writeFileThenFocus(page, { status: "ok", activeMods: reordered });

    await expect(page.getByTestId("file-changed-outside-text")).toHaveText(
      "Your active order was changed outside Rimmerge. Applying would overwrite that order.",
    );
    await expect(await badgeIn(page, "active", before[1] as string)).toHaveCount(0);
    await expectNoOwnWorkNotes(page);

    // Putting the order back clears the note.
    await writeFileThenFocus(page, { status: "ok", activeMods: before });
    await expect(page.getByTestId("file-changed-outside")).toHaveCount(0);
  });

  test("an outside edit alone is not your own unsaved work: no guard, no banner, no not-applied note", async ({
    page,
  }) => {
    const before = await activeIds(page);

    await writeFileThenFocus(page, { status: "ok", activeMods: [...before, "mod.906"] });

    await expect(page.getByTestId("file-changed-outside-text")).toHaveText(
      `${OUTSIDE} (+1, -0). Applying would overwrite those outside changes.`,
    );
    await expectNoOwnWorkNotes(page);
    await tryLeaveForSetup(page);
    await expect(page).toHaveURL(/\/setup/);
    await expect(page.getByTestId("stale-close-confirm-dialog")).toHaveCount(0);
  });

  test("your own edit still trips the guard beside an outside edit, and only it counts", async ({
    page,
  }) => {
    const before = await activeIds(page);
    await activate(page, "mod.905");
    await writeFileThenFocus(page, { status: "ok", activeMods: [...before, "mod.906"] });
    await expect(page.getByTestId("file-changed-outside")).toBeVisible();

    await tryLeaveForSetup(page);

    await expect(page.getByTestId("stale-close-confirm-dialog")).toBeVisible();
    await page.getByTestId("stale-close-confirm-stay").click();
    await expect(page).toHaveURL(/\/mods/);
    await expect(page.getByTestId("pending-changes-unscanned-text")).toContainText("1 pending");
  });

  test("Revert to your saved order is hidden while the file was changed outside, and returns once it is not", async ({
    page,
  }) => {
    const before = await activeIds(page);
    await page.getByTestId("mod-row-menu-mod.003").click();
    await page.getByTestId("mod-move-top").click();
    await expect(page.getByTestId("order-not-in-file-revert")).toBeVisible();

    // Someone else swaps two mods in the file: the backend would adopt that order on a revert.
    const reordered = [before[0], before[2], before[1], ...before.slice(3)] as string[];
    await writeFileThenFocus(page, { status: "ok", activeMods: reordered });

    await expect(page.getByTestId("file-changed-outside")).toBeVisible();
    await expect(page.getByTestId("order-not-in-file")).toBeVisible();
    await expect(page.getByTestId("order-not-in-file-revert")).toHaveCount(0);

    await writeFileThenFocus(page, { status: "ok", activeMods: before });

    await expect(page.getByTestId("file-changed-outside")).toHaveCount(0);
    await expect(page.getByTestId("order-not-in-file-revert")).toBeVisible();
  });

  test("the preview of the file's list is diffed against what you have now, pending edits included", async ({
    page,
  }) => {
    const before = await activeIds(page);
    await activate(page, "mod.905");
    await writeFileThenFocus(page, {
      status: "ok",
      activeMods: [before[0], before[2], before[1], ...before.slice(3)] as string[],
    });
    await expect(page.getByTestId("file-changed-outside")).toBeVisible();

    await page.getByTestId("file-changed-outside-use").click();

    // The file lacks mod.905, which is active here: importing the file would deactivate it.
    await expect(page.getByTestId("import-preview-deactivated")).toContainText("Inactive Mod 5");
  });

  test("an outside write that matches a pending change clears its mark", async ({ page }) => {
    const before = await activeIds(page);
    await activate(page, "mod.905");
    await expect(await badgeIn(page, "active", "mod.905")).toBeVisible();

    await writeFileThenFocus(page, { status: "ok", activeMods: [...before, "mod.905"] });

    await expect(await badgeIn(page, "active", "mod.905")).toHaveCount(0);
    await expect(page.getByTestId("file-changed-outside-text")).toContainText("(+1, -0)");
  });

  test("a polling tick that finds nothing new changes nothing and invalidates nothing", async ({
    page,
  }) => {
    const done = await page.evaluate(() => window.__REFRESH_BASELINE_DONE__ ?? 0);
    const emits = await page.evaluate(() => window.__BASELINE_CHANGED_EMITS__ ?? 0);
    const rowFetches = await page.evaluate(() => window.__LIST_WORKING_MODS_CALLS__?.length ?? 0);

    await page.evaluate(() => {
      window.dispatchEvent(new Event("blur"));
      window.dispatchEvent(new Event("focus"));
    });
    // Wait for the end of the handler, not its start: a refresh that wrongly emitted would have
    // done so by then.
    await expect
      .poll(() => page.evaluate(() => window.__REFRESH_BASELINE_DONE__ ?? 0))
      .toBeGreaterThan(done);

    expect(await page.evaluate(() => window.__BASELINE_CHANGED_EMITS__ ?? 0)).toBe(emits);
    expect(await page.evaluate(() => window.__LIST_WORKING_MODS_CALLS__?.length ?? 0)).toBe(
      rowFetches,
    );
    await expect(page.getByTestId("file-changed-outside")).toHaveCount(0);
  });

  test("an unreadable file shows a notice, keeps the marks from the last good list, and recovers", async ({
    page,
  }) => {
    const before = await activeIds(page);
    await activate(page, "mod.905");
    await expect(await badgeIn(page, "active", "mod.905")).toBeVisible();

    await writeFileThenFocus(page, { status: "malformed" });

    await expect(page.getByTestId("file-baseline-stale")).toContainText(
      "ModsConfig.xml can't be understood right now",
    );
    await expect(await badgeIn(page, "active", "mod.905")).toContainText("Added");
    await expect(page.getByTestId("file-changed-outside")).toHaveCount(0);

    await writeFileThenFocus(page, { status: "ok", activeMods: before });

    await expect(page.getByTestId("file-baseline-stale")).toHaveCount(0);
    await expect(await badgeIn(page, "active", "mod.905")).toContainText("Added");
  });

  for (const [status, text] of [
    ["missing", "ModsConfig.xml is missing."],
    ["unreadable", "Rimmerge can't read ModsConfig.xml right now."],
  ] as const) {
    test(`a ${status} file is worded on its own`, async ({ page }) => {
      await writeFileThenFocus(page, { status });

      await expect(page.getByTestId("file-baseline-stale")).toContainText(text);
    });
  }

  test("Apply makes the file match, so the marks and the notice are gone", async ({ page }) => {
    await activate(page, "mod.905");
    await page.getByTestId("pending-changes-rescan-button").click();
    await expect(page.getByTestId("pending-changes-banner")).toBeHidden();
    await expect(await badgeIn(page, "active", "mod.905")).toBeVisible();

    await page.getByTestId("shell-apply-button").click();
    await page.getByTestId("apply-dialog-submit").click();
    await page.getByTestId("apply-confirm-apply-anyway").click();
    await expect.poll(() => page.evaluate(() => window.__APPLY_CALLS__?.length ?? 0)).toBe(1);
    await page.keyboard.press("Escape");

    await expect(await badgeIn(page, "active", "mod.905")).toHaveCount(0);
    await expect(page.getByTestId("file-changed-outside")).toHaveCount(0);
  });

  test("Apply overwrites an outside edit: the next refresh does not bring the note back", async ({
    page,
  }) => {
    const before = await activeIds(page);
    await activate(page, "mod.905");
    await page.getByTestId("pending-changes-rescan-button").click();
    await expect(page.getByTestId("pending-changes-banner")).toBeHidden();
    await writeFileThenFocus(page, { status: "ok", activeMods: [...before, "mod.906"] });
    await expect(page.getByTestId("file-changed-outside")).toBeVisible();

    await page.getByTestId("shell-apply-button").click();
    await page.getByTestId("apply-dialog-submit").click();
    await page.getByTestId("apply-confirm-apply-anyway").click();
    await expect.poll(() => page.evaluate(() => window.__APPLY_CALLS__?.length ?? 0)).toBe(1);
    await page.keyboard.press("Escape");
    const done = await page.evaluate(() => window.__REFRESH_BASELINE_DONE__ ?? 0);
    await page.evaluate(() => {
      window.dispatchEvent(new Event("blur"));
      window.dispatchEvent(new Event("focus"));
    });
    await expect
      .poll(() => page.evaluate(() => window.__REFRESH_BASELINE_DONE__ ?? 0))
      .toBeGreaterThan(done);

    await expect(page.getByTestId("file-changed-outside")).toHaveCount(0);
  });

  test("Use the list from the file opens the existing import preview, without replacing anything yet", async ({
    page,
  }) => {
    const before = await activeIds(page);
    await writeFileThenFocus(page, { status: "ok", activeMods: [...before, "mod.906"] });
    await expect(page.getByTestId("file-changed-outside")).toBeVisible();

    await page.getByTestId("file-changed-outside-use").click();

    await expect(page.getByTestId("import-preview-dialog")).toBeVisible();
    const previews = await page.evaluate(() => window.__PREVIEW_ORDER_IMPORT_FILE_CALLS__ ?? []);
    expect(previews).toEqual([{ path: "C:/Profile/ModsConfig.xml" }]);
    expect(await page.evaluate(() => window.__IMPORT_ORDER_CALLS__ ?? [])).toEqual([]);
  });
});
