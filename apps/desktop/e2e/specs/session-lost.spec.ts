import { expect, test } from "@playwright/test";
// Type-only: pulls in `Window.__E2E_MOCK_IPC__`'s ambient declaration.
import type {} from "../../src/e2e-mock-bootstrap";
import { dashboardFixture } from "../fixtures/dashboard";

test.describe("session lost", () => {
  test("a session_lost response transparently reloads the project and lands back on the dashboard", async ({
    page,
  }) => {
    // `get_dashboard` fails with `session_lost` exactly once — the shape
    // of a caught backend panic (`with_session`'s own `catch_unwind`
    // fix) — then succeeds, so this pins the real, full loop: the global
    // handler (`main.ts`) sends the app to `/setup?autoReload=1` with the
    // paths it already had, `SetupPage.vue` auto-submits from them (never
    // from `get_default_paths`'s own, different-looking values), and the
    // app lands back on the dashboard with no manual intervention — a
    // visible reload through the ordinary setup flow, never a hidden one.
    // `dashboardFixture` is passed in as `addInitScript`'s own serialized
    // argument (plain data, no functions) so the closure below can still
    // hold real per-page state (`dashboardCalls`) — functions themselves
    // can't cross that boundary, only JSON-serializable values can.
    await page.addInitScript((fixture) => {
      let dashboardCalls = 0;
      window.__E2E_MOCK_IPC__ = {
        get_default_paths: {
          // Deliberately different from the `.fill()`-set value below:
          // `load_project` rejects anything else, so an auto-reload that
          // wrongly fell back to these defaults instead of the session's
          // own remembered paths fails loudly (stuck on `/setup`, the
          // final assertion times out) rather than silently passing.
          gameDir: "C:/DefaultGame",
          workshopDir: "C:/DefaultGame/workshop",
          modsConfig: "C:/DefaultProfile/ModsConfig.xml",
          profileDir: "C:/DefaultProfile/rimmerge",
          warning: null,
          error: null,
        },
        load_project: (payload: unknown) => {
          const { paths } = payload as { paths: { gameDir: string } };
          if (paths.gameDir !== "C:/PreviousGame") {
            throw { code: "scan_failed", message: `unexpected gameDir: ${paths.gameDir}` };
          }
          return {
            modCount: 1,
            gameVersion: "1.6.4871",
            elapsedMs: 10,
            warnings: [],
            ruleWarnings: [],
            selected: "suggested",
          };
        },
        get_dashboard: () => {
          dashboardCalls += 1;
          if (dashboardCalls === 1) {
            throw {
              code: "session_lost",
              message: "the session was lost after an earlier internal error; reload the project",
            };
          }
          return fixture;
        },
        // `SetupPage.vue`'s `submit()` fires this fire-and-forget right
        // after a successful `load_project` — registered so it doesn't
        // throw an unhandled rejection this spec never awaits.
        run_launch_network_checks: {
          updateCheck: { kind: "skipped", reason: "awaitingFirstRun" },
          databaseRefresh: [],
        },
      };
    }, dashboardFixture);

    await page.goto("/");
    await expect(page).toHaveURL(/\/setup$/);
    // Must match the literal `load_project` checks against inside the
    // `addInitScript` closure above (a plain string, not a shared
    // constant — that closure is serialized on its own, with no access
    // to this scope).
    await page.getByTestId("game-dir-input").fill("C:/PreviousGame");
    await page.getByTestId("load-project-button").click();

    // First load succeeds, `get_dashboard` fails, the app bounces through
    // `/setup` on its own and back to the dashboard — no click required.
    await expect(page).toHaveURL(/\/$/, { timeout: 10_000 });
    await expect(page.getByTestId("mod-count")).toHaveText(String(dashboardFixture.modCount));
    await expect(page.getByTestId("dashboard-error")).toHaveCount(0);
  });

  test("a no_project_loaded response also sends the app back to setup", async ({ page }) => {
    await page.addInitScript(() => {
      window.__E2E_MOCK_IPC__ = {
        get_default_paths: {
          gameDir: "C:/RimWorld",
          workshopDir: "C:/RimWorld/workshop",
          modsConfig: "C:/Profile/ModsConfig.xml",
          profileDir: "C:/Profile/rimmerge",
          warning: null,
          error: null,
        },
        load_project: {
          modCount: 1,
          gameVersion: "1.6.4871",
          elapsedMs: 10,
          warnings: [],
          ruleWarnings: [],
          selected: "suggested",
        },
        get_dashboard: () => {
          throw { code: "no_project_loaded", message: "no project is loaded" };
        },
        run_launch_network_checks: {
          updateCheck: { kind: "skipped", reason: "awaitingFirstRun" },
          databaseRefresh: [],
        },
      };
    });

    await page.goto("/");
    await page.getByTestId("load-project-button").click();

    await expect(page).toHaveURL(/\/setup$/);
  });
});
