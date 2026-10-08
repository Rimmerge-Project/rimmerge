import { expect, type Page, test } from "@playwright/test";
// Type-only: pulls in `Window.__E2E_MOCK_IPC__`'s ambient declaration.
import type {} from "../../src/e2e-mock-bootstrap";
import { loadScenario } from "./support";

type ClipboardWindow = Window & { __CLIPBOARD__?: string[] };

// Invented ids throughout. The mock's install holds `mod.000` (Core) to `mod.059`, all active in
// ModsConfig.xml, plus `mod.901` to `mod.910` installed but inactive.
const PASTED_LIST = [
  "# RimWorld 1.6.4871",
  "1. Core [mod.000]",
  "2. Mod 1 [mod.001]",
  "3. Inactive Mod 1 [mod.901]",
  "4. Example Framework [example.framework] <https://steamcommunity.com/sharedfiles/filedetails/?id=1234567890>",
  "5. Other Mod [example.other] <https://steamcommunity.com/sharedfiles/filedetails/?id=2222222222>",
  "6. Mod 2 [mod.002]",
].join("\n");

/** Replaces the clipboard with a recorder, so a copy is observable and cannot be blocked. */
async function stubClipboard(page: Page): Promise<void> {
  await page.addInitScript(() => {
    const written: string[] = [];
    (window as ClipboardWindow).__CLIPBOARD__ = written;
    Object.defineProperty(navigator, "clipboard", {
      value: {
        writeText: async (text: string) => {
          written.push(text);
        },
      },
      configurable: true,
    });
  });
}

async function clipboardWrites(page: Page): Promise<string[]> {
  return page.evaluate(() => (window as ClipboardWindow).__CLIPBOARD__ ?? []);
}

/** Closes every toast on screen, so one never stands between a spec and the page beneath it. */
async function dismissToasts(page: Page): Promise<void> {
  const toasts = page.locator(".p-toast-message");
  for (let remaining = await toasts.count(); remaining > 0; remaining = await toasts.count()) {
    await page.locator(".p-toast-close-button").first().click();
    // A closed toast plays its leave animation and still counts until it is gone; clicking the
    // same button again would wait for an element that is about to detach.
    await expect.poll(() => toasts.count()).toBeLessThan(remaining);
  }
}

async function openOrderPage(page: Page): Promise<void> {
  await loadScenario(page);
  await page.getByTestId("nav-order").click();
  await expect(page).toHaveURL(/\/order$/);
}

async function pasteAndPreview(page: Page, text: string): Promise<void> {
  await page.getByTestId("order-import-button").click();
  await page.getByTestId("order-import-paste").click();
  await page.getByTestId("paste-order-text").fill(text);
  await page.getByTestId("paste-order-preview").click();
}

