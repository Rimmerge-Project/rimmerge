import { clearMocks } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it, vi } from "vitest";

import {
  discardPendingChanges,
  getDashboard,
  moveMods,
  RimmergeError,
  revertOrderToFile,
  setSessionLostHandler,
} from "@/services/ipc";
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

describe("the reorder commands", () => {
  afterEach(() => {
    clearMocks();
  });

  it("moveMods sends the request under its own key and returns the result", async () => {
    const seen: unknown[] = [];
    installMockIpc({
      move_mods: (payload: unknown) => {
        seen.push(payload);
        return { selected: "current", didSwitchToCurrent: true };
      },
    });

    const result = await moveMods({
      ids: ["a.mod"],
      anchor: { kind: "before", modId: "b.mod" },
    });

    expect(seen).toEqual([
      { request: { ids: ["a.mod"], anchor: { kind: "before", modId: "b.mod" } } },
    ]);
    expect(result).toEqual({ selected: "current", didSwitchToCurrent: true });
  });

  it("surfaces a stale anchor as a RimmergeError with its own code", async () => {
    installMockIpc({
      move_mods: () => {
        throw { code: "move_anchor_not_active", message: "b.mod is not active" };
      },
    });

    await expect(moveMods({ ids: ["a.mod"], anchor: { kind: "start" } })).rejects.toMatchObject({
      code: "move_anchor_not_active",
    });
  });

  it("discardPendingChanges and revertOrderToFile call their commands with no arguments", async () => {
    const calls: string[] = [];
    const empty = { unscanned: { added: [], removed: [] }, unapplied: { added: [], removed: [] } };
    installMockIpc({
      discard_pending_changes: () => {
        calls.push("discard");
        return empty;
      },
      revert_order_to_file: () => {
        calls.push("revert");
        return empty;
      },
    });

    await discardPendingChanges();
    await revertOrderToFile();

    expect(calls).toEqual(["discard", "revert"]);
  });
});
