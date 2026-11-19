import { clearMocks } from "@tauri-apps/api/mocks";
import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { defineComponent } from "vue";

import {
  FILE_BASELINE_POLL_MS,
  useFileBaselinePolling,
} from "@/composables/useFileBaselinePolling";
import { installMockIpc } from "@/services/ipc.mock";
import type { FileBaselineDto } from "@/types/generated/FileBaselineDto";

const QUIET: FileBaselineDto = {
  state: { kind: "fresh" },
  externalChange: { kind: "none" },
};

const Host = defineComponent({
  setup() {
    useFileBaselinePolling();
    return () => null;
  },
});

function setVisibility(state: "visible" | "hidden"): void {
  Object.defineProperty(document, "visibilityState", { value: state, configurable: true });
  document.dispatchEvent(new Event("visibilitychange"));
}

describe("useFileBaselinePolling", () => {
  let calls = 0;
  let releaseHeldCall: (() => void) | null = null;
  let isHolding = false;

  beforeEach(() => {
    vi.useFakeTimers();
    calls = 0;
    isHolding = false;
    releaseHeldCall = null;
    setVisibility("visible");
    installMockIpc({
      refresh_mods_config_baseline: async () => {
        calls += 1;
        if (isHolding) {
          await new Promise<void>((resolve) => {
            releaseHeldCall = resolve;
          });
        }
        return QUIET;
      },
    });
  });

  afterEach(() => {
    clearMocks();
    vi.useRealTimers();
    setVisibility("visible");
  });

  it("re-reads the file every interval while the window is visible", async () => {
    const wrapper = mount(Host);

    await vi.advanceTimersByTimeAsync(FILE_BASELINE_POLL_MS * 3);

    expect(calls).toBe(3);
    wrapper.unmount();
  });

  it("stops while the window is hidden and reads again as soon as it is visible", async () => {
    const wrapper = mount(Host);
    setVisibility("hidden");

    await vi.advanceTimersByTimeAsync(FILE_BASELINE_POLL_MS * 4);
    expect(calls).toBe(0);

    setVisibility("visible");
    await flushPromises();
    expect(calls).toBe(1);
    wrapper.unmount();
  });

  it("reads once when the window regains focus", async () => {
    const wrapper = mount(Host);

    window.dispatchEvent(new Event("blur"));
    window.dispatchEvent(new Event("focus"));
    await flushPromises();

    expect(calls).toBe(1);
    wrapper.unmount();
  });

  it("skips a tick while the previous call is still pending", async () => {
    isHolding = true;
    const wrapper = mount(Host);

    await vi.advanceTimersByTimeAsync(FILE_BASELINE_POLL_MS * 5);
    expect(calls).toBe(1);

    isHolding = false;
    releaseHeldCall?.();
    await vi.advanceTimersByTimeAsync(FILE_BASELINE_POLL_MS);
    expect(calls).toBe(2);
    wrapper.unmount();
  });

  it("keeps polling after a call fails", async () => {
    clearMocks();
    installMockIpc({
      refresh_mods_config_baseline: () => {
        calls += 1;
        throw new Error("no project loaded");
      },
    });
    const wrapper = mount(Host);

    await vi.advanceTimersByTimeAsync(FILE_BASELINE_POLL_MS * 2);

    expect(calls).toBe(2);
    wrapper.unmount();
  });
});
