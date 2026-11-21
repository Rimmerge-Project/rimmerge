import { expect, type Page, test } from "@playwright/test";
// Type-only: pulls in `Window.__E2E_MOCK_IPC__`'s ambient declaration.
import type {} from "../../src/e2e-mock-bootstrap";
import type { ModListDto } from "../../src/types/generated/ModListDto";
import { loadScenario } from "./support";

const NO_QUERY = { search: null, tag: null, source: null, sort: null };

/** The ids the session holds active now (the file's list too, until something changes it). */
async function activeIds(page: Page): Promise<string[]> {
  const list = await page.evaluate(
    (query) => window.__TAURI_INTERNALS__.invoke("list_working_mods", { query }),
    NO_QUERY,
  );
  return (list as ModListDto).items.map((row) => row.modId);
}

/** What another program does between load and Apply: writes the file, with no poll in between. */
async function writeFileOutside(page: Page, activeMods: string[]): Promise<void> {
  await page.evaluate((next) => {
    window.__MODS_CONFIG_FILE__ = { status: "ok", activeMods: next };
  }, activeMods);
}

/** The same write, then the window coming back into focus, which makes the poller read it. */
async function writeFileThenFocus(page: Page, activeMods: string[]): Promise<void> {
  await writeFileOutside(page, activeMods);
  await page.evaluate(() => {
    window.dispatchEvent(new Event("blur"));
    window.dispatchEvent(new Event("focus"));
  });
}

/**
 * Makes the baseline poll a no-op: a refresh answers with what the baseline already holds, as if
 * the 5 s timer had not fired yet. Only a preflight that reads the file itself can then see an
 * outside edit.
 *
 * Two assumptions: `@tauri-apps/api/core` looks up `window.__TAURI_INTERNALS__.invoke` at call
 * time (so replacing it takes effect for the app's own calls), and no helper may install the mocks
 * again afterwards (a reload or a second `loadScenario` would put the original `invoke` back).
 */
async function silencePoller(page: Page): Promise<void> {
  await page.evaluate(() => {
    const internals = window.__TAURI_INTERNALS__;
    const original = internals.invoke.bind(internals);
    const forward = original as (...args: unknown[]) => Promise<unknown>;
    internals.invoke = ((command: string, ...rest: unknown[]) =>
      forward(
        command === "refresh_mods_config_baseline" ? "get_mods_config_baseline" : command,
        ...rest,
      )) as typeof internals.invoke;
  });
}

