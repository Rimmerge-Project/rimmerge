import { PiniaColada } from "@pinia/colada";
import { clearMocks } from "@tauri-apps/api/mocks";
import { mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import { afterEach, describe, expect, it, vi } from "vitest";
import { defineComponent } from "vue";

import { useModLabel } from "@/composables/useModLabel";
import { installMockIpc } from "@/services/ipc.mock";
import type { ModNamesDto } from "@/types/generated/ModNamesDto";

const STORAGE_KEY = "rimmerge.modLabelMode";

async function flush(): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, 0));
}

/** Mounts a host component so `useModLabel` runs inside a real setup scope, wired to a fresh Pinia + Pinia Colada per test. */
function mountHarness(names: ModNamesDto = {}) {
  installMockIpc({ list_mod_names: names });

  let composable!: ReturnType<typeof useModLabel>;
  const Harness = defineComponent({
    setup() {
      composable = useModLabel();
      return {};
    },
    template: "<div />",
  });

  const pinia = createPinia();
  setActivePinia(pinia);
  const wrapper = mount(Harness, { global: { plugins: [pinia, PiniaColada] } });

  return {
    wrapper,
    get modLabel() {
      return composable;
    },
  };
}

describe("useModLabel", () => {
  afterEach(() => {
    clearMocks();
    localStorage.clear();
  });

  it("resolves the display name when the map has the id directly", async () => {
    const { modLabel } = mountHarness({ "a.mod": "A Mod" });
    await flush();

    expect(modLabel.label("a.mod")).toBe("A Mod");
  });

  it("falls back to the bare id when the map has no entry for it", async () => {
    const { modLabel } = mountHarness({ "a.mod": "A Mod" });
    await flush();

    expect(modLabel.label("unknown.mod")).toBe("unknown.mod");
  });

  it("falls back to the bare id before the names query has resolved", () => {
    const { modLabel } = mountHarness({ "a.mod": "A Mod" });

    expect(modLabel.label("a.mod")).toBe("a.mod");
  });

  it("resolves a _steam-suffixed id through its base id", async () => {
    const { modLabel } = mountHarness({ "a.mod": "A Mod" });
    await flush();

    expect(modLabel.label("a.mod_steam")).toBe("A Mod");
  });

  it("prefers a direct entry for a _steam-suffixed id over its base id's", async () => {
    const { modLabel } = mountHarness({ "a.mod": "A Mod", "a.mod_steam": "A Mod (Workshop)" });
    await flush();

    expect(modLabel.label("a.mod_steam")).toBe("A Mod (Workshop)");
  });

  it("returns the bare id in id mode even when a name is known", async () => {
    const { modLabel } = mountHarness({ "a.mod": "A Mod" });
    await flush();

    modLabel.toggle();

    expect(modLabel.mode.value).toBe("id");
    expect(modLabel.label("a.mod")).toBe("a.mod");
  });

  it("titleFor returns the id only in name mode", async () => {
    const { modLabel } = mountHarness({ "a.mod": "A Mod" });
    await flush();

    expect(modLabel.titleFor("a.mod")).toBe("a.mod");

    modLabel.toggle();
    expect(modLabel.titleFor("a.mod")).toBeUndefined();
  });

  it("toggle flips between name and id, persisting the change", () => {
    const { modLabel } = mountHarness();

    expect(modLabel.mode.value).toBe("name");
    modLabel.toggle();
    expect(modLabel.mode.value).toBe("id");
    expect(localStorage.getItem(STORAGE_KEY)).toBe("id");

    modLabel.toggle();
    expect(modLabel.mode.value).toBe("name");
    expect(localStorage.getItem(STORAGE_KEY)).toBe("name");
  });

  it("hydrates the mode from a value already in localStorage", () => {
    localStorage.setItem(STORAGE_KEY, "id");

    const { modLabel } = mountHarness();

    expect(modLabel.mode.value).toBe("id");
  });

  it("defaults to name mode and keeps working when localStorage throws on every access", async () => {
    const getSpy = vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("storage blocked");
    });
    const setSpy = vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("storage blocked");
    });

    try {
      const { modLabel } = mountHarness({ "a.mod": "A Mod" });
      expect(modLabel.mode.value).toBe("name");

      // Toggling still updates the in-memory mode even though persisting
      // it throws — a blocked store must degrade, not crash.
      expect(() => modLabel.toggle()).not.toThrow();
      expect(modLabel.mode.value).toBe("id");
    } finally {
      getSpy.mockRestore();
      setSpy.mockRestore();
    }
  });
});
