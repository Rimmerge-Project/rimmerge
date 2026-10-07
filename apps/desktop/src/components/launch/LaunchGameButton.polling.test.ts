// LaunchGameButton: the 5 s status poll (paused while hidden, never stacking) and the 30 s
// Starting window, on fake timers.

import { clearMocks } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  installBackend,
  mountBothLaunchButtons,
  mountLaunchButton,
  newBackend,
  READY,
  resetLaunchState,
} from "@/components/launch/LaunchGameButton.test-support";
import { GAME_LAUNCH_POLL_MS } from "@/composables/useGameLaunchPolling";
import { STARTING_WINDOW_MS } from "@/stores/gameLaunch";

vi.mock("primevue/usetoast", () => ({ useToast: () => ({ add: vi.fn() }) }));

const BUTTON = '[data-testid="shell-launch-button"]';

/** Lets pending promise chains settle without moving the fake clock. */
const settle = () => vi.advanceTimersByTimeAsync(0);

function setVisibility(state: "visible" | "hidden"): void {
  vi.spyOn(document, "visibilityState", "get").mockReturnValue(state);
  document.dispatchEvent(new Event("visibilitychange"));
}

describe("LaunchGameButton polling", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    resetLaunchState();
    vi.restoreAllMocks();
    vi.useRealTimers();
    clearMocks();
    document.body.innerHTML = "";
  });

  it("re-reads the status every five seconds while the window is visible", async () => {
    const backend = newBackend(READY);
    installBackend(backend);
    mountLaunchButton();
    await settle();
    const afterMount = backend.statusCalls;

    await vi.advanceTimersByTimeAsync(GAME_LAUNCH_POLL_MS * 3);

    expect(backend.statusCalls - afterMount).toBe(3);
  });

  it("shows the game running within one poll of it starting", async () => {
    const backend = newBackend(READY);
    installBackend(backend);
    const wrapper = mountLaunchButton();
    await settle();
    expect(wrapper.get(BUTTON).text()).toBe("Launch RimWorld");

    backend.status = { kind: "gameRunning" };
    await vi.advanceTimersByTimeAsync(GAME_LAUNCH_POLL_MS);

    expect(wrapper.get(BUTTON).text()).toBe("RimWorld is running");
  });

  it("stops polling while hidden and resumes the interval when visible", async () => {
    const backend = newBackend(READY);
    installBackend(backend);
    mountLaunchButton();
    await settle();

    setVisibility("hidden");
    await settle();
    const whenHidden = backend.statusCalls;
    await vi.advanceTimersByTimeAsync(GAME_LAUNCH_POLL_MS * 4);
    expect(backend.statusCalls).toBe(whenHidden);

    setVisibility("visible");
    await settle();
    const whenVisible = backend.statusCalls;
    // Colada's own visibility refresh can account for the first read; the interval is the rest.
    await vi.advanceTimersByTimeAsync(GAME_LAUNCH_POLL_MS * 2);
    expect(backend.statusCalls - whenVisible).toBe(2);
  });

  it("reads once when the window regains focus, without waiting for a period", async () => {
    const backend = newBackend(READY);
    installBackend(backend);
    mountLaunchButton();
    await settle();
    window.dispatchEvent(new Event("blur"));
    await settle();
    const beforeFocus = backend.statusCalls;

    window.dispatchEvent(new Event("focus"));
    await settle();

    expect(backend.statusCalls - beforeFocus).toBe(1);
  });

  it("polls from one place only when both placements are mounted", async () => {
    const backend = newBackend(READY);
    installBackend(backend);
    mountBothLaunchButtons();
    await settle();
    const afterMount = backend.statusCalls;

    await vi.advanceTimersByTimeAsync(GAME_LAUNCH_POLL_MS * 4);

    expect(backend.statusCalls - afterMount).toBe(4);
  });

  it("does not start another request while one is still pending", async () => {
    const backend = newBackend(READY);
    backend.statusGate = new Promise(() => undefined);
    installBackend(backend);
    mountLaunchButton();
    await settle();
    expect(backend.statusCalls).toBe(1);

    await vi.advanceTimersByTimeAsync(GAME_LAUNCH_POLL_MS * 4);

    expect(backend.statusCalls).toBe(1);
  });

  it("polls again once the pending request has answered", async () => {
    const backend = newBackend(READY);
    let release: () => void = () => undefined;
    backend.statusGate = new Promise<void>((resolve) => {
      release = resolve;
    });
    installBackend(backend);
    mountLaunchButton();
    await settle();
    await vi.advanceTimersByTimeAsync(GAME_LAUNCH_POLL_MS * 2);
    expect(backend.statusCalls).toBe(1);

    backend.statusGate = null;
    release();
    await settle();
    await vi.advanceTimersByTimeAsync(GAME_LAUNCH_POLL_MS);

    expect(backend.statusCalls).toBe(2);
  });

  it("keeps Starting until the status reads running", async () => {
    const backend = newBackend(READY);
    installBackend(backend);
    const wrapper = mountLaunchButton();
    await settle();
    await wrapper.get(BUTTON).trigger("click");
    await settle();
    expect(wrapper.get(BUTTON).text()).toBe("Starting RimWorld…");

    backend.status = { kind: "gameRunning" };
    await vi.advanceTimersByTimeAsync(GAME_LAUNCH_POLL_MS);

    expect(wrapper.get(BUTTON).text()).toBe("RimWorld is running");
  });

  it("gives up on Starting after thirty seconds and offers the launch again", async () => {
    const backend = newBackend(READY);
    installBackend(backend);
    const wrapper = mountLaunchButton();
    await settle();
    await wrapper.get(BUTTON).trigger("click");
    await settle();
    expect(wrapper.get(BUTTON).text()).toBe("Starting RimWorld…");

    await vi.advanceTimersByTimeAsync(STARTING_WINDOW_MS - 1);
    expect(wrapper.get(BUTTON).text()).toBe("Starting RimWorld…");
    await vi.advanceTimersByTimeAsync(1);

    expect(wrapper.get(BUTTON).text()).toBe("Launch RimWorld");
    expect(wrapper.get(BUTTON).attributes("disabled")).toBeUndefined();
  });
});