test.describe("sharing a load order", () => {
  test("Copy as text puts the file's order on the clipboard and says how many mods", async ({
    page,
  }) => {
    await stubClipboard(page);
    await openOrderPage(page);

    await page.getByTestId("order-export-button").click();
    await expect(page.getByTestId("order-export-hint")).toContainText("what RimWorld loads");
    await page.getByTestId("order-export-copy").click();

    await expect(page.getByText("Copied 60 mods.")).toBeVisible();
    const [text] = await clipboardWrites(page);
    expect(text?.split("\n")[0]).toBe("# RimWorld 1.6.4871");
    expect(text).toContain("1. Mod 0 [mod.000]");
    expect(await page.evaluate(() => window.__EXPORT_ORDER_TEXT_CALLS__)).toBe(1);
  });

  test("a toast never covers the Export and Import buttons", async ({ page }) => {
    await stubClipboard(page);
    await openOrderPage(page);
    await page.getByTestId("order-export-button").click();
    await page.getByTestId("order-export-copy").click();
    await expect(page.getByText("Copied 60 mods.")).toBeVisible();

    for (const testId of ["order-export-button", "order-import-button"]) {
      const button = page.getByTestId(testId);
      const box = await button.boundingBox();
      expect(box).not.toBeNull();
      const toastBox = await page.locator(".p-toast-message").first().boundingBox();
      expect(toastBox).not.toBeNull();
      if (box && toastBox) {
        const overlapsVertically =
          box.y < toastBox.y + toastBox.height && toastBox.y < box.y + box.height;
        const overlapsHorizontally =
          box.x < toastBox.x + toastBox.width && toastBox.x < box.x + box.width;
        expect(overlapsVertically && overlapsHorizontally).toBe(false);
      }
    }
    // Still clickable with the toast up: no waiting for it to clear.
    await page.getByTestId("order-import-button").click();
    await expect(page.getByTestId("order-import-paste")).toBeVisible();
  });

  test("Save starts in RimWorld's ModLists folder, writes the chosen .rml and toasts the count", async ({
    page,
  }) => {
    await openOrderPage(page);
    await page.evaluate(() => {
      window.__SUGGESTED_MOD_LIST_PATH__ = "C:/Saves/ModLists/rimmerge-load-order.rml";
      window.__SAVE_PATH_RESULT__ = "C:/Saves/ModLists/mine.rml";
    });

    await page.getByTestId("order-export-button").click();
    await page.getByTestId("order-export-save").click();

    await expect(page.getByText("Saved 60 mods to mine.rml.")).toBeVisible();
    expect(await page.evaluate(() => window.__EXPORT_ORDER_FILE_CALLS__)).toEqual([
      { path: "C:/Saves/ModLists/mine.rml" },
    ]);
    const options = await page.evaluate(() => window.__SAVE_DIALOG_OPTIONS__);
    expect(options).toEqual([
      {
        filters: [{ name: "RimWorld mod list", extensions: ["rml"] }],
        defaultPath: "C:/Saves/ModLists/rimmerge-load-order.rml",
      },
    ]);
  });

  test("a cancelled save dialog writes nothing and says nothing", async ({ page }) => {
    await openOrderPage(page);

    await page.getByTestId("order-export-button").click();
    await page.getByTestId("order-export-save").click();

    // The save dialog has been asked and answered "cancelled" before anything is asserted.
    await expect.poll(() => page.evaluate(() => window.__SAVE_DIALOG_OPTIONS__?.length)).toBe(1);
    await expect(page.getByTestId("order-export-button")).toBeEnabled();
    expect(await page.evaluate(() => window.__EXPORT_ORDER_FILE_CALLS__)).toEqual([]);
    await expect(page.getByText(/Saved \d+ mods?/)).toHaveCount(0);
  });

  test("a save target that is not a .rml is refused and worded as a failed save", async ({
    page,
  }) => {
    await openOrderPage(page);
    await page.evaluate(() => {
      window.__SAVE_PATH_RESULT__ = "C:/Users/me/ModsConfig.xml";
    });

    await page.getByTestId("order-export-button").click();
    await page.getByTestId("order-export-save").click();

    await expect(page.getByText("Couldn't save the mod list.")).toBeVisible();
    expect(await page.evaluate(() => window.__EXPORT_ORDER_FILE_CALLS__)).toEqual([
      { path: "C:/Users/me/ModsConfig.xml" },
    ]);
  });

  test("a pasted list previews what activates, deactivates and is missing, with Workshop buttons", async ({
    page,
  }) => {
    await stubClipboard(page);
    await openOrderPage(page);

    await pasteAndPreview(page, PASTED_LIST);

    await expect(page.getByTestId("import-preview-summary")).toHaveText("6 mods in the list");
    await expect(page.getByTestId("import-preview-activated")).toContainText("Inactive Mod 1");
    await expect(page.getByTestId("import-preview-deactivated-heading")).toHaveText(
      "Will be deactivated (57)",
    );
    await expect(page.getByTestId("import-preview-missing-heading")).toHaveText(
      "Not installed (2)",
    );
    await expect(page.getByTestId("import-preview-moved")).toHaveText(
      "The installed mods keep their current order.",
    );
    expect(await page.evaluate(() => window.__PREVIEW_ORDER_IMPORT_TEXT_CALLS__)).toEqual([
      { text: PASTED_LIST },
    ]);

    await page.getByTestId("import-preview-workshop-1234567890").click();
    expect(await page.evaluate(() => window.__OPEN_WORKSHOP_PAGE_CALLS__)).toEqual([
      { workshopId: "1234567890" },
    ]);

    await page.getByTestId("import-preview-copy-missing").click();
    await expect(page.getByText("Copied the missing list.")).toBeVisible();
    expect(await clipboardWrites(page)).toEqual([
      [
        "1. Example Framework [example.framework] <https://steamcommunity.com/sharedfiles/filedetails/?id=1234567890>",
        "2. Other Mod [example.other] <https://steamcommunity.com/sharedfiles/filedetails/?id=2222222222>",
      ].join("\n"),
    ]);
    expect(await page.evaluate(() => window.__IMPORT_ORDER_CALLS__)).toEqual([]);
  });

  test("Use this order rescans with the previewed order, shows it as Current and notes it is unwritten", async ({
    page,
  }) => {
    await openOrderPage(page);
    await expect(page.getByTestId("order-not-in-file")).toBeHidden();
    await pasteAndPreview(page, PASTED_LIST);

    await page.getByTestId("import-preview-confirm").click();

    await expect(page.getByTestId("import-preview-dialog")).toBeHidden();
    await expect(
      page.getByText("Imported order ready. Review it, then Apply to write ModsConfig.xml."),
    ).toBeVisible();
    expect(await page.evaluate(() => window.__IMPORT_ORDER_CALLS__)).toEqual([
      { order: ["mod.000", "mod.001", "mod.901", "mod.002"] },
    ]);
    await expect(page.getByRole("heading", { level: 1 })).toContainText("Current");
    const rows = page.locator('[data-testid^="order-row-mod."]');
    await expect(rows).toHaveCount(4);
    await expect(rows.nth(2)).toHaveAttribute("data-testid", "order-row-mod.901");
    await expect(page.getByTestId("order-not-in-file")).toContainText(
      "This order isn't in ModsConfig.xml yet.",
    );
    // Nothing was written by the import itself.
    expect(await page.evaluate(() => window.__APPLY_CALLS__ ?? [])).toEqual([]);
  });

  test("Apply after an import writes the Current order", async ({ page }) => {
    await openOrderPage(page);
    await pasteAndPreview(page, PASTED_LIST);
    await page.getByTestId("import-preview-confirm").click();
    await expect(page.getByTestId("order-not-in-file")).toBeVisible();

    await page.getByTestId("order-not-in-file-apply").click();
    await expect(page.getByTestId("apply-dialog-order-source")).toHaveText("Current");
    await page.getByTestId("apply-dialog-submit").click();
    // The scenario's findings leave a hard problem in this order, so Apply asks first.
    await page.getByTestId("apply-confirm-apply-anyway").click();
    await expect(page.getByTestId("apply-dialog")).toBeHidden();

    expect(await page.evaluate(() => window.__APPLY_CALLS__ ?? [])).toEqual([
      { source: "current", writeModsConfig: true, writeMergeMod: false, force: false },
    ]);
    await expect(page.getByTestId("order-not-in-file")).toBeHidden();
  });

  test("a second preview warns that the unwritten imported order will be replaced", async ({
    page,
  }) => {
    await openOrderPage(page);
    await pasteAndPreview(page, PASTED_LIST);
    await expect(page.getByTestId("import-preview-replaces-unwritten")).toBeHidden();
    await page.getByTestId("import-preview-confirm").click();
    await expect(page.getByTestId("import-preview-dialog")).toBeHidden();
    await dismissToasts(page);

    await pasteAndPreview(page, "mod.000\nmod.001");

    await expect(page.getByTestId("import-preview-replaces-unwritten")).toContainText(
      "This import replaces your Current order",
    );
  });

  test("while the import scans, the dialog shows progress and cannot be dismissed", async ({
    page,
  }) => {
    await openOrderPage(page);
    await page.evaluate(() => {
      window.__HOLD_IMPORT_ORDER__ = true;
    });
    await pasteAndPreview(page, PASTED_LIST);

    await page.getByTestId("import-preview-confirm").click();

    await expect(page.getByTestId("import-preview-progress")).toBeVisible();
    await expect(page.getByTestId("import-preview-confirm")).toBeDisabled();
    await expect(page.getByTestId("import-preview-cancel")).toBeDisabled();
    expect(await page.evaluate(() => window.__IMPORT_ORDER_CALLS__?.length)).toBe(1);

    await page.evaluate(() => window.__RELEASE_IMPORT_ORDER__?.());
    await expect(page.getByTestId("import-preview-dialog")).toBeHidden();
  });

  test("a file with a DTD is rejected with its reason and no way to import it", async ({
    page,
  }) => {
    await openOrderPage(page);
    await page.evaluate(() => {
      window.__PICK_FILE_RESULT__ = "C:/lists/hostile.rml";
      window.__ORDER_FILES__ = {
        "C:/lists/hostile.rml":
          '<?xml version="1.0"?><!DOCTYPE x [<!ENTITY e "boom">]><savedModList/>',
      };
    });

    await page.getByTestId("order-import-button").click();
    await page.getByTestId("order-import-file").click();

    await expect(page.getByTestId("import-preview-rejected-reason")).toHaveText(
      "The file declares a DTD, which mod lists never do.",
    );
    await expect(page.getByTestId("import-preview-confirm")).toHaveCount(0);
    expect(await page.evaluate(() => window.__PREVIEW_ORDER_IMPORT_FILE_CALLS__)).toEqual([
      { path: "C:/lists/hostile.rml" },
    ]);
    await page.getByTestId("import-preview-close").click();
    await expect(page.getByTestId("import-preview-dialog")).toBeHidden();
  });

  test("a file saved by RimWorld previews through the same path as a paste", async ({ page }) => {
    await openOrderPage(page);
    await page.evaluate(() => {
      window.__PICK_FILE_RESULT__ = "C:/lists/shared.rml";
      window.__ORDER_FILES__ = {
        "C:/lists/shared.rml": [
          '<?xml version="1.0" encoding="utf-8"?>',
          "<savedModList>",
          "<meta><gameVersion>1.5.4409 rev1</gameVersion></meta>",
          "<modList><ids><li>mod.000</li><li>mod.001</li></ids></modList>",
          "</savedModList>",
        ].join("\n"),
      };
    });

    await page.getByTestId("order-import-button").click();
    await page.getByTestId("order-import-file").click();

    await expect(page.getByTestId("import-preview-version")).toHaveText(
      "Made with RimWorld 1.5.4409 rev1; you have 1.6.4871.",
    );
    await expect(page.getByTestId("import-preview-summary")).toHaveText("2 mods in the list");
  });

  test("text with no mod entry is rejected as having no ids", async ({ page }) => {
    await openOrderPage(page);

    await pasteAndPreview(page, "hello there, this is not a list");

    await expect(page.getByTestId("import-preview-rejected-reason")).toHaveText(
      "No package ids were found.",
    );
    await expect(page.getByTestId("paste-order-dialog")).toBeHidden();
  });

  test("an unreadable file is a toast with its code's sentence, not a preview", async ({
    page,
  }) => {
    await openOrderPage(page);
    await page.evaluate(() => {
      window.__PICK_FILE_RESULT__ = "C:/lists/gone.rml";
    });

    await page.getByTestId("order-import-button").click();
    await page.getByTestId("order-import-file").click();

    await expect(page.getByText("Couldn't read or write the mod list")).toBeVisible();
    await expect(page.getByTestId("import-preview-dialog")).toHaveCount(0);
  });

  test("Preview stays disabled until something is pasted, and a cancelled paste starts empty next time", async ({
    page,
  }) => {
    await openOrderPage(page);
    await page.getByTestId("order-import-button").click();
    await page.getByTestId("order-import-paste").click();
    await expect(page.getByTestId("paste-order-preview")).toBeDisabled();

    await page.getByTestId("paste-order-text").fill("mod.000");
    await expect(page.getByTestId("paste-order-preview")).toBeEnabled();
    await page.getByTestId("paste-order-cancel").click();
    await page.getByTestId("order-import-button").click();
    await page.getByTestId("order-import-paste").click();

    await expect(page.getByTestId("paste-order-text")).toHaveValue("");
    expect(await page.evaluate(() => window.__PREVIEW_ORDER_IMPORT_TEXT_CALLS__)).toEqual([]);
  });

  test("a list naming nothing installed beside Core says so and cannot be imported", async ({
    page,
  }) => {
    await openOrderPage(page);

    await pasteAndPreview(page, "1. Core [mod.000]\n2. Ghost [example.ghost]");

    await expect(page.getByTestId("import-preview-blocked")).toHaveText(
      "None of the listed mods are installed, so there is nothing to import.",
    );
    await expect(page.getByTestId("import-preview-confirm")).toBeDisabled();
    expect(await page.evaluate(() => window.__IMPORT_ORDER_CALLS__)).toEqual([]);
  });

  test("a list made with another RimWorld version shows the version note, a same one does not", async ({
    page,
  }) => {
    await openOrderPage(page);

    await pasteAndPreview(page, "# RimWorld 1.5.4409 rev1\n1. Core [mod.000]\n2. Mod 1 [mod.001]");
    await expect(page.getByTestId("import-preview-version")).toHaveText(
      "Made with RimWorld 1.5.4409 rev1; you have 1.6.4871.",
    );
    await page.getByTestId("import-preview-cancel").click();

    await pasteAndPreview(page, "# RimWorld 1.6.1 rev9\n1. Core [mod.000]\n2. Mod 1 [mod.001]");
    await expect(page.getByTestId("import-preview-summary")).toHaveText("2 mods in the list");
    await expect(page.getByTestId("import-preview-version")).toHaveCount(0);
    await page.getByTestId("import-preview-cancel").click();

    await pasteAndPreview(page, "# RimWorld nightly\n1. Core [mod.000]\n2. Mod 1 [mod.001]");
    await expect(page.getByTestId("import-preview-summary")).toHaveText("2 mods in the list");
    await expect(page.getByTestId("import-preview-version")).toHaveCount(0);
  });

  test("re-importing your own export keeps the merge mod at the end and deactivates nothing", async ({
    page,
  }) => {
    await stubClipboard(page);
    await openOrderPage(page);
    await page.evaluate(() => window.__PUT_MERGE_MOD_IN_FILE__?.());
    await page.getByTestId("order-export-button").click();
    await page.getByTestId("order-export-copy").click();
    await expect(page.getByText("Copied 60 mods.")).toBeVisible();
    const [exported] = await clipboardWrites(page);
    expect(exported).not.toContain("rimmerge.merge.");
    await dismissToasts(page);

    await pasteAndPreview(page, exported ?? "");

    await expect(page.getByTestId("import-preview-summary")).toHaveText("60 mods in the list");
    await expect(page.getByTestId("import-preview-deactivated")).toHaveCount(0);
    await expect(page.getByTestId("import-preview-activated")).toHaveCount(0);
    await expect(page.getByTestId("import-preview-moved")).toHaveText(
      "The installed mods keep their current order.",
    );
    await expect(page.getByTestId("import-preview-blocked")).toHaveCount(0);
    await page.getByTestId("import-preview-confirm").click();
    await expect(page.getByTestId("import-preview-dialog")).toBeHidden();
    const [call] = (await page.evaluate(() => window.__IMPORT_ORDER_CALLS__)) ?? [];
    expect(call?.order).toHaveLength(61);
    expect(call?.order.at(-1)).toBe("rimmerge.merge.3f9a1c2b7d5e");
    expect(call?.order.slice(0, 60)).not.toContain("rimmerge.merge.3f9a1c2b7d5e");
  });
});
