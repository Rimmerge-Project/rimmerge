import { expect, type Page, test } from "@playwright/test";
// Type-only: pulls in `Window.__E2E_MOCK_IPC__`'s ambient declaration.
import type {} from "../../src/e2e-mock-bootstrap";
import { loadScenario } from "./support";

type Ipc = Record<string, (payload?: unknown) => Promise<unknown>>;

async function openModInfo(page: Page, modId: string): Promise<void> {
  await page.getByTestId("nav-mods").click();
  await page.getByTestId("mods-search").fill(modId);
  await page.getByTestId(`mod-row-${modId}`).click();
  await expect(page.getByTestId("mod-description")).toBeVisible();
}

/** The mod ids of the rendered (virtualized) table rows, in DOM order. */
async function renderedRowIds(page: Page): Promise<string[]> {
  return page
    .getByTestId("mod-table")
    .locator('[role="row"]')
    .evaluateAll((rows) =>
      rows.map((row) => (row.getAttribute("data-testid") ?? "").replace(/^mod-row-/, "")),
    );
}

test.describe("mods", () => {
  test("lists Active in the working set's own order and Inactive by name", async ({ page }) => {
    await loadScenario(page);
    await page.getByTestId("nav-mods").click();
    // The working set lists the scenario's own mods first, in their order; the fixture owners
    // that sort ahead of them by id (`core.gadget.mod`, ...) come after, as in `ModsConfig.xml`.
    await expect(page.getByTestId("mod-row-mod.000")).toBeVisible();

    const activeIds = await renderedRowIds(page);
    expect(activeIds[0]).toBe("mod.000");
    // Only the viewport is rendered, so assert the leading working-order prefix, not absence.
    expect(activeIds.slice(0, 3)).toEqual(["mod.000", "mod.001", "mod.002"]);

    await page.getByTestId("mods-tab-inactive").click();
    await expect(page.getByTestId("mod-row-mod.901")).toBeVisible();
    const inactiveIds = await renderedRowIds(page);
    // Name order, A to Z: "Inactive Mod 10" sorts before "Inactive Mod 2", so it is not id order.
    expect(inactiveIds.slice(0, 3)).toEqual(["mod.901", "mod.910", "mod.902"]);
  });

  test("every row shows its source as a badge with an accessible name, on both tabs", async ({
    page,
  }) => {
    await loadScenario(page);
    await page.getByTestId("nav-mods").click();
    await expect(page.getByTestId("mod-row-mod.000")).toBeVisible();

    const activeBadge = (id: string) =>
      page.getByTestId(`mod-row-${id}`).getByTestId("source-badge");
    await expect(activeBadge("mod.000")).toHaveAttribute("data-source", "core");
    await expect(activeBadge("mod.000")).toHaveAccessibleName("RimWorld Core");
    await expect(activeBadge("mod.001")).toHaveAccessibleName("RimWorld DLC");
    await expect(activeBadge("mod.002")).toHaveAccessibleName("Steam Workshop");
    await expect(activeBadge("mod.003")).toHaveAccessibleName("Local mod");
    await expect(activeBadge("mod.003")).toHaveAttribute("title", "Local mod");
    // The raw enum text is gone from the row.
    // `getByText` with `exact` is case-sensitive: the wire value is lowercase, no label is.
    await expect(
      page.getByTestId("mod-row-mod.002").getByText("workshop", { exact: true }),
    ).toHaveCount(0);

    await page.getByTestId("mods-tab-inactive").click();
    await expect(page.getByTestId("mod-row-mod.901")).toBeVisible();
    const inactiveBadge = (id: string) =>
      page.getByTestId(`mod-row-${id}`).getByTestId("source-badge");
    await expect(inactiveBadge("mod.901")).toHaveAccessibleName("Local mod");
    await expect(inactiveBadge("mod.902")).toHaveAccessibleName("Steam Workshop");
  });

  test("searches the mod list and opens a mod's info panel", async ({ page }) => {
    await loadScenario(page);
    await page.getByTestId("nav-mods").click();
    await expect(page).toHaveURL(/\/mods$/);
    await expect(page.getByTestId("mod-table")).toBeVisible();

    await page.getByTestId("mods-search").fill("Mod 20");
    await expect(page.getByTestId("mods-total")).toHaveText("1 mod");

    await page.getByTestId("mod-row-mod.020").click();

    await expect(page).toHaveURL(/\/mods\/mod\.020$/);
    await expect(page.getByTestId("mod-info-panel")).toBeVisible();
    await expect(page.getByTestId("mod-info-name")).toHaveText("Mod 20");
  });

  test("a one-word description has no Show more toggle", async ({ page }) => {
    await loadScenario(page);
    await openModInfo(page, "mod.020");

    await expect(page.getByTestId("mod-description-text")).toHaveText("Continued");
    await expect(page.getByTestId("mod-description-toggle")).toHaveCount(0);
  });

  test("a long description has the toggle, and expanding it grows the text", async ({ page }) => {
    await loadScenario(page);
    await openModInfo(page, "mod.021");

    const text = page.getByTestId("mod-description-text");
    const toggle = page.getByTestId("mod-description-toggle");
    await expect(toggle).toHaveText("Show more");
    const clampedHeight = await text.evaluate((element) => element.clientHeight);

    await toggle.click();

    await expect(toggle).toHaveText("Show less");
    const expandedHeight = await text.evaluate((element) => element.clientHeight);
    expect(expandedHeight).toBeGreaterThan(clampedHeight);
  });

  test("the Inactive tab resolves real names, not raw ids, and opens too", async ({ page }) => {
    await loadScenario(page);
    await page.getByTestId("nav-mods").click();
    await page.getByTestId("mods-tab-inactive").click();

    const row = page.getByTestId("mod-row-mod.901");
    await expect(row).toContainText("Inactive Mod 1");
    await expect(row).not.toContainText("mod.901");

    await row.click();
    await expect(page).toHaveURL(/\/mods\/mod\.901$/);
    await expect(page.getByTestId("mod-info-name")).toHaveText("Inactive Mod 1");
    // No position/tier/dependents/findings sections for an inactive mod.
    await expect(page.getByTestId("mod-info-facts-placement")).not.toBeAttached();
  });

  test("a mod's findings link opens the inbox filtered to it, with a removable chip", async ({
    page,
  }) => {
    // In-app navigation throughout, not `page.goto` — a full page load
    // would reset the client-only `session.loaded` state and bounce the
    // router guard back to `/setup`.
    await loadScenario(page);
    await page.getByTestId("nav-mods").click();
    await page.getByTestId("mods-search").fill("Mod 0");
    await page.getByTestId("mod-row-mod.000").click();
    await expect(page).toHaveURL(/\/mods\/mod\.000$/);

    await page.getByTestId("mod-info-findings-link").click();

    await expect(page).toHaveURL(/\/inbox\?mod=mod\.000$/);
    const chip = page.getByTestId("mod-filter-chip");
    await expect(chip).toBeVisible();
    await expect(chip).toContainText("Mod 0");

    await page.getByTestId("mod-filter-chip-clear").click();
    await expect(chip).not.toBeAttached();
  });

  test("the panel's all-details link opens the full mod detail page", async ({ page }) => {
    await loadScenario(page);
    await page.getByTestId("nav-mods").click();
    await page.getByTestId("mods-search").fill("Mod 0");
    await page.getByTestId("mod-row-mod.000").click();
    await expect(page).toHaveURL(/\/mods\/mod\.000$/);

    await page.getByTestId("mod-info-all-details-link").click();

    await expect(page).toHaveURL(/\/mods\/mod\.000\/details$/);
    await expect(page.getByTestId("mod-detail-name")).toHaveText("Mod 0");
  });

  test("the panel's homepage link opens through the backend opener, never a URL the frontend builds", async ({
    page,
  }) => {
    await loadScenario(page);
    await page.getByTestId("nav-mods").click();
    await page.getByTestId("mods-search").fill("Mod 0");
    await page.getByTestId("mod-row-mod.000").click();
    await expect(page).toHaveURL(/\/mods\/mod\.000$/);

    await page.getByTestId("mod-info-homepage-link").click();

    await expect
      .poll(() => page.evaluate(() => window.__OPEN_MOD_LINK_CALLS__ ?? []))
      .toEqual([{ modId: "mod.000", link: "homepage" }]);
  });

  test("closing the panel with Escape returns to the plain /mods list", async ({ page }) => {
    await loadScenario(page);
    await page.getByTestId("nav-mods").click();
    await page.getByTestId("mods-search").fill("Mod 0");
    await page.getByTestId("mod-row-mod.000").click();
    await expect(page.getByTestId("mod-info-panel")).toBeVisible();

    // Escape only reaches the panel's own `@keydown` listener once
    // something inside it holds focus — the close button is the one
    // naturally focusable element there.
    await page.getByTestId("mod-info-panel-close").focus();
    await page.keyboard.press("Escape");

    await expect(page).toHaveURL(/\/mods$/);
    await expect(page.getByTestId("mod-info-panel")).not.toBeAttached();
  });

  test("an Apply followed by a Rescan keeps the written order", async ({ page }) => {
    await loadScenario(page);
    const workingIds = () =>
      page.evaluate(async () => {
        const ipc = window.__E2E_MOCK_IPC__ as unknown as Ipc;
        const list = (await ipc.list_working_mods?.({
          query: { search: null, tag: null, source: null },
        })) as { items: { modId: string }[] };
        return list.items.map((row) => row.modId);
      });
    const fileMatchesCurrent = () =>
      page.evaluate(async () => {
        const ipc = window.__E2E_MOCK_IPC__ as unknown as Ipc;
        const dashboard = (await ipc.get_dashboard?.()) as { fileMatchesCurrent: boolean };
        return dashboard.fileMatchesCurrent;
      });
    const call = (name: string, payload?: unknown) =>
      page.evaluate(
        async ([command, args]) => {
          const ipc = window.__E2E_MOCK_IPC__ as unknown as Ipc;
          await ipc[command as string]?.(args);
        },
        [name, payload] as const,
      );

    const before = await workingIds();
    // A rescan that changes nothing must not make the file look different from Current.
    await call("rescan_project");
    expect(await fileMatchesCurrent(), "after a no-op rescan").toBe(true);
    await call("apply", {
      request: { source: "suggested", writeModsConfig: true, writeMergeMod: false, force: false },
    });
    const afterApply = await workingIds();
    expect(afterApply, "the working order follows the written order").not.toEqual(before);

    await call("rescan_project");

    expect(await workingIds()).toEqual(afterApply);
    expect(await fileMatchesCurrent(), "after apply then rescan").toBe(true);
  });

  test("deactivating then re-activating a mod puts it back in its place", async ({ page }) => {
    await loadScenario(page);
    await page.getByTestId("nav-mods").click();
    await expect(page.getByTestId("mod-row-mod.002")).toBeVisible();
    const before = (await renderedRowIds(page)).slice(0, 5);
    expect(before[2]).toBe("mod.002");

    await page.getByTestId("mod-row-select-mod.002").locator('input[type="checkbox"]').check();
    await page.getByTestId("deactivate-selected-button").click();
    await page.getByTestId("deactivate-confirm-submit").click();
    await expect(page.getByTestId("deactivate-confirm-dialog")).toBeHidden();

    await page.getByTestId("mods-tab-inactive").click();
    await page.getByTestId("mod-row-select-mod.002").locator('input[type="checkbox"]').check();
    await page.getByTestId("activate-selected-button").click();
    await page.getByTestId("activate-confirm-submit").click();
    await expect(page.getByTestId("activate-confirm-dialog")).toBeHidden();

    await page.getByTestId("mods-tab-active").click();
    await expect(page.getByTestId("mod-row-mod.002")).toBeVisible();
    expect((await renderedRowIds(page)).slice(0, 5)).toEqual(before);
    await expect(page.getByTestId("pending-changes-banner")).toHaveCount(0);
  });
});
