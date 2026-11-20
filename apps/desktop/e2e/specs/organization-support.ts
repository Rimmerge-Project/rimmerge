import { expect, type Page } from "@playwright/test";
// Type-only: pulls in `Window.__E2E_MOCK_IPC__`'s ambient declaration.
import type {} from "../../src/e2e-mock-bootstrap";
import type { ModListDto } from "../../src/types/generated/ModListDto";
import { loadScenario, modsPane } from "./support";

/**
 * Helpers the organization specs share (groups, bulk actions): open the Mods page with the
 * organization recorder armed, create groups, add a mod to a group through its row menu, and
 * assert that an organization action reached no mod, no apply and no pending change.
 */

export const WIDE = { width: 1900, height: 900 } as const;

export const NO_QUERY = { search: null, tag: null, source: null, sort: null };

/** The ids the session holds active now, in working order. */
export async function workingIds(page: Page): Promise<string[]> {
  const list = await page.evaluate(
    (query) => window.__TAURI_INTERNALS__.invoke("list_working_mods", { query }),
    NO_QUERY,
  );
  return (list as ModListDto).items.map((row) => row.modId);
}

/** Opens the Mods page and returns the working ids it started with, for `expectNothingMoved`. */
export async function openMods(
  page: Page,
  viewport: { width: number; height: number } = WIDE,
): Promise<string[]> {
  await page.setViewportSize(viewport);
  await page.addInitScript(() => {
    window.__ORGANIZATION_CALLS__ = [];
  });
  await loadScenario(page);
  await page.getByTestId("nav-mods").click();
  await expect(modsPane(page, "active").getByTestId("mod-table")).toBeVisible();
  return workingIds(page);
}

export async function createGroup(page: Page, name: string): Promise<void> {
  await page.getByTestId("group-new").click();
  await page.getByTestId("group-name-input").fill(name);
  await page.getByTestId("group-name-submit").click();
  await expect(page.getByTestId("group-name-dialog")).toBeHidden();
  await expect(page.getByTestId("group-tree").getByText(name, { exact: true })).toBeVisible();
}

/** The tree entry for the group named `name`. */
export function node(page: Page, name: string) {
  return page.getByTestId("group-tree").getByRole("treeitem", { name: new RegExp(`^${name}\\b`) });
}

export async function groupMenu(page: Page, name: string, item: string): Promise<void> {
  await node(page, name).click({ button: "right" });
  await page.getByTestId(`group-menu-${item}`).click();
}

/**
 * Opens a row's menu, once no dialog is still fading out. A closing dialog's mask keeps eating
 * pointer events until its leave animation ends; Playwright then retries the click, scrolling the
 * list to a new alignment, and the scroll event for that arrives a frame after the menu opened,
 * where a popup menu closes on any scroll. Waiting for the mask to go removes the retry.
 */
export async function openRowMenu(
  page: Page,
  pane: "active" | "inactive",
  modId: string,
): Promise<void> {
  await expect(page.locator(".p-overlay-mask")).toHaveCount(0);
  await modsPane(page, pane).getByTestId(`mod-row-menu-${modId}`).click();
}

/** Adds one mod to a group through its row menu. */
export async function addToGroup(
  page: Page,
  pane: "active" | "inactive",
  modId: string,
  name: string,
) {
  await openRowMenu(page, pane, modId);
  await page.getByTestId("mod-menu-groups").click();
  await page
    .getByTestId("mod-groups-dialog")
    .getByLabel(new RegExp(`^${name}$`))
    .click();
  await page.getByTestId("mod-groups-done").click();
  await expect(page.getByTestId("mod-groups-dialog")).toBeHidden();
}

/**
 * Nothing organization does may reach a mod: no activation, deactivation, move, apply, discard,
 * revert or import, and the working ids are what they were at the start. `editsOrganization` is
 * `false` for a spec that never sends an organization command.
 */
export async function expectNothingMoved(
  page: Page,
  before: readonly string[],
  { editsOrganization = true }: { editsOrganization?: boolean } = {},
): Promise<void> {
  const recorded = await page.evaluate(() => ({
    activate: window.__ACTIVATE_CALLS__ ?? [],
    deactivate: window.__DEACTIVATE_CALLS__ ?? [],
    move: window.__MOVE_MODS_CALLS__ ?? [],
    apply: window.__APPLY_CALLS__ ?? [],
    discard: window.__DISCARD_PENDING_CALLS__ ?? 0,
    revert: window.__REVERT_ORDER_CALLS__ ?? 0,
    importOrder: window.__IMPORT_ORDER_CALLS__ ?? [],
    importRimSort: window.__IMPORT_RIMSORT_CALLS__ ?? [],
    commands: (window.__ORGANIZATION_CALLS__ ?? []).map((call) => call.command),
  }));
  expect(recorded.activate).toEqual([]);
  expect(recorded.deactivate).toEqual([]);
  expect(recorded.move).toEqual([]);
  expect(recorded.apply).toEqual([]);
  expect(recorded.discard).toBe(0);
  expect(recorded.revert).toBe(0);
  expect(recorded.importOrder).toEqual([]);
  expect(recorded.importRimSort).toEqual([]);
  if (editsOrganization) {
    expect(recorded.commands.length).toBeGreaterThan(0);
  }
  expect(await workingIds(page)).toEqual(before);
}
