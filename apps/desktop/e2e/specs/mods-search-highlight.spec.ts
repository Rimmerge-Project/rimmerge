import { expect, type Locator, type Page, test } from "@playwright/test";
// Type-only: pulls in `Window.__E2E_MOCK_IPC__`'s ambient declaration.
import type {} from "../../src/e2e-mock-bootstrap";
import { openMods } from "./organization-support";
import { modsPane } from "./support";

/**
 * Search matches are marked in the shown label of a Mods row (#57). The backend answers where the
 * search matched in the name and in the id; the row marks whichever of the two it is labelled
 * with, so the id/name toggle needs no new request. The scenario's generic mods are named
 * "Mod <n>" with the id `mod.<nnn>`, so a name fragment ("d 12") and an id fragment ("od.01")
 * match different text.
 */

const MATCH = '[data-testid="search-match"]';

/** The marked text of one row's label, in order. */
async function marksOf(label: Locator): Promise<string[]> {
  return label.locator(MATCH).allTextContents();
}

async function search(page: Page, text: string): Promise<void> {
  await modsPane(page, "active").getByTestId("mods-search").fill(text);
}

function listLabel(page: Page, modId: string): Locator {
  return modsPane(page, "active").getByTestId(`mod-row-name-${modId}`);
}

function groupsLabel(page: Page, folder: number, modId: string): Locator {
  return modsPane(page, "active")
    .getByTestId(`mod-entry-${folder}-${modId}`)
    .getByTestId(`mod-row-name-${modId}`);
}

async function showGroups(page: Page): Promise<void> {
  await page.evaluate(async () => {
    await window.__TAURI_INTERNALS__.invoke("create_group", {
      request: { name: "Weapons", parent: null, modIds: ["mod.012", "mod.013"] },
    });
  });
  await modsPane(page, "active").getByTestId("mods-view-groups").click();
  await expect(modsPane(page, "active").getByTestId("mod-groups-table")).toBeVisible();
}

