// LaunchGameButton: the click against a fresh status, the Apply-first prompt, the hand-off to
// Apply, the in-flight guards, and the unknown (pending or failed) status.

import { useQueryCache } from "@pinia/colada";
import { clearMocks } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { flush } from "@/components/apply/ApplyDialog.test-support";
import {
  installBackend,
  type LaunchBackend,
  mountBothLaunchButtons,
  mountLaunchButton,
  NEEDS_APPLY,
  newBackend,
  READY,
  resetLaunchState,
} from "@/components/launch/LaunchGameButton.test-support";
import { useGameLaunchStore } from "@/stores/gameLaunch";

const toastAddSpy = vi.hoisted(() => vi.fn());
vi.mock("primevue/usetoast", () => ({ useToast: () => ({ add: toastAddSpy }) }));

const BUTTON = '[data-testid="shell-launch-button"]';
const STRIP_BUTTON = '[data-testid="guide-launch-button"]';

async function mountWith(backend: LaunchBackend, variant: "sidebar" | "strip" = "sidebar") {
  installBackend(backend);
  const wrapper = mountLaunchButton(variant);
  await flush();
  return wrapper;
}

describe("LaunchGameButton", () => {
  beforeEach(() => {
    toastAddSpy.mockClear();
  });
  afterEach(() => {
    resetLaunchState();
    clearMocks();
    document.body.innerHTML = "";
  });

  it("launches with ifNotApplied refuse when the status is ready, and shows Starting", async () => {
    const backend = newBackend(READY);
    const wrapper = await mountWith(backend);

    await wrapper.get(BUTTON).trigger("click");
    await flush();

    expect(backend.launchCalls).toEqual([{ ifNotApplied: "refuse" }]);
    expect(wrapper.get(BUTTON).text()).toBe("Starting RimWorld…");
    expect(wrapper.get(BUTTON).attributes("disabled")).toBeDefined();
    expect(toastAddSpy.mock.calls.at(-1)?.[0].summary).toBe("Asked Steam to start RimWorld.");
    expect(wrapper.get('[data-testid="launch-game-live-region"]').text()).toBe(
      "Asked Steam to start RimWorld.",
    );
  });

  it("uses the strip's test id for the strip variant", async () => {
    const wrapper = await mountWith(newBackend(READY), "strip");

    expect(wrapper.find(STRIP_BUTTON).exists()).toBe(true);
    expect(wrapper.find(BUTTON).exists()).toBe(false);
  });

  it("decides from a fresh status, not the one on screen", async () => {
    const backend = newBackend(READY);
    const wrapper = await mountWith(backend);
    backend.status = NEEDS_APPLY;

    await wrapper.get(BUTTON).trigger("click");
    await flush();

    expect(backend.launchCalls).toEqual([]);
    expect(wrapper.find('[data-testid="launch-apply-first"]').exists()).toBe(true);
  });

  describe("Apply first? prompt", () => {
    it("opens on needsApply with Cancel focused and launches nothing", async () => {
      const backend = newBackend(NEEDS_APPLY);
      const wrapper = await mountWith(backend);

      await wrapper.get(BUTTON).trigger("click");
      await flush();

      expect(wrapper.get('[data-testid="launch-apply-first-sentence"]').text()).toContain(
        "doesn't hold the current order",
      );
      expect(document.activeElement).toBe(
        wrapper.get('[data-testid="launch-apply-first-cancel"]').element,
      );
      expect(backend.launchCalls).toEqual([]);
    });

    it("words the unscanned-activation reason on its own", async () => {
      const backend = newBackend({
        kind: "needsApply",
        route: "steam",
        reason: "activationChangesNotScanned",
      });
      const wrapper = await mountWith(backend);

      await wrapper.get(BUTTON).trigger("click");
      await flush();

      expect(wrapper.get('[data-testid="launch-apply-first-sentence"]').text()).toContain(
        "Your pending activation changes",
      );
    });

    it("Cancel closes it with no launch and no apply", async () => {
      const backend = newBackend(NEEDS_APPLY);
      const wrapper = await mountWith(backend);
      await wrapper.get(BUTTON).trigger("click");
      await flush();

      await wrapper.get('[data-testid="launch-apply-first-cancel"]').trigger("click");
      await flush();

      expect(wrapper.find('[data-testid="launch-apply-first"]').exists()).toBe(false);
      expect(backend.launchCalls).toEqual([]);
      expect(backend.applyCalls).toEqual([]);
    });

    it("Launch anyway launches with launchAnyway and applies nothing", async () => {
      const backend = newBackend(NEEDS_APPLY);
      const wrapper = await mountWith(backend);
      await wrapper.get(BUTTON).trigger("click");
      await flush();

      await wrapper.get('[data-testid="launch-apply-first-anyway"]').trigger("click");
      await flush();

      expect(backend.launchCalls).toEqual([{ ifNotApplied: "launchAnyway" }]);
      expect(backend.applyCalls).toEqual([]);
    });

    it("Apply first opens its own Apply dialog and launches once the apply wrote ModsConfig.xml", async () => {
      const backend = newBackend(NEEDS_APPLY);
      const wrapper = await mountWith(backend);
      await wrapper.get(BUTTON).trigger("click");
      await flush();

      await wrapper.get('[data-testid="launch-apply-first-apply"]').trigger("click");
      await flush();
      expect(wrapper.find('[data-testid="apply-dialog"]').exists()).toBe(true);
      expect(backend.launchCalls).toEqual([]);
      backend.status = READY;

      await wrapper.get('[data-testid="apply-dialog-submit"]').trigger("click");
      await flush();

      expect(backend.applyCalls).toHaveLength(1);
      expect(backend.launchCalls).toEqual([{ ifNotApplied: "refuse" }]);
    });

    it("does not launch, and says so, when the apply did not write ModsConfig.xml", async () => {
      const backend = newBackend(NEEDS_APPLY);
      const wrapper = await mountWith(backend);
      await wrapper.get(BUTTON).trigger("click");
      await flush();
      await wrapper.get('[data-testid="launch-apply-first-apply"]').trigger("click");
      await flush();

      await wrapper
        .get('[data-testid="apply-dialog-write-modsconfig-checkbox"] input')
        .setValue(false);
      await wrapper.get('[data-testid="apply-dialog-submit"]').trigger("click");
      await flush();

      expect(backend.applyCalls).toHaveLength(1);
      expect(backend.launchCalls).toEqual([]);
      expect(toastAddSpy.mock.calls.map(([toast]) => toast.summary)).toContain(
        "ModsConfig.xml wasn't written, so RimWorld wasn't started.",
      );
    });

    it("applies nothing and launches nothing when the Apply dialog is cancelled", async () => {
      const backend = newBackend(NEEDS_APPLY);
      const wrapper = await mountWith(backend);
      await wrapper.get(BUTTON).trigger("click");
      await flush();
      await wrapper.get('[data-testid="launch-apply-first-apply"]').trigger("click");
      await flush();

      await wrapper.get('[data-testid="apply-dialog-cancel"]').trigger("click");
      await flush();

      expect(wrapper.find('[data-testid="apply-dialog"]').exists()).toBe(false);
      expect(backend.launchCalls).toEqual([]);
      expect(backend.applyCalls).toEqual([]);
    });
  });

  describe("states that are not clickable", () => {
    it("reads RimWorld is running, disabled, when the game is running", async () => {
      const backend = newBackend({ kind: "gameRunning" });
      const wrapper = await mountWith(backend);

      expect(wrapper.get(BUTTON).text()).toBe("RimWorld is running");
      expect(wrapper.get(BUTTON).attributes("disabled")).toBeDefined();
      await wrapper.get(BUTTON).trigger("click");
      await flush();
      expect(backend.launchCalls).toEqual([]);
    });

    it("is disabled with the missing-executable hint when the install is unavailable", async () => {
      const wrapper = await mountWith(
        newBackend({ kind: "unavailable", reason: "executableMissing" }),
      );

      expect(wrapper.get(BUTTON).text()).toBe("Launch RimWorld");
      expect(wrapper.get(BUTTON).attributes("disabled")).toBeDefined();
      expect(wrapper.get('[data-testid="launch-game-hint"]').text()).toBe(
        "RimWorldWin64.exe isn't in the install folder.",
      );
    });

    it("is disabled with no hint while the status has not answered", async () => {
      const backend = newBackend(READY);
      backend.statusGate = new Promise(() => undefined);
      const wrapper = await mountWith(backend);

      expect(wrapper.get(BUTTON).attributes("disabled")).toBeDefined();
      expect(wrapper.find('[data-testid="launch-game-hint"]').exists()).toBe(false);
    });

    it("is disabled with a failure hint when the status call failed, and a click launches nothing", async () => {
      const backend = newBackend(READY);
      backend.statusError = { code: "internal", message: "boom" };
      const wrapper = await mountWith(backend);

      expect(wrapper.get(BUTTON).attributes("disabled")).toBeDefined();
      expect(wrapper.get('[data-testid="launch-game-hint"]').text()).toBe(
        "Couldn't check whether RimWorld can be launched.",
      );
      await wrapper.get(BUTTON).trigger("click");
      await flush();
      expect(backend.launchCalls).toEqual([]);
    });
  });

  describe("requests in flight", () => {
    it("disables the button while launch_game is pending, so a second click cannot queue another", async () => {
      const backend = newBackend(READY);
      let release: () => void = () => undefined;
      backend.launchGate = new Promise<void>((resolve) => {
        release = resolve;
      });
      const wrapper = await mountWith(backend);

      await wrapper.get(BUTTON).trigger("click");
      await flush();
      expect(wrapper.get(BUTTON).attributes("disabled")).toBeDefined();
      await wrapper.get(BUTTON).trigger("click");
      await flush();
      expect(backend.launchCalls).toHaveLength(1);

      release();
      await flush();
      expect(wrapper.get(BUTTON).text()).toBe("Starting RimWorld…");
    });

    it("waits behind a held status call without launching, then continues when it answers", async () => {
      const backend = newBackend(READY);
      const wrapper = await mountWith(backend);
      let release: () => void = () => undefined;
      backend.statusGate = new Promise<void>((resolve) => {
        release = resolve;
      });

      await wrapper.get(BUTTON).trigger("click");
      await flush();
      expect(backend.launchCalls).toEqual([]);
      expect(wrapper.get(BUTTON).attributes("disabled")).toBeDefined();

      release();
      await flush();
      expect(backend.launchCalls).toEqual([{ ifNotApplied: "refuse" }]);
    });
  });

  describe("a click superseded by an invalidation", () => {
    const UNSCANNED = {
      kind: "needsApply",
      route: "steam",
      reason: "activationChangesNotScanned",
    } as const;
    const SENTENCE = '[data-testid="launch-apply-first-sentence"]';

    function newGate(): { gate: Promise<void>; release: () => void } {
      let release: () => void = () => undefined;
      const gate = new Promise<void>((resolve) => {
        release = resolve;
      });
      return { gate, release };
    }

    it("decides from the newest status when an invalidation supersedes the click's read", async () => {
      const backend = newBackend(READY);
      const wrapper = await mountWith(backend);
      backend.status = UNSCANNED;
      const held = newGate();
      backend.statusGate = held.gate;

      await wrapper.get(BUTTON).trigger("click");
      useQueryCache()
        .invalidateQueries()
        .catch(() => undefined);
      held.release();
      await flush();

      expect(backend.launchCalls).toEqual([]);
      expect(wrapper.get(SENTENCE).text()).toContain("Your pending activation changes");
    });

    it("waits for a superseding read that is still pending", async () => {
      const backend = newBackend(READY);
      const wrapper = await mountWith(backend);
      backend.status = UNSCANNED;
      const first = newGate();
      backend.statusGate = first.gate;

      await wrapper.get(BUTTON).trigger("click");
      const second = newGate();
      backend.statusGate = second.gate;
      useQueryCache()
        .invalidateQueries()
        .catch(() => undefined);
      first.release();
      await flush();
      expect(backend.launchCalls).toEqual([]);
      expect(wrapper.find(SENTENCE).exists()).toBe(false);

      second.release();
      await flush();

      expect(backend.launchCalls).toEqual([]);
      expect(wrapper.get(SENTENCE).text()).toContain("Your pending activation changes");
    });

    it("keeps waiting when a second invalidation supersedes the newer read", async () => {
      const backend = newBackend(READY);
      const wrapper = await mountWith(backend);
      backend.status = UNSCANNED;
      const first = newGate();
      backend.statusGate = first.gate;

      await wrapper.get(BUTTON).trigger("click");
      const second = newGate();
      backend.statusGate = second.gate;
      useQueryCache()
        .invalidateQueries()
        .catch(() => undefined);
      first.release();
      await flush();
      const third = newGate();
      backend.statusGate = third.gate;
      useQueryCache()
        .invalidateQueries()
        .catch(() => undefined);
      second.release();
      await flush();
      expect(backend.launchCalls).toEqual([]);
      expect(wrapper.find(SENTENCE).exists()).toBe(false);

      third.release();
      await flush();

      expect(backend.launchCalls).toEqual([]);
      expect(wrapper.get(SENTENCE).text()).toContain("Your pending activation changes");
    });
  });

  describe("launch failures", () => {
    it("says RimWorld is already running, with its own sentence, for rimworld_running", async () => {
      const backend = newBackend(READY);
      backend.launchError = { code: "rimworld_running", message: "running" };
      const wrapper = await mountWith(backend);

      await wrapper.get(BUTTON).trigger("click");
      await flush();

      const summaries = toastAddSpy.mock.calls.map(([toast]) => toast.summary);
      expect(summaries).toEqual(["RimWorld is already running."]);
    });

    it("reopens the Apply-first prompt when launch_game itself refuses with order_not_applied", async () => {
      const backend = newBackend(READY);
      backend.launchError = { code: "order_not_applied", message: "not applied" };
      // The order moves between the click's status read and the launch call.
      backend.onLaunch = () => {
        backend.status = NEEDS_APPLY;
      };
      const wrapper = await mountWith(backend);

      await wrapper.get(BUTTON).trigger("click");
      await flush();

      expect(backend.launchCalls).toEqual([{ ifNotApplied: "refuse" }]);
      expect(wrapper.find('[data-testid="launch-apply-first"]').exists()).toBe(true);
      expect(toastAddSpy).not.toHaveBeenCalled();
    });

    it("reopens the prompt once, without looping, when the launch after Apply first is refused", async () => {
      const backend = newBackend(NEEDS_APPLY);
      backend.launchError = { code: "order_not_applied", message: "not applied" };
      const wrapper = await mountWith(backend);
      await wrapper.get(BUTTON).trigger("click");
      await flush();
      await wrapper.get('[data-testid="launch-apply-first-apply"]').trigger("click");
      await flush();

      await wrapper.get('[data-testid="apply-dialog-submit"]').trigger("click");
      await flush();

      expect(backend.applyCalls).toHaveLength(1);
      expect(backend.launchCalls).toEqual([{ ifNotApplied: "refuse" }]);
      expect(wrapper.find('[data-testid="launch-apply-first"]').exists()).toBe(true);
    });

    it("shows a coded error with technical details for another failure", async () => {
      const backend = newBackend(READY);
      backend.launchError = { code: "steam_launch_failed", message: "no handler" };
      const wrapper = await mountWith(backend);

      await wrapper.get(BUTTON).trigger("click");
      await flush();

      const toast = toastAddSpy.mock.calls.at(-1)?.[0];
      expect(toast.severity).toBe("error");
      expect(toast.summary).toBe("Steam didn't start RimWorld");
      expect(toast.detail).toContain("then start RimWorld from Steam.");
      expect(toast.detail).toContain("no handler");
    });
  });

  it("speaks a repeat launch again by clearing the live region first", async () => {
    const backend = newBackend(READY);
    const wrapper = await mountWith(backend);
    const region = wrapper.get('[data-testid="launch-game-live-region"]').element;
    let mutations = 0;
    const observer = new MutationObserver((records) => {
      mutations += records.length;
    });
    observer.observe(region, { childList: true, characterData: true, subtree: true });

    await wrapper.get(BUTTON).trigger("click");
    await flush();
    useGameLaunchStore().endStarting();
    await wrapper.vm.$nextTick();
    await wrapper.get(BUTTON).trigger("click");
    await flush();
    observer.disconnect();

    // Set once for the first launch; cleared and set again for the second. Without the clear,
    // the second identical text would change nothing and a screen reader would stay silent.
    expect(mutations).toBe(3);
    expect(region.textContent).toBe("Asked Steam to start RimWorld.");
  });

  describe("both placements mounted", () => {
    const STRIP = STRIP_BUTTON;

    it("disables the other button while one launch is in flight, then reads Starting on both", async () => {
      const backend = newBackend(READY);
      let release: () => void = () => undefined;
      backend.launchGate = new Promise<void>((resolve) => {
        release = resolve;
      });
      installBackend(backend);
      const wrapper = mountBothLaunchButtons();
      await flush();

      await wrapper.get(BUTTON).trigger("click");
      await flush();
      expect(wrapper.get(STRIP).attributes("disabled")).toBeDefined();
      await wrapper.get(STRIP).trigger("click");
      await flush();
      expect(backend.launchCalls).toHaveLength(1);

      release();
      await flush();
      expect(wrapper.get(BUTTON).text()).toBe("Starting RimWorld…");
      expect(wrapper.get(STRIP).text()).toBe("Starting RimWorld…");
      await wrapper.get(STRIP).trigger("click");
      await flush();
      expect(backend.launchCalls).toHaveLength(1);
    });
  });
});
