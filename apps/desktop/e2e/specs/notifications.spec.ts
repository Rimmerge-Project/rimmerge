import { expect, test } from "@playwright/test";
// Type-only: pulls in `Window.__E2E_MOCK_IPC__`'s ambient declaration.
import type {} from "../../src/e2e-mock-bootstrap";
import { loadScenario } from "./support";

test.describe("notifications", () => {
  test("first run: 'Turn off internet access' dismisses Welcome and reaches no further port call", async ({
    page,
  }) => {
    await loadScenario(page);

    // `loadScenario`'s own `submit()` already fired
    // `run_launch_network_checks` once, gated to `awaitingFirstRun`
    // (the notice is still unanswered at that point) — the counter only
    // advances past the shared gate, so it stays at 0.
    const gatedCalls = await page.evaluate(() => window.__RUN_LAUNCH_NETWORK_CHECKS_CALLS__ ?? -1);
    expect(gatedCalls).toBe(0);

    await expect(page.getByTestId("welcome-card")).toBeVisible();
    await expect(page.getByTestId("notification-bell")).toHaveAttribute(
      "aria-label",
      "Notifications, 1",
    );

    await page.getByTestId("welcome-card-turn-off").click();

    await expect(page.getByTestId("welcome-card")).toBeHidden();
    await expect(page.getByTestId("notification-bell")).toHaveAttribute(
      "aria-label",
      "Notifications, none",
    );

    const completeCalls = await page.evaluate(() => window.__COMPLETE_WELCOME_CALLS__ ?? 0);
    expect(completeCalls).toBe(1);

    // The save must carry `allowNetwork: false` and run before
    // `complete_welcome`, and it must be the *only* `update_app_settings`
    // call — a regression here (saving a stale snapshot, or skipping the
    // save while settings are still loading) would leave the network
    // gate open with no visible sign in this test's other assertions.
    const settingsCalls = await page.evaluate(() => window.__UPDATE_APP_SETTINGS_CALLS__ ?? []);
    expect(settingsCalls).toHaveLength(1);
    expect(settingsCalls[0]?.network.allowNetwork).toBe(false);
    expect(settingsCalls[0]?.network.checkForUpdates).toBe(true);
    expect(settingsCalls[0]?.network.autoRefreshRuleDatabases).toBe(true);

    const gatedCallsAfter = await page.evaluate(
      () => window.__RUN_LAUNCH_NETWORK_CHECKS_CALLS__ ?? -1,
    );
    expect(gatedCallsAfter).toBe(0);
  });

  test("first run: flipping two switches in a row saves each on top of the live app settings", async ({
    page,
  }) => {
    await loadScenario(page);

    await page.getByTestId("welcome-card-check-for-updates").click();
    await page.getByTestId("welcome-card-auto-refresh").click();

    const settingsCalls = await page.evaluate(() => window.__UPDATE_APP_SETTINGS_CALLS__ ?? []);
    expect(settingsCalls).toHaveLength(2);
    // The second save must still carry the first save's own
    // `checkForUpdates: false` — a stale-snapshot regression would
    // instead silently flip it back to `true` here.
    expect(settingsCalls[0]?.network.checkForUpdates).toBe(false);
    expect(settingsCalls[0]?.network.autoRefreshRuleDatabases).toBe(true);
    expect(settingsCalls[1]?.network.checkForUpdates).toBe(false);
    expect(settingsCalls[1]?.network.autoRefreshRuleDatabases).toBe(false);
    expect(settingsCalls[1]?.reminders).toEqual(settingsCalls[0]?.reminders);
  });

  test("first run: 'Keep these settings' dismisses Welcome without touching network settings", async ({
    page,
  }) => {
    await loadScenario(page);

    await page.getByTestId("welcome-card-keep").click();

    await expect(page.getByTestId("welcome-card")).toBeHidden();
    const completeCalls = await page.evaluate(() => window.__COMPLETE_WELCOME_CALLS__ ?? 0);
    expect(completeCalls).toBe(1);
  });

  test("bell: dismissing a notice removes it and it stays dismissed after navigating away and back", async ({
    page,
  }) => {
    await loadScenario(page);

    await page.getByTestId("notification-bell").click();
    await expect(page.getByTestId("notification-popover")).toBeVisible();
    await page.getByTestId("notification-dismiss-welcome").click();

    await expect(page.getByTestId("notification-bell")).toHaveAttribute(
      "aria-label",
      "Notifications, none",
    );

    await page.getByTestId("nav-settings").click();
    await page.getByTestId("nav-dashboard").click();

    await expect(page.getByTestId("notification-bell")).toHaveAttribute(
      "aria-label",
      "Notifications, none",
    );
    await expect(page.getByTestId("welcome-card")).toBeHidden();

    const calls = await page.evaluate(() => window.__DISMISS_NOTIFICATION_CALLS__ ?? []);
    expect(calls).toEqual([{ kind: "welcome", fingerprint: "welcome" }]);
  });

  test("Check now: up to date, a new version, rate limited, and a failure", async ({ page }) => {
    await loadScenario(page);
    await page.getByTestId("nav-settings").click();

    await page.getByTestId("settings-check-for-update-now").click();
    await expect(page.getByTestId("settings-check-for-update-result")).toContainText("Up to date");

    await page.evaluate(() => {
      window.__CHECK_FOR_UPDATE_RESULT__ = {
        kind: "ran",
        outcome: { kind: "updated", latestVersion: "0.2.0" },
      };
    });
    await page.getByTestId("settings-check-for-update-now").click();
    await expect(page.getByTestId("settings-check-for-update-result")).toContainText("0.2.0");

    await page.evaluate(() => {
      window.__CHECK_FOR_UPDATE_RESULT__ = {
        kind: "skipped",
        reason: { rateLimitedUntil: { until: "2026-09-25T14:05:00Z" } },
      };
    });
    await page.getByTestId("settings-check-for-update-now").click();
    await expect(page.getByTestId("settings-check-for-update-result")).toContainText("rate limit");

    await page.evaluate(() => {
      window.__CHECK_FOR_UPDATE_RESULT__ = {
        kind: "ran",
        outcome: {
          kind: "failed",
          failure: { cause: { kind: "transport" }, detail: "connection timed out" },
        },
      };
    });
    await page.getByTestId("settings-check-for-update-now").click();
    await expect(page.getByTestId("settings-check-for-update-result")).toContainText(
      "Couldn't reach GitHub",
    );
    await expect(page.getByTestId("settings-check-for-update-result")).not.toContainText(
      "connection timed out",
    );
    await expect(page.getByTestId("settings-check-for-update-result-detail")).toHaveText(
      "Technical details: connection timed out",
    );

    const calls = await page.evaluate(() => window.__CHECK_FOR_UPDATE_CALLS__ ?? 0);
    expect(calls).toBe(4);
  });

  test("Check now is disabled while internet access is off", async ({ page }) => {
    await loadScenario(page);
    await page.getByTestId("nav-settings").click();
    await page.getByTestId("settings-allow-network").click();

    await expect(page.getByTestId("settings-check-for-update-now")).toBeDisabled();
  });

  test("mute from the bell removes the notice, Settings lists it, and Unmute reverses it", async ({
    page,
  }) => {
    await loadScenario(page);
    // Answer the first-run notice first, so only the imported-rules
    // notice this test drives is left to reason about.
    await page.getByTestId("welcome-card-keep").click();

    // A successful refresh leaves `community`'s cache ahead of what this
    // profile last imported — `ImportedRulesOutdated` fires.
    await page.getByTestId("nav-rules").click();
    await page.getByTestId("rule-databases-refresh").click();
    await expect(page.getByTestId("rule-database-needs-reimport-community")).toBeVisible();

    await page.getByTestId("notification-bell").click();
    await expect(page.getByTestId("notification-item-importedRulesOutdated")).toBeVisible();
    await page.getByTestId("notification-mute-importedRulesOutdated").click();
    await expect(page.getByTestId("notification-item-importedRulesOutdated")).toBeHidden();

    await page.getByTestId("nav-settings").click();
    await expect(page.getByTestId("settings-muted-kind-importedRulesOutdated")).toBeVisible();

    await page.getByTestId("settings-unmute-importedRulesOutdated").click();
    await expect(page.getByTestId("settings-muted-kinds-empty")).toBeVisible();

    await page.getByTestId("notification-bell").click();
    await expect(page.getByTestId("notification-item-importedRulesOutdated")).toBeVisible();
  });

  test("first run: the Steam Workshop switch saves on its own and answers nothing", async ({
    page,
  }) => {
    await loadScenario(page);

    const steamSwitch = page.getByTestId("welcome-card-steam-workshop").locator("input");
    await expect(steamSwitch).toBeChecked();
    await steamSwitch.uncheck({ force: true });
    await expect(steamSwitch).not.toBeChecked();

    const settingsCalls = await page.evaluate(() => window.__UPDATE_APP_SETTINGS_CALLS__ ?? []);
    expect(settingsCalls).toHaveLength(1);
    expect(settingsCalls[0]?.network.fetchSteamWorkshop).toBe(false);
    expect(settingsCalls[0]?.network.allowNetwork).toBe(true);
    expect(await page.evaluate(() => window.__COMPLETE_WELCOME_CALLS__ ?? 0)).toBe(0);
    await expect(page.getByTestId("welcome-card")).toBeVisible();
  });

  test("recommended sources: not shown before Welcome is answered, nor while internet access is off", async ({
    page,
  }) => {
    await loadScenario(page);
    // Steam is on by default and never downloaded, yet only Welcome is listed.
    await page.getByTestId("notification-bell").click();
    await expect(page.getByTestId("notification-item-welcome")).toBeVisible();
    await expect(page.getByTestId("notification-item-recommendedSourcesIncomplete")).toBeHidden();

    await page.getByTestId("welcome-card-turn-off").click();
    await expect(page.getByTestId("welcome-card")).toBeHidden();
    await expect(page.getByTestId("notification-bell")).toHaveAttribute(
      "aria-label",
      "Notifications, none",
    );
  });

  test("recommended sources: 'Turn on and download' enables the sources, downloads them, and the notice goes away", async ({
    page,
  }) => {
    await loadScenario(page);
    const steamSwitch = page.getByTestId("welcome-card-steam-workshop").locator("input");
    await steamSwitch.uncheck({ force: true });
    await expect(steamSwitch).not.toBeChecked();
    await page.getByTestId("welcome-card-keep").click();

    await page.getByTestId("notification-bell").click();
    const item = page.getByTestId("notification-item-recommendedSourcesIncomplete");
    await expect(item).toBeVisible();
    await expect(page.getByTestId("notification-sources-recommendedSourcesIncomplete")).toHaveText(
      /Steam Workshop: off/,
    );

    await page
      .getByTestId("notification-action-recommendedSourcesIncomplete-enableRecommendedSources")
      .click();

    await expect(item).toBeHidden();
    expect(await page.evaluate(() => window.__ENABLE_RECOMMENDED_CALLS__ ?? 0)).toBe(1);
    // Internet access itself was never touched by the action.
    const settingsCalls = await page.evaluate(() => window.__UPDATE_APP_SETTINGS_CALLS__ ?? []);
    expect(settingsCalls.every((call) => call.network.allowNetwork)).toBe(true);

    await page.getByTestId("nav-rules").click();
    await expect(page.getByTestId("rule-database-status-steam")).not.toContainText("Never fetched");
  });

  test("recommended sources: a source that is on but never downloaded is listed as such, and dismissing it sticks", async ({
    page,
  }) => {
    await loadScenario(page);
    await page.getByTestId("welcome-card-keep").click();

    await page.getByTestId("notification-bell").click();
    await expect(page.getByTestId("notification-sources-recommendedSourcesIncomplete")).toHaveText(
      /Steam Workshop: on, not downloaded yet/,
    );
    await page.getByTestId("notification-dismiss-recommendedSourcesIncomplete").click();
    await expect(page.getByTestId("notification-item-recommendedSourcesIncomplete")).toBeHidden();

    await page.getByTestId("nav-settings").click();
    await page.getByTestId("nav-dashboard").click();
    await page.getByTestId("notification-bell").click();
    await expect(page.getByTestId("notification-item-recommendedSourcesIncomplete")).toBeHidden();

    const calls = await page.evaluate(() => window.__DISMISS_NOTIFICATION_CALLS__ ?? []);
    expect(calls).toEqual([
      {
        kind: "recommendedSourcesIncomplete",
        fingerprint: "steam_workshop@not_downloaded",
      },
    ]);
  });

  test("stale rule databases: 'Refresh now' refreshes exactly the sources the notice lists, the card refreshes all", async ({
    page,
  }) => {
    await loadScenario(page);
    await page.evaluate(() => {
      window.__STALE_RULE_DATABASES__ = ["community", "rimmerge"];
    });
    await page.getByTestId("welcome-card-keep").click();

    await page.getByTestId("notification-bell").click();
    await expect(page.getByTestId("notification-item-ruleDatabasesStale")).toBeVisible();
    await page.getByTestId("notification-action-ruleDatabasesStale-refreshRuleDatabases").click();

    await expect
      .poll(() => page.evaluate(() => window.__REFRESH_RULE_DATABASES_CALLS__ ?? []))
      .toEqual([{ sources: ["community", "rimmerge"] }]);

    // The Steam Workshop database (about 49 MB) was not on the notice, so
    // the click did not download it.
    await page.getByTestId("nav-rules").click();
    await expect(page.getByTestId("rule-database-status-steam")).toContainText("Never fetched");

    await page.getByTestId("rule-databases-refresh").click();
    await expect
      .poll(() => page.evaluate(() => window.__REFRESH_RULE_DATABASES_CALLS__ ?? []))
      .toEqual([{ sources: ["community", "rimmerge"] }, null]);
    await expect(page.getByTestId("rule-database-status-steam")).not.toContainText("Never fetched");
  });
});
