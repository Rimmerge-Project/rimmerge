import { expect, type Page, test } from "@playwright/test";
// Type-only: pulls in `Window.__E2E_MOCK_IPC__`'s ambient declaration.
import type {} from "../../src/e2e-mock-bootstrap";
import { loadScenario } from "./support";

// Invented names and ids. The mock's install holds `mod.000` (Core) to `mod.059`, all active.
const SAVES_FOLDER = "C:/Users/example/Saves";

function saveHeader(ids: readonly string[]): string {
  return [
    '<?xml version="1.0" encoding="utf-8"?>',
    "<savegame><meta><gameVersion>1.6.4871 rev591</gameVersion>",
    `<modIds>${ids.map((id) => `<li>${id}</li>`).join("")}</modIds>`,
    `<modSteamIds>${ids.map(() => "<li>0</li>").join("")}</modSteamIds>`,
    `<modNames>${ids.map((id) => `<li>${id}</li>`).join("")}</modNames></meta>`,
    "<game><unclosed></game>",
  ].join("\n");
}

async function openOrderPage(page: Page): Promise<void> {
  await loadScenario(page);
  await page.getByTestId("nav-order").click();
  await expect(page).toHaveURL(/\/order$/);
}

async function openSavesDialog(page: Page): Promise<void> {
  await page.getByTestId("order-import-button").click();
  await page.getByTestId("order-import-save").click();
  await expect(page.getByTestId("save-import-dialog")).toBeVisible();
}

