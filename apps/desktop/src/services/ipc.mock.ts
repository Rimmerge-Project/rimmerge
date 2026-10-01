import type { InvokeArgs } from "@tauri-apps/api/core";
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";

/**
 * One command's mocked response: either a fixed value or a function of
 * the invoke payload (for commands whose result depends on their
 * arguments, e.g. `list_order`'s `source`).
 */
export type MockedResponse = unknown | ((payload: InvokeArgs | undefined) => unknown);

/** Command name (as Tauri sees it, snake_case) to its mocked response. */
export type IpcFixtures = Record<string, MockedResponse>;

/**
 * Narrows a `MockedResponse` to its function arm. A plain `typeof
 * response === "function"` check at the call site can't narrow
 * `MockedResponse` itself, since `unknown | T` collapses to `unknown` —
 * this type predicate (a real runtime check, just with its result typed)
 * is what lets the call site avoid an `as` cast.
 */
function isMockedFunction(
  response: MockedResponse,
): response is (payload: InvokeArgs | undefined) => unknown {
  return typeof response === "function";
}

/**
 * Installs `mockIPC` (from `@tauri-apps/api/mocks`) with canned responses
 * for the commands in `fixtures`, for Vitest component tests and the
 * mock-IPC Playwright tier. Events are mocked too (`shouldMockEvents`),
 * so `@tauri-apps/api/event`'s `emit`/`listen` work against the same
 * in-page mock — a test can `emit("project://progress", ...)` and
 * `useSessionEvents`'s `listen` call will see it.
 *
 * An invoked command with no matching fixture throws, loudly, rather
 * than silently resolving `undefined` — a missing fixture is a test bug,
 * not a valid response.
 *
 * Also calls `mockWindows("main")`: without it,
 * `@tauri-apps/api/window`'s `getCurrentWindow()` throws synchronously
 * (no `window.__TAURI_INTERNALS__.metadata.currentWindow` to read) —
 * `PendingChangesCloseGuard.vue`, mounted unconditionally in
 * `TheShell.vue`, calls it on every mount, so every existing caller of
 * this function needs it, not just tests that exercise that guard
 * directly.
 */
export function installMockIpc(fixtures: IpcFixtures): void {
  mockWindows("main");
  mockIPC(
    (command, payload) => {
      if (!(command in fixtures)) {
        throw new Error(`installMockIpc: no fixture registered for command "${command}"`);
      }
      const response = fixtures[command];
      return isMockedFunction(response) ? response(payload) : response;
    },
    { shouldMockEvents: true },
  );
}
