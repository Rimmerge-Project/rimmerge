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