test.describe("importing a load order from a save", () => {
  test("lists the Saves folder newest first, previews the chosen save and imports it as Current", async ({
    page,
  }) => {
    await openOrderPage(page);
    await page.evaluate(
      ({ folder, header }) => {
        window.__SAVES__ = {
          folder,
          saves: [
            { path: `${folder}/Older.rws`, name: "Older", savedAt: 1_700_000_000 },
            { path: `${folder}/Newest.rws`, name: "Newest", savedAt: 1_800_000_000 },
            { path: `${folder}/Undated.rws`, name: "Undated", savedAt: null },
          ],
          omitted: 0,
        };
        window.__ORDER_FILES__ = { [`${folder}/Newest.rws`]: header };
      },
      { folder: SAVES_FOLDER, header: saveHeader(["mod.000", "mod.001", "mod.002"]) },
    );

    await openSavesDialog(page);

    await expect(page.getByTestId("save-import-name")).toHaveText(["Newest", "Older", "Undated"]);
    expect(await page.evaluate(() => window.__LIST_SAVES_CALLS__)).toBe(1);

    await page.getByTestId("save-import-preview").first().click();

    await expect(page.getByTestId("save-import-dialog")).toBeHidden();
    await expect(page.getByTestId("import-preview-listed-version")).toHaveText(
      "From a save made with RimWorld 1.6.4871 rev591.",
    );
    expect(await page.evaluate(() => window.__PREVIEW_ORDER_IMPORT_FILE_CALLS__)).toEqual([
      { path: `${SAVES_FOLDER}/Newest.rws` },
    ]);

    await page.getByTestId("import-preview-confirm").click();

    await expect(page.getByTestId("import-preview-dialog")).toBeHidden();
    await expect(page.getByRole("heading", { level: 1 })).toContainText("Current");
    await expect(page.getByTestId("order-not-in-file")).toContainText(
      "This isn't your active order yet.",
    );
    expect(await page.evaluate(() => window.__IMPORT_ORDER_CALLS__)).toEqual([
      { order: ["mod.000", "mod.001", "mod.002"] },
    ]);
  });

  test("Browse opens the native picker in the Saves folder and previews the pick", async ({
    page,
  }) => {
    await openOrderPage(page);
    await page.evaluate(
      ({ folder, header }) => {
        window.__SAVES__ = { folder, saves: [], omitted: 0 };
        window.__PICK_FILE_RESULT__ = "D:/backups/Elsewhere.rws";
        window.__ORDER_FILES__ = { "D:/backups/Elsewhere.rws": header };
      },
      { folder: SAVES_FOLDER, header: saveHeader(["mod.000", "mod.001"]) },
    );
    await openSavesDialog(page);
    await expect(page.getByTestId("save-import-empty")).toBeVisible();

    await page.getByTestId("save-import-browse").click();

    await expect(page.getByTestId("import-preview-summary")).toHaveText("2 mods in the list");
    const [options] = (await page.evaluate(() => window.__OPEN_DIALOG_OPTIONS__)) as [
      { defaultPath?: string; filters?: { name: string; extensions: string[] }[] },
    ];
    expect(options.defaultPath).toBe(SAVES_FOLDER);
    expect(options.filters?.[0]).toEqual({ name: "RimWorld saves (.rws)", extensions: ["rws"] });
    expect(await page.evaluate(() => window.__PREVIEW_ORDER_IMPORT_FILE_CALLS__)).toEqual([
      { path: "D:/backups/Elsewhere.rws" },
    ]);
  });

  test("a truncated save is rejected with its own reason, and Use this order is not offered", async ({
    page,
  }) => {
    await openOrderPage(page);
    await page.evaluate((folder) => {
      window.__SAVES__ = {
        folder,
        saves: [{ path: `${folder}/Cut.rws`, name: "Cut", savedAt: 1_800_000_000 }],
        omitted: 0,
      };
      window.__ORDER_FILES__ = {
        [`${folder}/Cut.rws`]: "<savegame><meta><modIds><li>mod.000</li>",
      };
    }, SAVES_FOLDER);
    await openSavesDialog(page);

    await page.getByTestId("save-import-preview").click();

    await expect(page.getByTestId("import-preview-rejected-reason")).toHaveText(
      "The save ends before its mod list does. It may be damaged, or the game may still be writing it.",
    );
    await expect(page.getByTestId("import-preview-confirm")).toHaveCount(0);
  });

  test("a listed save whose preview fails is a toast above the open dialog, which stays usable", async ({
    page,
  }) => {
    await openOrderPage(page);
    await page.evaluate((folder) => {
      window.__SAVES__ = {
        folder,
        saves: [{ path: `${folder}/Gone.rws`, name: "Gone", savedAt: 1_800_000_000 }],
        omitted: 0,
      };
    }, SAVES_FOLDER);
    await openSavesDialog(page);

    await page.getByTestId("save-import-preview").click();

    // The order-share toast group is above the modal mask: the toast is seen and its close
    // button is clickable while the saves dialog is still open.
    const toast = page.locator(".p-toast-message");
    await expect(toast).toBeVisible();
    await expect(toast).toContainText("Couldn't read or write the mod list");
    await expect(page.getByTestId("save-import-dialog")).toBeVisible();
    await page.locator(".p-toast-close-button").click();
    await expect(toast).toHaveCount(0);
    await expect(page.getByTestId("save-import-dialog")).toBeVisible();
    await expect(page.getByTestId("save-import-browse")).toBeEnabled();
  });

  test("says so, and still offers Browse, when the Saves folder was not found", async ({
    page,
  }) => {
    await openOrderPage(page);

    await openSavesDialog(page);

    await expect(page.getByTestId("save-import-no-folder")).toHaveText(
      "RimWorld's Saves folder wasn't found next to your ModsConfig.xml. Browse for a save instead.",
    );
    await expect(page.getByTestId("save-import-browse")).toBeEnabled();
  });

  test("a folder that cannot be listed is worded by its own code", async ({ page }) => {
    await openOrderPage(page);
    await page.evaluate(() => {
      window.__LIST_SAVES_ERROR__ = "saves_folder_unreadable";
    });

    await openSavesDialog(page);

    await expect(page.getByTestId("save-import-error")).toContainText(
      "Couldn't read the Saves folder",
    );
    await expect(page.getByTestId("save-import-browse")).toBeEnabled();
  });

  test("lists 200 saves and counts the older ones it leaves out", async ({ page }) => {
    await openOrderPage(page);
    await page.evaluate((folder) => {
      window.__SAVES__ = {
        folder,
        saves: Array.from({ length: 203 }, (_, index) => ({
          path: `${folder}/Save${index}.rws`,
          name: `Save${index}`,
          savedAt: 1_700_000_000 + index,
        })),
        omitted: 0,
      };
    }, SAVES_FOLDER);

    await openSavesDialog(page);

    await expect(page.getByTestId("save-import-row")).toHaveCount(200);
    await expect(page.getByTestId("save-import-omitted")).toHaveText("3 older saves not shown.");
    await expect(page.getByTestId("save-import-name").first()).toHaveText("Save202");
  });
});
