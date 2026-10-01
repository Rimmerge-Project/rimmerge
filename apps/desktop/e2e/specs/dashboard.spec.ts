import { expect, test } from "@playwright/test";
// Type-only: pulls in `Window.__E2E_MOCK_IPC__`'s ambient declaration
// (this file's own program never imports the module at runtime — that
// would try to read `window` under Node).
import type {} from "../../src/e2e-mock-bootstrap";
import { dashboardFixture, dashboardScenario, defaultPathsFixture } from "../fixtures/dashboard";
import { installScenario } from "../fixtures/scenario";
import { loadScenario } from "./support";

test.describe("setup page path config", () => {
  test("saves the three install paths as defaults, never the derived profile directory", async ({
    page,
  }) => {
    // `installScenario`, not `dashboardScenario`: the payload tracker is
    // a real mock *function*, and a fixture object handed to
    // `addInitScript` as an argument crosses the Node/browser boundary as
    // JSON, which drops functions.
    await page.addInitScript(installScenario);
    await page.goto("/setup");

    await page.getByTestId("game-dir-input").fill("Q:/Steam/steamapps/common/RimWorld");
    await page.getByTestId("save-defaults-button").click();

    await expect(page.getByText("Saved as defaults")).toBeVisible();
    // The exact payload, not merely that something was sent:
    // `profileDir` is derived from the `ModsConfig.xml` path, so pinning
    // it here is how two installs end up sharing one profile's decisions.
    const calls = await page.evaluate(() => window.__SAVE_APP_CONFIG_CALLS__ ?? []);
    expect(calls).toEqual([
      {
        gameDir: "Q:/Steam/steamapps/common/RimWorld",
        workshopDir: "C:/RimWorld/workshop",
        modsConfig: "C:/Profile/ModsConfig.xml",
      },
    ]);
  });

  test("shows the no-install-found error with every candidate location", async ({ page }) => {
    await page.addInitScript((fixtures) => {
      window.__E2E_MOCK_IPC__ = {
        ...fixtures,
        get_default_paths: {
          gameDir: null,
          workshopDir: null,
          modsConfig: null,
          profileDir: null,
          warning: null,
          error: [
            "no RimWorld install found. Pass --game-dir <path>, set RIMMERGE_GAME_DIR, or",
            "run `rimmerge config set --game-dir <path>`. Looked in:",
            "  Q:/Steam/steamapps/common/RimWorld",
            "  W:/SteamGames/steamapps/common/RimWorld",
          ].join("\n"),
        },
      };
    }, dashboardScenario);
    await page.goto("/setup");

    const banner = page.getByTestId("setup-detection-error");
    await expect(banner).toContainText("no RimWorld install found");
    await expect(banner).toContainText("Q:/Steam/steamapps/common/RimWorld");
    await expect(banner).toContainText("W:/SteamGames/steamapps/common/RimWorld");
    await expect(page.getByTestId("game-dir-input")).toHaveValue("");
    await expect(page.getByTestId("load-project-button")).toBeDisabled();
  });
});

