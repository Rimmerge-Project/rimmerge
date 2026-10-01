import { createPinia, setActivePinia } from "pinia";
import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  type AppI18n,
  createAppI18n,
  loadLocale,
  MissingMessageError,
  registerAppI18n,
  registerPrimeVueLocaleConfig,
  resolveStoredLocale,
  setAppLocale,
} from "@/i18n/i18n";
import { SUPPORTED_LOCALES, type SupportedLocale } from "@/i18n/locales";
import { usePreferencesStore } from "@/stores/preferences";

describe("resolveStoredLocale", () => {
  it("passes an explicit locale through unchanged", () => {
    expect(resolveStoredLocale("zh-CN")).toBe("zh-CN");
  });

  it("detects from navigator.languages for 'system'", () => {
    vi.spyOn(navigator, "languages", "get").mockReturnValue(["pt-BR"]);
    expect(resolveStoredLocale("system")).toBe("pt-BR");
    vi.restoreAllMocks();
  });
});

describe("loadLocale", () => {
  it("registers zh-CN's messages exactly once", async () => {
    const i18n = createAppI18n();
    const setSpy = vi.spyOn(i18n.global, "setLocaleMessage");

    await loadLocale(i18n, "zh-CN");
    await loadLocale(i18n, "zh-CN");

    expect(setSpy).toHaveBeenCalledTimes(1);
    expect(setSpy).toHaveBeenCalledWith(
      "zh-CN",
      expect.objectContaining({ common: expect.objectContaining({ cancel: "取消" }) }),
    );
  });

  it("is a no-op for en, already bundled at construction", async () => {
    const i18n = createAppI18n();
    const setSpy = vi.spyOn(i18n.global, "setLocaleMessage");

    await loadLocale(i18n, "en");

    expect(setSpy).not.toHaveBeenCalled();
  });
});

describe("setAppLocale", () => {
  beforeEach(() => {
    localStorage.clear();
    setActivePinia(createPinia());
    document.documentElement.lang = "";
  });

  it("loads the locale, sets the reactive i18n locale, syncs html[lang], and persists the choice", async () => {
    const i18n = createAppI18n();
    registerAppI18n(i18n);
    registerPrimeVueLocaleConfig({});

    await setAppLocale("pt-BR");

    expect(i18n.global.locale.value).toBe("pt-BR");
    expect(document.documentElement.lang).toBe("pt-BR");
    expect(usePreferencesStore().locale).toBe("pt-BR");
  });

  it("persists 'system' itself, not the resolved locale, so a later OS change is followed again", async () => {
    const i18n = createAppI18n();
    registerAppI18n(i18n);
    registerPrimeVueLocaleConfig({});
    vi.spyOn(navigator, "languages", "get").mockReturnValue(["zh-CN"]);

    await setAppLocale("system");

    expect(i18n.global.locale.value).toBe("zh-CN");
    expect(usePreferencesStore().locale).toBe("system");
    vi.restoreAllMocks();
  });

  it("copies the resolved locale's primevue.* subtree onto the registered PrimeVue locale config", async () => {
    const i18n = createAppI18n();
    registerAppI18n(i18n);
    const primeVueLocale: Record<string, unknown> = {};
    registerPrimeVueLocaleConfig(primeVueLocale);

    await setAppLocale("en");

    expect(primeVueLocale["emptyMessage"]).toBe("No available options");
  });
});
describe("setAppLocale and PrimeVue strings", () => {
  beforeEach(() => {
    localStorage.clear();
    setActivePinia(createPinia());
  });

  it("shows English PrimeVue strings, not the previous locale's, for a locale with no primevue subtree", async () => {
    const i18n = createAppI18n();
    registerAppI18n(i18n);
    const primeVueLocale: Record<string, unknown> = {};
    registerPrimeVueLocaleConfig(primeVueLocale);
    await setAppLocale("zh-CN");
    expect(primeVueLocale["emptyMessage"]).toBe("没有可用选项");

    i18n.global.setLocaleMessage("ru", {});
    await setAppLocale("ru");

    expect(primeVueLocale["emptyMessage"]).toBe("No available options");
  });

  it("keeps the English nested aria strings a locale's partial subtree leaves out", async () => {
    const i18n = createAppI18n();
    registerAppI18n(i18n);
    const primeVueLocale: Record<string, unknown> = {};
    registerPrimeVueLocaleConfig(primeVueLocale);
    i18n.global.setLocaleMessage("de", { primevue: { aria: { close: "Schließen" } } });
    await setAppLocale("de");

    expect(primeVueLocale["aria"]).toMatchObject({ close: "Schließen", next: "Next" });
  });
});

describe("a locale whose catalogue holds nothing yet", () => {
  // Every new locale starts as `{}` until its translator finishes; the
  // app must read as English in the meantime, never as raw dotted keys.
  const OTHER_LOCALES = SUPPORTED_LOCALES.filter((locale) => locale !== "en");

  function emptyLocaleI18n(locale: SupportedLocale): AppI18n {
    const i18n = createAppI18n();
    i18n.global.setLocaleMessage(locale, {});
    i18n.global.locale.value = locale;
    return i18n;
  }

  it.each(OTHER_LOCALES)("%s falls back to the English text for a key", (locale) => {
    const i18n = emptyLocaleI18n(locale);
    expect(i18n.global.t("shell.nav.dashboard")).toBe("Dashboard");
  });

  it.each(OTHER_LOCALES)(
    "%s falls back to the English plural form that matches the count",
    (locale) => {
      const i18n = emptyLocaleI18n(locale);
      const render = (count: number): string =>
        i18n.global.t("defGraphics.viewer.truncatedVariants", { count }, count);
      expect([render(1), render(5)]).toEqual([
        "1 more variant not shown",
        "5 more variants not shown",
      ]);
    },
  );

  it("still throws for a key en itself lacks, so a typo is not hidden by the fallback", () => {
    const i18n = emptyLocaleI18n("ru");
    expect(() => i18n.global.t("no.such.key")).toThrow(MissingMessageError);
  });

  it("loads every supported locale's catalogue file", async () => {
    const i18n = createAppI18n();
    for (const locale of SUPPORTED_LOCALES) {
      await expect(loadLocale(i18n, locale)).resolves.toBeUndefined();
    }
  });
});
