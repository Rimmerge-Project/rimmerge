import { expect, test } from "@playwright/test";
// Type-only: pulls in `Window.__E2E_MOCK_IPC__`'s ambient declaration.
import type {} from "../../src/e2e-mock-bootstrap";
import { loadScenario } from "./support";

// Contract checks for the scenario's own `get_apply_preflight` /
// `fileMatchesSuggested` mock: no UI consumes them yet, so this guards
// that the mock keeps mirroring `rim_resolve::preflight` and
// `Session::file_matches` until the dialog and strip land.
type Ipc = Record<string, (payload?: unknown) => Promise<unknown>>;

test.describe("apply preflight mock", () => {
  test("a missing mod is kept in the current order and removed from the suggested one", async ({
    page,
  }) => {
    await loadScenario(page);

    const { current, suggested } = await page.evaluate(async () => {
      const ipc = window.__E2E_MOCK_IPC__ as unknown as Ipc;
      return {
        current: (await ipc.get_apply_preflight?.({ source: "current" })) as {
          items: { problem: { kind: string; outcome?: string } }[];
          requiresConfirmation: boolean;
        },
        suggested: (await ipc.get_apply_preflight?.({ source: "suggested" })) as {
          items: { problem: { kind: string; outcome?: string } }[];
          requiresConfirmation: boolean;
        },
      };
    });

    const outcomes = (items: typeof current.items) =>
      items.filter((item) => item.problem.kind === "missingMod").map((i) => i.problem.outcome);
    expect(outcomes(current.items).length).toBeGreaterThan(0);
    expect(new Set(outcomes(current.items))).toEqual(new Set(["keptInActiveList"]));
    expect(new Set(outcomes(suggested.items))).toEqual(new Set(["removedFromActiveList"]));
    expect(current.requiresConfirmation).toBe(true);
    expect(suggested.requiresConfirmation).toBe(true);
  });

  test("the file matches the suggested order only after applying it", async ({ page }) => {
    await loadScenario(page);
    const matches = () =>
      page.evaluate(async () => {
        const ipc = window.__E2E_MOCK_IPC__ as unknown as Ipc;
        const dashboard = (await ipc.get_dashboard?.()) as { fileMatchesSuggested: boolean };
        return dashboard.fileMatchesSuggested;
      });
    const applySource = (source: "current" | "suggested") =>
      page.evaluate(async (chosen) => {
        const ipc = window.__E2E_MOCK_IPC__ as unknown as Ipc;
        await ipc.apply?.({
          request: { source: chosen, writeModsConfig: true, writeMergeMod: false, force: false },
        });
      }, source);

    expect(await matches()).toBe(false);
    await applySource("suggested");
    expect(await matches()).toBe(true);
    await applySource("current");
    expect(await matches()).toBe(false);
  });
});
