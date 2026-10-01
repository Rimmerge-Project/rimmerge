import { expect, test } from "@playwright/test";
// Type-only: pulls in `Window.__E2E_MOCK_IPC__`'s ambient declaration.
import type {} from "../../src/e2e-mock-bootstrap";
import { loadScenario } from "./support";

test.describe("settings", () => {
  test("toggling enforcement and saving persists and re-fetches on reload", async ({ page }) => {
    await loadScenario(page);
    await page.getByTestId("nav-settings").click();
    await expect(page).toHaveURL(/\/settings$/);

    const softToggle = page.getByTestId("settings-enforce-soft");
    await expect(softToggle).not.toBeChecked();

    await softToggle.click();
    await page.getByTestId("settings-save").click();

    // The save mutation invalidates every query, which triggers a
    // `get_settings` re-fetch — reloading the page proves the toggle's
    // new value round-tripped through the mock session, not just local
    // component state.
    await page.getByTestId("nav-dashboard").click();
    await page.getByTestId("nav-settings").click();

    await expect(page.getByTestId("settings-enforce-soft")).toBeChecked();

    // Confirms the saved payload is exactly the scenario's default
    // settings with only the toggled field changed — not just that some
    // `set_settings` call happened.
    const calls = await page.evaluate(() => window.__SET_SETTINGS_CALLS__ ?? []);
    expect(calls).toEqual([
      {
        threshold: 80,
        enforceSoft: true,
        enforceAwareness: false,
        suggestMergeWhenClean: true,
        tieBreak: "rebuild",
        useImportedPairs: false,
        useImportedPlacements: true,
        enforceInferred: true,
        showDanglingDefReferences: false,
      },
    ]);
  });

  test("toggling enforce-inferred and saving persists and re-fetches on reload", async ({
    page,
  }) => {
    await loadScenario(page);
    await page.getByTestId("nav-settings").click();
    await expect(page).toHaveURL(/\/settings$/);

    const inferredToggle = page.getByTestId("settings-enforce-inferred");
    await expect(inferredToggle).toBeChecked();

    await inferredToggle.click();
    await page.getByTestId("settings-save").click();

    await page.getByTestId("nav-dashboard").click();
    await page.getByTestId("nav-settings").click();

    await expect(page.getByTestId("settings-enforce-inferred")).not.toBeChecked();

    const calls = await page.evaluate(() => window.__SET_SETTINGS_CALLS__ ?? []);
    expect(calls).toEqual([
      {
        threshold: 80,
        enforceSoft: false,
        enforceAwareness: false,
        suggestMergeWhenClean: true,
        tieBreak: "rebuild",
        useImportedPairs: false,
        useImportedPlacements: true,
        enforceInferred: false,
        showDanglingDefReferences: false,
      },
    ]);
  });

  test("toggling show-dangling-def-references and saving persists and re-fetches on reload", async ({
    page,
  }) => {
    await loadScenario(page);
    await page.getByTestId("nav-settings").click();
    await expect(page).toHaveURL(/\/settings$/);

    const danglingToggle = page.getByTestId("settings-show-dangling-def-references");
    await expect(danglingToggle).not.toBeChecked();

    await danglingToggle.click();
    await page.getByTestId("settings-save").click();

    await page.getByTestId("nav-dashboard").click();
    await page.getByTestId("nav-settings").click();

    await expect(page.getByTestId("settings-show-dangling-def-references")).toBeChecked();

    const calls = await page.evaluate(() => window.__SET_SETTINGS_CALLS__ ?? []);
    expect(calls).toEqual([
      {
        threshold: 80,
        enforceSoft: false,
        enforceAwareness: false,
        suggestMergeWhenClean: true,
        tieBreak: "rebuild",
        useImportedPairs: false,
        useImportedPlacements: true,
        enforceInferred: true,
        showDanglingDefReferences: true,
      },
    ]);
  });

  test("switching the tie-break and toggling imported rules persists and re-fetches on reload", async ({
    page,
  }) => {
    await loadScenario(page);
    await page.getByTestId("nav-settings").click();

    await expect(page.getByTestId("settings-tie-break-rebuild")).toHaveAttribute(
      "aria-checked",
      "true",
    );
    await page.getByTestId("settings-tie-break-preserve-current").click();
    await page.getByTestId("settings-use-imported-pairs").click();
    await page.getByTestId("settings-save").click();

    await page.getByTestId("nav-dashboard").click();
    await page.getByTestId("nav-settings").click();

    await expect(page.getByTestId("settings-tie-break-preserve-current")).toHaveAttribute(
      "aria-checked",
      "true",
    );
    await expect(page.getByTestId("settings-use-imported-pairs")).toBeChecked();

    const calls = await page.evaluate(() => window.__SET_SETTINGS_CALLS__ ?? []);
    expect(calls).toEqual([
      {
        threshold: 80,
        enforceSoft: false,
        enforceAwareness: false,
        suggestMergeWhenClean: true,
        tieBreak: "preserveCurrent",
        useImportedPairs: true,
        useImportedPlacements: true,
        enforceInferred: true,
        showDanglingDefReferences: false,
      },
    ]);

    // The dashboard's "mods that move" tile states which settings
    // produced the suggested order, so the switch above is legible there
    // too, not just on the settings page.
    await page.getByTestId("nav-dashboard").click();
    await expect(page.getByTestId("moved-mods-provenance")).toContainText("preserve current");
    await expect(page.getByTestId("moved-mods-provenance")).toContainText("imported pairs: on");
  });

  test("'Reset to defaults' confirms, refills the form from the backend's own defaults, and doesn't save until Save is pressed", async ({
    page,
  }) => {
    await loadScenario(page);
    await page.getByTestId("nav-settings").click();
    await expect(page).toHaveURL(/\/settings$/);

    // The scenario's own settings start with imported pairs off — the
    // real backend's default is on, so the reset is observable.
    await expect(page.getByTestId("settings-use-imported-pairs")).not.toBeChecked();

    await page.getByTestId("settings-reset-defaults-button").click();
    await expect(page.getByTestId("settings-reset-confirm-dialog")).toBeVisible();

    // Cancelling leaves the form untouched.
    await page.getByTestId("settings-reset-confirm-cancel").click();
    await expect(page.getByTestId("settings-reset-confirm-dialog")).toBeHidden();
    await expect(page.getByTestId("settings-use-imported-pairs")).not.toBeChecked();

    await page.getByTestId("settings-reset-defaults-button").click();
    await page.getByTestId("settings-reset-confirm-submit").click();
    await expect(page.getByTestId("settings-reset-confirm-dialog")).toBeHidden();

    // Refilled, and flagged unsaved — but not yet persisted.
    await expect(page.getByTestId("settings-use-imported-pairs")).toBeChecked();
    await expect(page.getByTestId("settings-dirty-indicator")).toBeVisible();
    let calls = await page.evaluate(() => window.__SET_SETTINGS_CALLS__ ?? []);
    expect(calls).toEqual([]);

    await page.getByTestId("settings-save").click();
    await expect(page.getByTestId("settings-dirty-indicator")).toBeHidden();

    calls = await page.evaluate(() => window.__SET_SETTINGS_CALLS__ ?? []);
    expect(calls).toEqual([
      {
        threshold: 80,
        enforceSoft: false,
        enforceAwareness: false,
        suggestMergeWhenClean: true,
        tieBreak: "rebuild",
        useImportedPairs: true,
        useImportedPlacements: true,
        enforceInferred: true,
        showDanglingDefReferences: false,
      },
    ]);
  });

  test("the About section's version and links, and the sidebar's support link, each open through the backend opener", async ({
    page,
  }) => {
    await loadScenario(page);
    await page.getByTestId("nav-settings").click();
    await expect(page).toHaveURL(/\/settings$/);

    await expect(page.getByTestId("settings-about-version")).toContainText("1.0.0");

    await page.getByTestId("settings-about-repo-link").click();
    await page.getByTestId("settings-about-issues-link").click();
    await page.getByTestId("settings-about-support-link").click();
    // The sidebar's own support link is visible from every page, not
    // just Settings — clicked from here to prove that too.
    await page.getByTestId("shell-support-link").click();

    const calls = await page.evaluate(() => window.__OPEN_APP_LINK_CALLS__ ?? []);
    expect(calls).toEqual(["githubRepo", "githubIssues", "support", "support"]);
  });
});