test.describe("setup and dashboard", () => {
  test("loads a project through the mocked IPC and shows the dashboard counts", async ({
    page,
  }) => {
    // Runs before any of the page's own scripts, so `src/e2e-mock-bootstrap.ts`
    // sees `window.__E2E_MOCK_IPC__` already set when it evaluates.
    await page.addInitScript((fixtures) => {
      window.__E2E_MOCK_IPC__ = fixtures;
      // The mock-IPC tier never loads a real Tauri webview, so nothing
      // ever calls `window.__TAURI_INTERNALS__` on its own — `mockIPC`
      // (installed by the bootstrap module) is what defines it.
    }, dashboardScenario);

    await page.goto("/");

    // No project loaded yet: the router guard sends us to /setup, and the
    // folder inputs prefill from the mocked `get_default_paths`.
    await expect(page).toHaveURL(/\/setup$/);
    await expect(page.getByTestId("game-dir-input")).toHaveValue(defaultPathsFixture.gameDir ?? "");
    await expect(page.getByTestId("mods-config-input")).toHaveValue(
      defaultPathsFixture.modsConfig ?? "",
    );

    await expect(page.getByTestId("load-project-button")).toBeEnabled();
    await page.getByTestId("load-project-button").click();

    await expect(page).toHaveURL(/\/$/);
    await expect(page.getByTestId("mod-count")).toHaveText(String(dashboardFixture.modCount));
    await expect(page.getByTestId("needs-input-current")).toHaveText(
      String(dashboardFixture.ledgerStats.current.needsInput),
    );
    await expect(page.getByTestId("moved-mods")).toHaveText(String(dashboardFixture.movedMods));
    // Rendered through its translated label (`orderSourceLabel`), not the
    // raw `OrderSourceDto` wire value `dashboardFixture.selected` carries.
    await expect(page.getByTestId("selected-order")).toHaveText("Current");
  });

  test("shows the needs-input count as information with an Inbox link, and never gates Apply on it", async ({
    page,
  }) => {
    await loadScenario(page);

    await expect(page.getByTestId("needs-input-current")).not.toHaveText("0");
    await expect(page.getByTestId("guide-needs-input")).toBeVisible();
    await expect(page.getByTestId("dashboard-apply-button")).toBeEnabled();
    await page.getByTestId("guide-inbox-link").click();
    await expect(page).toHaveURL(/\/inbox/);
  });

  test("shows the top needs-input kinds ranked by count, largest first", async ({ page }) => {
    await loadScenario(page);

    // Reads the scenario's own `get_dashboard` fixture directly (called
    // in-page, inside `page.evaluate`'s callback — not serialized across
    // the Node/browser boundary) rather than hand-copying its counts here,
    // so this stays correct if the scenario's finding mix ever changes.
    const needsInputByKind = await page.evaluate(async () => {
      const getDashboard = window.__E2E_MOCK_IPC__?.["get_dashboard"];
      if (typeof getDashboard !== "function") {
        throw new Error("get_dashboard fixture missing or not a function");
      }
      const dashboard = (await getDashboard(undefined)) as {
        needsInputByKind: Record<string, number>;
      };
      return dashboard.needsInputByKind;
    });

    // Compared by 'data-testid' ('needs-input-kind-' + the raw enum kind),
    // not the rendered label text — that keeps this independent of
    // 'findingKindLabel''s wording, and of 'src/utils/finding.ts''s own
    // '@/*' imports, which this project's 'tsconfig.json' doesn't resolve.
    const expectedKinds = Object.entries(needsInputByKind)
      .filter(([, count]) => count > 0)
      .sort((a, b) => b[1] - a[1])
      .slice(0, 5)
      .map(([kind]) => kind);

    const items = page.locator('[data-testid="top-needs-input-kinds"] li');
    await expect(items).toHaveCount(expectedKinds.length);
    for (const [index, kind] of expectedKinds.entries()) {
      await expect(items.nth(index)).toHaveAttribute("data-testid", `needs-input-kind-${kind}`);
    }
  });

  test("shows one 'Scan finished with N notes' toast and a matching Scan notes card, not one toast per note", async ({
    page,
  }) => {
    await page.addInitScript(installScenario);
    // Overrides just `load_project`'s response, after `installScenario`'s
    // own `addInitScript` has already set `window.__E2E_MOCK_IPC__` — both
    // run, in order, before `src/e2e-mock-bootstrap.ts` ever reads it.
    await page.addInitScript(() => {
      if (!window.__E2E_MOCK_IPC__) {
        throw new Error("installScenario must run before this init script");
      }
      window.__E2E_MOCK_IPC__["load_project"] = {
        modCount: 993,
        gameVersion: "1.6.4871",
        elapsedMs: 900,
        warnings: [
          { modId: "mod.one", message: "Languages/English/Keyed/keys.xml: skipped: bad xml" },
          { modId: "mod.two", message: "Defs/Buildings.xml: skipped: bad xml" },
          { modId: null, message: "some/unattributed/path: skipped: bad xml" },
        ],
        ruleWarnings: [],
        selected: "suggested",
      };
    });

    await page.goto("/");
    await page.getByTestId("load-project-button").click();
    await page.waitForURL(/\/$/);

    await expect(page.getByText("Scan finished with 3 notes")).toBeVisible();
    await expect(page.locator('[role="alert"]')).toHaveCount(1);

    const disclosure = page.getByTestId("scan-notes-disclosure");
    await expect(disclosure).toContainText("Scan notes (3)");
    await disclosure.locator("summary").click();
    await expect(page.locator('[data-testid="scan-note"]')).toHaveCount(3);
  });
});
