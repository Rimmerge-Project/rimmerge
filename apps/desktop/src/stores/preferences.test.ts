import { createPinia, setActivePinia } from "pinia";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { usePreferencesStore } from "@/stores/preferences";

const LOCALE_KEY = "rimmerge.locale";

describe("usePreferencesStore locale", () => {
  beforeEach(() => {
    localStorage.clear();
    setActivePinia(createPinia());
  });

  it("defaults to 'system' when nothing is persisted", () => {
    expect(usePreferencesStore().locale).toBe("system");
  });

  it("hydrates from a previously persisted supported locale", () => {
    localStorage.setItem(LOCALE_KEY, "zh-CN");
    expect(usePreferencesStore().locale).toBe("zh-CN");
  });

  it("falls back to 'system' for a corrupted stored value", () => {
    localStorage.setItem(LOCALE_KEY, "fr-FR");
    expect(usePreferencesStore().locale).toBe("system");
  });

  it("setLocale updates state and persists the choice", () => {
    const preferences = usePreferencesStore();
    preferences.setLocale("pt-BR");
    expect(preferences.locale).toBe("pt-BR");
    expect(localStorage.getItem(LOCALE_KEY)).toBe("pt-BR");
  });

  it("setLocale still updates in-memory state when localStorage throws", () => {
    const preferences = usePreferencesStore();
    const spy = vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("blocked site data");
    });
    expect(() => preferences.setLocale("en")).not.toThrow();
    expect(preferences.locale).toBe("en");
    spy.mockRestore();
  });
});

describe("usePreferencesStore panes", () => {
  const PANES_KEY = "rimmerge.modsPanes.v1";

  beforeEach(() => {
    localStorage.clear();
    setActivePinia(createPinia());
  });

  it("starts at the defaults", () => {
    const { panes } = usePreferencesStore();
    expect(panes.active).toEqual({
      sort: { key: "loadOrder", direction: "ascending" },
      view: "list",
    });
    expect(panes.inactive).toEqual({ sort: { key: "name", direction: "ascending" }, view: "list" });
  });

  it("persists a pane's sort and view and restores them in a new store", () => {
    const first = usePreferencesStore();
    first.setPaneSort("active", { key: "packageId", direction: "descending" });
    first.setPaneView("active", "groups");

    setActivePinia(createPinia());
    const { panes } = usePreferencesStore();

    expect(panes.active).toEqual({
      sort: { key: "packageId", direction: "descending" },
      view: "groups",
    });
    expect(panes.inactive.view).toBe("list");
  });

  it("falls back to the defaults for a stored value it does not know", () => {
    localStorage.setItem(
      PANES_KEY,
      JSON.stringify({ active: { sort: { key: "gone", direction: "ascending" }, view: "list" } }),
    );
    expect(usePreferencesStore().panes.active.sort.key).toBe("loadOrder");
  });

  it("falls back to the defaults when reading storage throws", () => {
    const spy = vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("blocked site data");
    });
    expect(usePreferencesStore().panes.active.sort.key).toBe("loadOrder");
    spy.mockRestore();
  });

  it("still changes in memory when writing storage throws", () => {
    const preferences = usePreferencesStore();
    const spy = vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("quota");
    });
    expect(() => preferences.setPaneView("inactive", "groups")).not.toThrow();
    expect(preferences.panes.inactive.view).toBe("groups");
    spy.mockRestore();
  });
});
