// Relative import, not the `@/*` alias: `e2e/specs/dashboard.spec.ts`
// pulls this file in (type-only) for its ambient `Window` augmentation,
// under `e2e/tsconfig.json`'s own program, which doesn't define that
// alias.
import { type IpcFixtures, installMockIpc } from "./services/ipc.mock";

declare global {
  interface Window {
    /**
     * Set by the Playwright mock-IPC e2e tier (via `page.addInitScript`,
     * before this module ever runs) to a plain, JSON-serializable
     * {@link IpcFixtures} object. Never set in a real Tauri build or in
     * the CDP smoke tier, so this is a no-op there.
     */
    __E2E_MOCK_IPC__?: IpcFixtures;
  }
}

if (window.__E2E_MOCK_IPC__) {
  installMockIpc(window.__E2E_MOCK_IPC__);
}
