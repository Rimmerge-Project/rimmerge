import { expect, type Page, test } from "@playwright/test";
// Type-only: pulls in `Window.__E2E_MOCK_IPC__`'s ambient declaration.
import type {} from "../../src/e2e-mock-bootstrap";
import { loadScenario } from "./support";

/**
 * The mock's `parseUserPath` must agree with `rim_session::user_path::UserPath::parse` on every
 * spelling, so a spec that passes against the mock cannot pass a path the backend refuses. This
 * table mirrors that parser's own tests (`crates/rim-session/src/user_path.rs`) case for case; the
 * `std::path` readings it relies on (`\\?\C:/x` is a verbatim drive, `//?/C:/x` a UNC path with
 * the server `?`) were checked against `std` on Windows. It also pins that every path-taking
 * command runs the parser before it does anything else.
 */

type Outcome = "ok" | "network_path_refused" | "invalid_input";

/** `[text, outcome]`, written with `String.raw` so a backslash is one character. */
const CASES: ReadonlyArray<readonly [string, Outcome]> = [
  // Drive-letter paths.
  [String.raw`C:\x`, "ok"],
  [String.raw`c:\data\me\x`, "ok"],
  [String.raw`Z:\mapped\x`, "ok"],
  ["C:/forward/slash", "ok"],
  ["  C:\\x \t", "ok"],
  [String.raw`\\?\C:\x`, "ok"],
  [String.raw`\\?\C:/x`, "ok"],
  // Network paths, however they are spelled.
  [String.raw`\\server\share\x`, "network_path_refused"],
  ["//server/share/x", "network_path_refused"],
  [String.raw`\\?\UNC\server\share\x`, "network_path_refused"],
  [String.raw`\\?\unc\server\share\x`, "network_path_refused"],
  [String.raw`\\?\Unc\server\share\x`, "network_path_refused"],
  [String.raw`\\server@SSL\x`, "network_path_refused"],
  [String.raw`\\server@SSL\DavWWWRoot\x`, "network_path_refused"],
  ["//?/C:/x", "network_path_refused"],
  [String.raw`\\?/C:\x`, "network_path_refused"],
  // Device paths and the bare volume.
  [String.raw`\\.\pipe\x`, "invalid_input"],
  ["//./pipe/x", "invalid_input"],
  [String.raw`\\.\C:`, "invalid_input"],
  [String.raw`\\?\GLOBALROOT\x`, "invalid_input"],
  [String.raw`\\?\pipe\x`, "invalid_input"],
  [String.raw`\\?\CONIN$`, "invalid_input"],
  [String.raw`\\?\C:`, "invalid_input"],
  [String.raw`\\?\c:`, "invalid_input"],
  [String.raw`\\?\C:x`, "invalid_input"],
  // No prefix at all: not absolute.
  [String.raw`\\server`, "invalid_input"],
  [String.raw`\\\server\share`, "invalid_input"],
  [String.raw`x\y`, "invalid_input"],
  ["x", "invalid_input"],
  [String.raw`..\x`, "invalid_input"],
  [String.raw`\rooted`, "invalid_input"],
  ["C:drive-relative", "invalid_input"],
  // Empty and NUL.
  ["", "invalid_input"],
  ["  \t", "invalid_input"],
  ["C:\\x\u0000y", "invalid_input"],
  // Legacy DOS device names as any component.
  [String.raw`C:\x\COM1.rml`, "invalid_input"],
  [String.raw`C:\x\NUL`, "invalid_input"],
  [String.raw`C:\CON`, "invalid_input"],
  [String.raw`C:\NUL\x.rml`, "invalid_input"],
  ["C:/x/aux.txt", "invalid_input"],
  [String.raw`\\?\C:\x\PRN`, "invalid_input"],
  ["C:\\x\\COM\u00b9.rml", "invalid_input"],
  [String.raw`C:\x\nul .txt`, "invalid_input"],
  [String.raw`C:\x\LPT9.`, "invalid_input"],
  // Names that only resemble a device are fine.
  [String.raw`C:\x\console.rml`, "ok"],
  [String.raw`C:\x\COM10.rml`, "ok"],
  [String.raw`C:\NULL`, "ok"],
];