test.describe("apply and an outside change of the file", () => {
  test("an edit made after load is found when Apply is pressed, and Go back is the default", async ({
    page,
  }) => {
    await loadScenario(page);
    const ids = await activeIds(page);
    await silencePoller(page);
    await writeFileOutside(page, [...ids, "mod.905"]);
    const baselineKind = await page.evaluate(
      async () =>
        (
          (await window.__TAURI_INTERNALS__.invoke("get_mods_config_baseline")) as {
            externalChange: { kind: string };
          }
        ).externalChange.kind,
    );
    expect(baselineKind, "the poll has not seen the edit").toBe("none");

    await page.getByTestId("dashboard-apply-button").click();
    await page.getByTestId("apply-dialog-submit").click();

    await expect(page.getByTestId("apply-confirm")).toBeVisible();
    await expect(page.getByTestId("apply-confirm-outside-text")).toContainText("(+1, -0)");
    await expect(page.getByTestId("apply-confirm-go-back")).toBeFocused();
    expect(await page.evaluate(() => window.__APPLY_CALLS__ ?? [])).toEqual([]);
  });

  test("an edit made after the dialog's own first read is found at the click, not served from the cache", async ({
    page,
  }) => {
    await loadScenario(page);
    const ids = await activeIds(page);
    await silencePoller(page);
    await page.getByTestId("dashboard-apply-button").click();
    await expect(page.getByTestId("apply-dialog-problems-summary")).toBeVisible();
    // The dialog's preflight has resolved (it shows its summary); now the file changes, well
    // inside the query's 5 s stale time.
    await writeFileOutside(page, [...ids, "mod.905"]);

    await page.getByTestId("apply-dialog-submit").click();

    await expect(page.getByTestId("apply-confirm-outside-text")).toContainText("(+1, -0)");
  });

  test("Go back spends the baseline's one-shot change: the hint shows on the Dashboard afterwards", async ({
    page,
  }) => {
    await loadScenario(page);
    const ids = await activeIds(page);
    await silencePoller(page);
    await writeFileOutside(page, [...ids, "mod.905"]);
    await page.getByTestId("dashboard-apply-button").click();
    await page.getByTestId("apply-dialog-submit").click();
    await expect(page.getByTestId("apply-confirm-outside-text")).toBeVisible();

    await page.getByTestId("apply-confirm-go-back").click();
    await page.getByTestId("apply-dialog-cancel").click();

    await expect(page.getByTestId("apply-dialog")).toBeHidden();
    await expect(page.getByTestId("outside-change-hint")).toBeVisible();
    expect(await page.evaluate(() => window.__BASELINE_CHANGED_EMITS__ ?? 0)).toBeGreaterThan(0);
  });

  test("a file that already holds the order being applied is not an outside change to confirm", async ({
    page,
  }) => {
    await loadScenario(page);
    const suggested = await page.evaluate(async () => {
      const rows = (await window.__TAURI_INTERNALS__.invoke("list_order", {
        source: "suggested",
      })) as { modId: string }[];
      return rows.map((row) => row.modId);
    });
    await writeFileOutside(page, suggested);

    const kinds = await page.evaluate(async () => {
      const ask = async (source: string) =>
        (
          (await window.__TAURI_INTERNALS__.invoke("get_apply_preflight", { source })) as {
            outsideChange: { kind: string };
          }
        ).outsideChange.kind;
      return { suggested: await ask("suggested"), current: await ask("current") };
    });

    // Same file, two orders: it is an outside change for Current, but writing Suggested
    // overwrites nothing.
    expect(kinds.suggested).toBe("none");
    expect(kinds.current).not.toBe("none");
  });

  test("without an outside edit the confirm has no outside section", async ({ page }) => {
    await loadScenario(page);

    await page.getByTestId("dashboard-apply-button").click();
    await page.getByTestId("apply-dialog-submit").click();

    await expect(page.getByTestId("apply-confirm")).toBeVisible();
    await expect(page.getByTestId("apply-confirm-outside")).toHaveCount(0);
  });

  test("an outside reorder is worded on its own", async ({ page }) => {
    await loadScenario(page);
    const ids = await activeIds(page);
    await writeFileOutside(page, [...ids].reverse());

    await page.getByTestId("dashboard-apply-button").click();
    await page.getByTestId("apply-dialog-submit").click();

    await expect(page.getByTestId("apply-confirm-outside-text")).toContainText(
      "active order was changed elsewhere",
    );
  });

  test("the confirm works from the keyboard: Tab reaches Apply anyway, Enter on Go back backs out", async ({
    page,
  }) => {
    await loadScenario(page);
    const ids = await activeIds(page);
    await writeFileOutside(page, [...ids, "mod.905"]);
    await page.getByTestId("dashboard-apply-button").click();
    await page.getByTestId("apply-dialog-submit").click();
    await expect(page.getByTestId("apply-confirm-go-back")).toBeFocused();

    await page.keyboard.press("Tab");
    await expect(page.getByTestId("apply-confirm-apply-anyway")).toBeFocused();
    await page.keyboard.press("Shift+Tab");
    await page.keyboard.press("Enter");

    await expect(page.getByTestId("apply-confirm")).toBeHidden();
    await expect(page.getByTestId("apply-dialog-submit")).toBeVisible();
    expect(await page.evaluate(() => window.__APPLY_CALLS__ ?? [])).toEqual([]);
  });

  test("Apply anyway writes once and the next Apply asks nothing about the file", async ({
    page,
  }) => {
    await loadScenario(page);
    const ids = await activeIds(page);
    await writeFileOutside(page, [...ids, "mod.905"]);
    await page.getByTestId("dashboard-apply-button").click();
    await page.getByTestId("apply-dialog-submit").click();

    await page.getByTestId("apply-confirm-apply-anyway").click();

    await expect(page.getByTestId("apply-dialog")).toBeHidden();
    await expect(page.getByText(/ModsConfig\.xml\.bak-/)).toBeVisible();
    expect(await page.evaluate(() => window.__APPLY_CALLS__ ?? [])).toHaveLength(1);
    const outside = await page.evaluate(
      async () =>
        (
          (await window.__TAURI_INTERNALS__.invoke("get_apply_preflight", {
            source: "suggested",
          })) as { outsideChange: { kind: string } }
        ).outsideChange.kind,
    );
    expect(outside).toBe("none");
  });
});

test.describe("the outside-change hint", () => {
  test("shows off the Mods page, and goes away after a rescan", async ({ page }) => {
    await loadScenario(page);
    await expect(page.getByTestId("outside-change-hint")).toHaveCount(0);

    await writeFileThenFocus(page, [...(await activeIds(page)), "mod.905"]);

    await expect(page.getByTestId("outside-change-hint")).toBeVisible();
    await page.getByTestId("nav-rules").click();
    await expect(page.getByTestId("outside-change-hint")).toBeVisible();

    await page.getByTestId("outside-change-hint-link").click();
    await expect(page.getByTestId("file-changed-outside")).toBeVisible();
    await expect(page.getByTestId("outside-change-hint")).toHaveCount(0);

    await page.getByTestId("mods-rescan-button").click();
    await expect(page.getByTestId("file-changed-outside")).toHaveCount(0);
    await page.getByTestId("nav-dashboard").click();
    await expect(page.getByTestId("outside-change-hint")).toHaveCount(0);
  });

  test("goes away after an apply", async ({ page }) => {
    await loadScenario(page);
    await writeFileThenFocus(page, [...(await activeIds(page)), "mod.905"]);
    await expect(page.getByTestId("outside-change-hint")).toBeVisible();

    await page.getByTestId("dashboard-apply-button").click();
    await page.getByTestId("apply-dialog-submit").click();
    await page.getByTestId("apply-confirm-apply-anyway").click();

    await expect(page.getByTestId("apply-dialog")).toBeHidden();
    await expect(page.getByTestId("outside-change-hint")).toHaveCount(0);
  });

  test("says nothing about unsaved work: no pending banner for an outside edit alone", async ({
    page,
  }) => {
    await loadScenario(page);
    await writeFileThenFocus(page, [...(await activeIds(page)), "mod.905"]);

    await expect(page.getByTestId("outside-change-hint")).toBeVisible();
    await expect(page.getByTestId("pending-changes-banner")).toHaveCount(0);
  });
});
