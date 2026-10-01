import { PiniaColada } from "@pinia/colada";
import Aura from "@primevue/themes/aura";
import { clearMocks } from "@tauri-apps/api/mocks";
import { mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import PrimeVue from "primevue/config";
import Select from "primevue/select";
import { afterEach, describe, expect, it, vi } from "vitest";

import LanguagePicker from "@/components/settings/LanguagePicker.vue";
import { createAppI18n, registerAppI18n, registerPrimeVueLocaleConfig } from "@/i18n/i18n";
import { installMockIpc } from "@/services/ipc.mock";
import { usePreferencesStore } from "@/stores/preferences";
import type { AppLinkTargetDto } from "@/types/generated/AppLinkTargetDto";

/**
 * `pinia` is the same instance the test just made active with
 * `setActivePinia` — `PiniaColada` still needs its own `app.use`, but
 * the store a test reads back afterward (`usePreferencesStore()`,
 * called outside the mounted app) must resolve to that same instance,
 * not a second one created here.
 */
function mountPicker(
  pinia: ReturnType<typeof createPinia>,
  openAppLinkCalls: AppLinkTargetDto[] = [],
) {
  installMockIpc({
    open_app_link: (payload: unknown) => {
      openAppLinkCalls.push((payload as { target: AppLinkTargetDto }).target);
      return null;
    },
  });
  return mount(LanguagePicker, {
    global: {
      plugins: [pinia, PiniaColada, [PrimeVue, { theme: { preset: Aura } }]],
    },
  });
}

describe("LanguagePicker", () => {
  afterEach(() => {
    localStorage.clear();
    clearMocks();
    vi.restoreAllMocks();
  });

  it("lists System, then every supported locale by its native name", () => {
    const pinia = createPinia();
    setActivePinia(pinia);
    registerAppI18n(createAppI18n());
    registerPrimeVueLocaleConfig({});
    vi.spyOn(navigator, "languages", "get").mockReturnValue(["en-US"]);

    const wrapper = mountPicker(pinia);
    const options = wrapper.findComponent(Select).props("options") as { label: string }[];

    expect(options.map((option) => option.label)).toEqual([
      "System (English)",
      "English",
      "简体中文 (preview)",
      "Português (Brasil)",
      "Русский (preview)",
      "Українська (preview)",
      "Polski (preview)",
      "Deutsch (preview)",
      "Français (preview)",
      "Español (España) (preview)",
      "Türkçe (preview)",
      "日本語 (preview)",
      "한국어 (preview)",
      "繁體中文 (preview)",
    ]);
  });

  it("shows no preview note while English (the reviewed locale) is selected", () => {
    const pinia = createPinia();
    setActivePinia(pinia);
    registerAppI18n(createAppI18n());
    registerPrimeVueLocaleConfig({});
    usePreferencesStore().setLocale("en");

    const wrapper = mountPicker(pinia);

    expect(wrapper.find('[data-testid="language-picker-preview-note"]').exists()).toBe(false);
  });

  it("shows the preview note, with a clickable issues link, once a preview locale is selected", async () => {
    const pinia = createPinia();
    setActivePinia(pinia);
    registerAppI18n(createAppI18n());
    registerPrimeVueLocaleConfig({});

    const wrapper = mountPicker(pinia);
    await wrapper.findComponent(Select).vm.$emit("update:modelValue", "zh-CN");

    // `handleChange` is async (it awaits `setAppLocale`, which itself
    // awaits a dynamic `import()` for zh-CN/pt-BR) but `$emit` does not
    // await its listener — `vi.waitFor` polls until the DOM actually
    // reflects the change instead of guessing how many ticks a fixed
    // flush would need (a real risk under a full, loaded test-suite
    // run, not just in isolation).
    await vi.waitFor(() => {
      wrapper.vm.$forceUpdate();
      expect(wrapper.find('[data-testid="language-picker-preview-note"]').exists()).toBe(true);
    });

    expect(wrapper.find('[data-testid="language-picker-issues-link"]').exists()).toBe(true);
  });

  it("clicking the preview note's issues link opens the app's own fixed issues link, never a URL the frontend builds", async () => {
    const pinia = createPinia();
    setActivePinia(pinia);
    registerAppI18n(createAppI18n());
    registerPrimeVueLocaleConfig({});

    const openAppLinkCalls: AppLinkTargetDto[] = [];
    const wrapper = mountPicker(pinia, openAppLinkCalls);
    await wrapper.findComponent(Select).vm.$emit("update:modelValue", "zh-CN");
    await vi.waitFor(() => {
      wrapper.vm.$forceUpdate();
      expect(wrapper.find('[data-testid="language-picker-issues-link"]').exists()).toBe(true);
    });

    await wrapper.get('[data-testid="language-picker-issues-link"]').trigger("click");

    await vi.waitFor(() => {
      expect(openAppLinkCalls).toEqual(["githubIssues"]);
    });
  });

  it("selecting a locale applies it immediately, through setAppLocale", async () => {
    const pinia = createPinia();
    setActivePinia(pinia);
    registerAppI18n(createAppI18n());
    registerPrimeVueLocaleConfig({});

    const wrapper = mountPicker(pinia);
    await wrapper.findComponent(Select).vm.$emit("update:modelValue", "pt-BR");

    await vi.waitFor(() => {
      expect(usePreferencesStore().locale).toBe("pt-BR");
    });
    expect(document.documentElement.lang).toBe("pt-BR");
  });
});
