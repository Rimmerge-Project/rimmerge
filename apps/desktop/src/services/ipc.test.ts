import { clearMocks } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it, vi } from "vitest";

import { getDashboard, RimmergeError, setSessionLostHandler } from "@/services/ipc";
import { installMockIpc } from "@/services/ipc.mock";

describe("setSessionLostHandler", () => {
  afterEach(() => {
    clearMocks();
    // Every test registers its own handler — leaving a spy from a
    // previous test registered would let it "see" a later test's calls
    // too, since the handler lives in module-level state.
    setSessionLostHandler(() => {});
  });

  it("is called with the code when a command fails with session_lost", async () => {
    installMockIpc({
      get_dashboard: () => {
        throw {
          code: "session_lost",
          message: "the session was lost after an earlier internal error",
        };
      },
    });
    const handler = vi.fn();
    setSessionLostHandler(handler);

    await expect(getDashboard()).rejects.toBeInstanceOf(RimmergeError);

    expect(handler).toHaveBeenCalledExactlyOnceWith("session_lost");
  });

  it("is called with the code when a command fails with no_project_loaded", async () => {
    installMockIpc({
      get_dashboard: () => {
        throw { code: "no_project_loaded", message: "no project is loaded" };
      },
    });
    const handler = vi.fn();
    setSessionLostHandler(handler);

    await expect(getDashboard()).rejects.toBeInstanceOf(RimmergeError);

    expect(handler).toHaveBeenCalledExactlyOnceWith("no_project_loaded");
  });

  it("is not called for any other command error code", async () => {
    installMockIpc({
      get_dashboard: () => {
        throw { code: "internal", message: "background task panicked" };
      },
    });
    const handler = vi.fn();
    setSessionLostHandler(handler);

    await expect(getDashboard()).rejects.toBeInstanceOf(RimmergeError);

    expect(handler).not.toHaveBeenCalled();
  });
});