test.describe("search highlight in the Mods lists", () => {
  test("marks a name match in name mode, and no mark appears without a search", async ({
    page,
  }) => {
    await openMods(page);
    await expect(page.locator(MATCH)).toHaveCount(0);

    await search(page, "D 12");

    await expect(listLabel(page, "mod.012")).toHaveText("Mod 12");
    await expect.poll(() => marksOf(listLabel(page, "mod.012"))).toEqual(["d 12"]);

    await search(page, "");
    await expect(page.locator(MATCH)).toHaveCount(0);
  });

  test("marks an id match in id mode, and the same row shows no mark in name mode", async ({
    page,
  }) => {
    await openMods(page);
    await expect(listLabel(page, "mod.000")).toBeVisible();
    await search(page, "OD.01");
    // The search's answer has landed once a row it excludes is gone; only then is "no mark" a fact.
    await expect(listLabel(page, "mod.000")).toHaveCount(0);
    await expect(listLabel(page, "mod.012")).toBeVisible();

    // Name mode: the label is "Mod 12", which the id search did not match in.
    await expect(listLabel(page, "mod.012")).toHaveText("Mod 12");
    expect(await marksOf(listLabel(page, "mod.012"))).toEqual([]);

    // Id mode: the same answer marks the id, with no new search typed.
    await page.getByTestId("mod-label-mode-id").click();
    await expect(listLabel(page, "mod.012")).toHaveText("mod.012");
    await expect.poll(() => marksOf(listLabel(page, "mod.012"))).toEqual(["od.01"]);
  });

  test("marks nothing in id mode for a name-only match, and marks it again in name mode", async ({
    page,
  }) => {
    await openMods(page);
    await page.getByTestId("mod-label-mode-id").click();
    await expect(listLabel(page, "mod.000")).toBeVisible();
    await search(page, "d 12");
    // Wait for the answer (a row it excludes is gone) before asserting an absence.
    await expect(listLabel(page, "mod.000")).toHaveCount(0);

    // The row is listed because of its name, but the shown label is its id: nothing to mark.
    await expect(listLabel(page, "mod.012")).toHaveText("mod.012");
    expect(await marksOf(listLabel(page, "mod.012"))).toEqual([]);

    await page.getByTestId("mod-label-mode-name").click();
    await expect.poll(() => marksOf(listLabel(page, "mod.012"))).toEqual(["d 12"]);
  });

  test("marks nothing in a label that is neither the row's name nor its id", async ({ page }) => {
    // The name the list resolves for mod.012 differs from the row's own name, as it can for a
    // name the scan read differently: the shown label then matches neither text.
    await page.addInitScript(() => {
      let fixtures: Record<string, (...args: unknown[]) => Record<string, string>> | undefined;
      Object.defineProperty(window, "__E2E_MOCK_IPC__", {
        configurable: true,
        get: () => fixtures,
        set: (value) => {
          const original = value.list_mod_names;
          value.list_mod_names = (...args: unknown[]) => ({
            ...original(...args),
            "mod.012": "Renamed Twelve",
          });
          fixtures = value;
        },
      });
    });
    await openMods(page);
    await expect(listLabel(page, "mod.000")).toBeVisible();

    await search(page, "d 12");
    await expect(listLabel(page, "mod.000")).toHaveCount(0);

    await expect(listLabel(page, "mod.012")).toHaveText("Renamed Twelve");
    expect(await marksOf(listLabel(page, "mod.012"))).toEqual([]);
    // The id's label still marks, so the absence above is the label's, not a missing answer.
    await page.getByTestId("mod-label-mode-id").click();
    await search(page, "od.012");
    await expect.poll(() => marksOf(listLabel(page, "mod.012"))).toEqual(["od.012"]);
  });

  test("a Greek search ending in a capital sigma finds it inside a name and marks that span", async ({
    page,
  }) => {
    // The whole-string lowercase writes a word-final "Σ" as "ς", so "ΑΣ" never matched "ΑΣΑ".
    await page.addInitScript(() => {
      window.__INACTIVE_MOD_NAMES__ = { "mod.901": "Μοντέλο ΑΣΑ", "mod.902": "Μοντέλο ΟΔΟΣ" };
    });
    await openMods(page);
    const pane = modsPane(page, "inactive");

    await pane.getByTestId("mods-search").fill("ΑΣ");
    await expect(pane.getByTestId("mod-row-name-mod.902")).toHaveCount(0);
    const label = pane.getByTestId("mod-row-name-mod.901");
    await expect(label).toHaveText("Μοντέλο ΑΣΑ");
    await expect.poll(() => marksOf(label)).toEqual(["ΑΣ"]);

    // A word-final "ς", as Greek is typed, finds the capital "Σ".
    await pane.getByTestId("mods-search").fill("οδος");
    await expect(pane.getByTestId("mod-row-name-mod.901")).toHaveCount(0);
    await expect.poll(() => marksOf(pane.getByTestId("mod-row-name-mod.902"))).toEqual(["ΟΔΟΣ"]);
  });

  test("the Inactive list marks its matches too", async ({ page }) => {
    await openMods(page);
    const label = modsPane(page, "inactive").getByTestId("mod-row-name-mod.901");

    await modsPane(page, "inactive").getByTestId("mods-search").fill("901");
    await page.getByTestId("mod-label-mode-id").click();

    await expect(label).toHaveText("mod.901");
    await expect.poll(() => marksOf(label)).toEqual(["901"]);
  });

  test("Groups view: marks a name match in name mode and an id match in id mode", async ({
    page,
  }) => {
    await openMods(page);
    await showGroups(page);

    await search(page, "d 12");
    await expect(groupsLabel(page, 1, "mod.012")).toHaveText("Mod 12");
    await expect.poll(() => marksOf(groupsLabel(page, 1, "mod.012"))).toEqual(["d 12"]);

    await search(page, "od.01");
    // The old answer's marks stay until the new search's answer arrives, so wait for them to go.
    await expect.poll(() => marksOf(groupsLabel(page, 1, "mod.012"))).toEqual([]);
    await expect(groupsLabel(page, 1, "mod.012")).toHaveText("Mod 12");

    await page.getByTestId("mod-label-mode-id").click();
    await expect(groupsLabel(page, 1, "mod.012")).toHaveText("mod.012");
    await expect.poll(() => marksOf(groupsLabel(page, 1, "mod.012"))).toEqual(["od.01"]);
    // The id search matched mod.013 as well, in the same folder.
    await expect.poll(() => marksOf(groupsLabel(page, 1, "mod.013"))).toEqual(["od.01"]);
  });

  test("a match is a styled span read as part of the label, not a mark element", async ({
    page,
  }) => {
    await openMods(page);
    await search(page, "d 12");
    const match = listLabel(page, "mod.012").locator(MATCH);
    await expect(match).toHaveCount(1);

    expect(await listLabel(page, "mod.012").locator("mark").count()).toBe(0);
    // No extra accessible name or role: the row's own text reads as one run.
    await expect(match).not.toHaveAttribute("role", /.+/);
    await expect(match).not.toHaveAttribute("aria-label", /.+/);
    expect(await match.evaluate((node) => getComputedStyle(node).textDecorationLine)).toBe(
      "underline",
    );
    expect(await match.evaluate((node) => getComputedStyle(node).backgroundColor)).not.toBe(
      "rgba(0, 0, 0, 0)",
    );
  });
});