type Result = { ok: unknown } | { error: { code: string; message: string } };

/** Invokes one mock IPC command and returns its result or the thrown `{ code, message }`. */
async function call(page: Page, command: string, args?: unknown): Promise<Result> {
  return page.evaluate(
    async ({ command, args }) => {
      try {
        return { ok: await window.__TAURI_INTERNALS__.invoke(command, args) };
      } catch (error) {
        return { error: error as { code: string; message: string } };
      }
    },
    { command, args },
  );
}

async function outcomeOf(page: Page, command: string, args?: unknown): Promise<string> {
  const result = await call(page, command, args);
  return "error" in result ? result.error.code : "ok";
}

test.describe("the mock's user path rule", () => {
  test.beforeEach(async ({ page }) => {
    await loadScenario(page);
  });

  test("agrees with the backend parser on every spelling in the table", async ({ page }) => {
    for (const [text, expected] of CASES) {
      const outcome = await outcomeOf(page, "import_game_log", { request: { path: text } });

      expect(outcome, JSON.stringify(text)).toBe(expected);
    }
  });

  test("every path-taking command refuses a network share before it does anything else", async ({
    page,
  }) => {
    const share = String.raw`\\server\share\x`;
    const commands: ReadonlyArray<readonly [string, unknown]> = [
      [
        "load_project",
        {
          paths: {
            gameDir: "C:/ok",
            workshopDir: "C:/ok",
            modsConfig: share,
            profileDir: "C:/ok",
          },
        },
      ],
      ["save_app_config", { config: { gameDir: "C:/ok", workshopDir: share, modsConfig: null } }],
      ["import_rimsort", { paths: { userRules: null, communityRules: share, steamDb: null } }],
      ["export_patch", { request: { patchId: "none", outDir: share, install: false } }],
      ["export_assignment", { request: { assignmentId: "none", outDir: share, install: false } }],
      ["export_order_file", { request: { path: share } }],
      ["preview_order_import_file", { request: { path: share } }],
      ["preview_copy_organization", { request: { sourceDir: share, mode: "merge" } }],
      ["copy_organization", { request: { sourceDir: share, mode: "merge" } }],
      ["import_game_log", { request: { path: share } }],
    ];

    for (const [command, args] of commands) {
      expect(await outcomeOf(page, command, args), command).toBe("network_path_refused");
    }
  });

  test("a refused path in save_app_config saves nothing, a blank one still unpins", async ({
    page,
  }) => {
    await page.evaluate(() => {
      window.__SAVE_APP_CONFIG_CALLS__ = [];
    });

    const refused = await outcomeOf(page, "save_app_config", {
      config: { gameDir: "C:/ok", workshopDir: String.raw`\\server\share\w`, modsConfig: null },
    });
    const blank = await outcomeOf(page, "save_app_config", {
      config: { gameDir: "C:/ok", workshopDir: "  ", modsConfig: null },
    });

    expect(refused).toBe("network_path_refused");
    expect(blank).toBe("ok");
    const saved = await page.evaluate(() => window.__SAVE_APP_CONFIG_CALLS__ ?? []);
    expect(saved).toHaveLength(1);
  });

  test("get_default_paths reports a typed share instead of dropping it silently", async ({
    page,
  }) => {
    const result = await call(page, "get_default_paths", {
      request: { gameDir: String.raw`\\server\share\RimWorld` },
    });

    expect("ok" in result).toBe(true);
    const answer = (result as { ok: { gameDir: string; warningCode: unknown } }).ok;
    expect(answer.gameDir).toBe("C:/RimWorld");
    expect(answer.warningCode).toEqual({
      kind: "pathRefused",
      field: "gameDir",
      reason: "networkShare",
      source: "typed",
    });
  });
});
