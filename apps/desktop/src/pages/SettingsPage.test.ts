import { PiniaColada } from "@pinia/colada";
import Aura from "@primevue/themes/aura";
import { clearMocks } from "@tauri-apps/api/mocks";
import { mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import PrimeVue from "primevue/config";
import ToastService from "primevue/toastservice";
import { afterEach, describe, expect, it } from "vitest";

import SettingsPage from "@/pages/SettingsPage.vue";
import { installMockIpc } from "@/services/ipc.mock";
import type { AppSettingsDto } from "@/types/generated/AppSettingsDto";
import type { AppSettingsResponseDto } from "@/types/generated/AppSettingsResponseDto";
import type { SettingsDto } from "@/types/generated/SettingsDto";

const settingsFixture: SettingsDto = {
  threshold: 80,
  enforceSoft: false,
  enforceAwareness: false,
  suggestMergeWhenClean: true,
  tieBreak: "rebuild",
  useImportedPairs: false,
  useImportedPlacements: true,
  enforceInferred: true,
  showDanglingDefReferences: false,
};

const appSettingsFixture: AppSettingsResponseDto = {
  settings: {
    network: {
      allowNetwork: true,
      checkForUpdates: true,
      autoRefreshRuleDatabases: true,
      fetchCommunityRules: true,
      fetchSteamWorkshop: true,
      fetchRimmergeRules: true,
    },
    reminders: {
      ruleDatabasesStaleAfterDays: 30,
    },
  },
  loadStatus: "loaded",
};

function mountSettingsPage(pinia: ReturnType<typeof createPinia>) {
  return mount(SettingsPage, {
    global: {
      plugins: [pinia, PiniaColada, [PrimeVue, { theme: { preset: Aura } }], ToastService],
      // The reset-confirm `Dialog` teleports by default — stubbing keeps
      // its content inside the component tree `wrapper.get` searches,
      // the same pattern `ApplyDialog.test-support.ts` uses.
      stubs: { teleport: true },
    },
  });
}

async function flush(): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, 0));
}

describe("SettingsPage", () => {
  afterEach(() => {
    clearMocks();
  });

  it("prefills every field once the settings query resolves", async () => {
    installMockIpc({
      get_app_version: "1.2.3",
      list_muted_notification_kinds: [],
      get_app_settings: appSettingsFixture,
      get_settings: settingsFixture,
      set_settings: settingsFixture,
    });
    const pinia = createPinia();
    const wrapper = mountSettingsPage(pinia);
    await flush();
    await wrapper.vm.$nextTick();

    expect(
      (wrapper.get('[data-testid="settings-threshold"] input').element as HTMLInputElement).value,
    ).toBe("80");
    expect(wrapper.get('[data-testid="settings-enforce-soft"]').attributes("data-p-checked")).toBe(
      "false",
    );
    expect(
      wrapper.get('[data-testid="settings-enforce-inferred"]').attributes("data-p-checked"),
    ).toBe("true");
    expect(
      wrapper.get('[data-testid="settings-suggest-merge-when-clean"]').attributes("data-p-checked"),
    ).toBe("true");
  });

  it("sends the enforce-inferred toggle on save", async () => {
    let received: SettingsDto | undefined;
    installMockIpc({
      get_app_version: "1.2.3",
      list_muted_notification_kinds: [],
      get_app_settings: appSettingsFixture,
      get_settings: settingsFixture,
      set_settings: (payload: unknown) => {
        received = (payload as { settings: SettingsDto }).settings;
        return settingsFixture;
      },
    });
    const pinia = createPinia();
    const wrapper = mountSettingsPage(pinia);
    await flush();
    await wrapper.vm.$nextTick();

    await wrapper.get('[data-testid="settings-enforce-inferred"] input').setValue(false);
    await wrapper.get('[data-testid="settings-save"]').trigger("click");
    await wrapper.vm.$nextTick();

    expect(received?.enforceInferred).toBe(false);
  });

  it("sends the suggest-merge-when-clean toggle on save", async () => {
    let received: SettingsDto | undefined;
    installMockIpc({
      get_app_version: "1.2.3",
      list_muted_notification_kinds: [],
      get_app_settings: appSettingsFixture,
      get_settings: settingsFixture,
      set_settings: (payload: unknown) => {
        received = (payload as { settings: SettingsDto }).settings;
        return settingsFixture;
      },
    });
    const pinia = createPinia();
    const wrapper = mountSettingsPage(pinia);
    await flush();
    await wrapper.vm.$nextTick();

    await wrapper.get('[data-testid="settings-suggest-merge-when-clean"] input').setValue(false);
    await wrapper.get('[data-testid="settings-save"]').trigger("click");
    await wrapper.vm.$nextTick();

    expect(received?.suggestMergeWhenClean).toBe(false);
  });

  it("prefills the tie-break control and the two imported-rule toggles", async () => {
    installMockIpc({
      get_app_version: "1.2.3",
      list_muted_notification_kinds: [],
      get_app_settings: appSettingsFixture,
      get_settings: settingsFixture,
      set_settings: settingsFixture,
    });
    const pinia = createPinia();
    const wrapper = mountSettingsPage(pinia);
    await flush();
    await wrapper.vm.$nextTick();

    expect(
      wrapper.get('[data-testid="settings-tie-break-rebuild"]').attributes("aria-checked"),
    ).toBe("true");
    expect(
      wrapper.get('[data-testid="settings-use-imported-pairs"]').attributes("data-p-checked"),
    ).toBe("false");
    expect(
      wrapper.get('[data-testid="settings-use-imported-placements"]').attributes("data-p-checked"),
    ).toBe("true");
  });

  it("sends the chosen tie-break on save", async () => {
    let received: SettingsDto | undefined;
    installMockIpc({
      get_app_version: "1.2.3",
      list_muted_notification_kinds: [],
      get_app_settings: appSettingsFixture,
      get_settings: settingsFixture,
      set_settings: (payload: unknown) => {
        received = (payload as { settings: SettingsDto }).settings;
        return settingsFixture;
      },
    });
    const pinia = createPinia();
    const wrapper = mountSettingsPage(pinia);
    await flush();
    await wrapper.vm.$nextTick();

    await wrapper.get('[data-testid="settings-tie-break-preserve-current"]').trigger("click");
    await wrapper.get('[data-testid="settings-save"]').trigger("click");
    await wrapper.vm.$nextTick();

    expect(received?.tieBreak).toBe("preserveCurrent");
  });

  it("sends the imported-pairs toggle on save", async () => {
    let received: SettingsDto | undefined;
    installMockIpc({
      get_app_version: "1.2.3",
      list_muted_notification_kinds: [],
      get_app_settings: appSettingsFixture,
      get_settings: settingsFixture,
      set_settings: (payload: unknown) => {
        received = (payload as { settings: SettingsDto }).settings;
        return settingsFixture;
      },
    });
    const pinia = createPinia();
    const wrapper = mountSettingsPage(pinia);
    await flush();
    await wrapper.vm.$nextTick();

    await wrapper.get('[data-testid="settings-use-imported-pairs"] input').setValue(true);
    await wrapper.get('[data-testid="settings-save"]').trigger("click");
    await wrapper.vm.$nextTick();

    expect(received?.useImportedPairs).toBe(true);
  });

  // -- the Network section: a separate app-global query/mutation, saved
  // immediately per toggle rather than riding along the profile
  // settings' own Save button --

  it("prefills the network toggles from get_app_settings and saves the allow-network toggle immediately", async () => {
    let received: AppSettingsDto | undefined;
    installMockIpc({
      get_app_version: "1.2.3",
      list_muted_notification_kinds: [],
      get_app_settings: appSettingsFixture,
      get_settings: settingsFixture,
      set_settings: settingsFixture,
      update_app_settings: (payload: unknown) => {
        received = (payload as { settings: AppSettingsDto }).settings;
        return { ...appSettingsFixture.settings, network: received.network };
      },
    });
    const pinia = createPinia();
    const wrapper = mountSettingsPage(pinia);
    await flush();
    await wrapper.vm.$nextTick();

    expect(wrapper.get('[data-testid="settings-allow-network"]').attributes("data-p-checked")).toBe(
      "true",
    );

    await wrapper.get('[data-testid="settings-allow-network"] input').setValue(false);
    await flush();

    expect(received?.network.allowNetwork).toBe(false);
    // No profile-settings save was triggered — this toggle bypasses the
    // page's own Save button entirely.
  });

  it("saves the auto-refresh-rule-databases toggle immediately", async () => {
    let received: AppSettingsDto | undefined;
    installMockIpc({
      get_app_version: "1.2.3",
      list_muted_notification_kinds: [],
      get_app_settings: appSettingsFixture,
      get_settings: settingsFixture,
      set_settings: settingsFixture,
      update_app_settings: (payload: unknown) => {
        received = (payload as { settings: AppSettingsDto }).settings;
        return { ...appSettingsFixture.settings, network: received.network };
      },
    });
    const pinia = createPinia();
    const wrapper = mountSettingsPage(pinia);
    await flush();
    await wrapper.vm.$nextTick();

    await wrapper.get('[data-testid="settings-auto-refresh-rule-databases"] input').setValue(false);
    await flush();

    expect(received?.network.autoRefreshRuleDatabases).toBe(false);
  });

  it("'Restore internet access defaults' calls reset_network_policy", async () => {
    let resetCalled = false;
    installMockIpc({
      get_app_version: "1.2.3",
      list_muted_notification_kinds: [],
      get_app_settings: appSettingsFixture,
      get_settings: settingsFixture,
      set_settings: settingsFixture,
      reset_network_policy: () => {
        resetCalled = true;
        return appSettingsFixture.settings;
      },
    });
    const pinia = createPinia();
    const wrapper = mountSettingsPage(pinia);
    await flush();
    await wrapper.vm.$nextTick();

    await wrapper.get('[data-testid="settings-restore-network-defaults"]').trigger("click");
    await flush();

    expect(resetCalled).toBe(true);
  });

  it("tells the user that restoring defaults includes the Steam Workshop database", async () => {
    installMockIpc({
      get_app_version: "1.2.3",
      list_muted_notification_kinds: [],
      get_app_settings: appSettingsFixture,
      get_settings: settingsFixture,
    });
    const wrapper = mountSettingsPage(createPinia());
    await flush();
    await wrapper.vm.$nextTick();

    const button = wrapper.get('[data-testid="settings-restore-network-defaults"]');
    const hint = wrapper.get(`#${button.attributes("aria-describedby")}`);
    expect(hint.text()).toContain("Steam Workshop database");
  });

  it("shows the recovered-settings warning when app-settings.json couldn't be read, and hides it otherwise", async () => {
    installMockIpc({
      get_app_version: "1.2.3",
      list_muted_notification_kinds: [],
      get_app_settings: {
        ...appSettingsFixture,
        loadStatus: "recovered",
      },
      get_settings: settingsFixture,
      set_settings: settingsFixture,
    });
    const pinia = createPinia();
    const wrapper = mountSettingsPage(pinia);
    await flush();
    await wrapper.vm.$nextTick();

    expect(wrapper.find('[data-testid="settings-app-settings-recovered-warning"]').exists()).toBe(
      true,
    );
  });

  it("does not show the recovered-settings warning for an ordinary loaded file", async () => {
    installMockIpc({
      get_app_version: "1.2.3",
      list_muted_notification_kinds: [],
      get_app_settings: appSettingsFixture,
      get_settings: settingsFixture,
      set_settings: settingsFixture,
    });
    const pinia = createPinia();
    const wrapper = mountSettingsPage(pinia);
    await flush();
    await wrapper.vm.$nextTick();

    expect(wrapper.find('[data-testid="settings-app-settings-recovered-warning"]').exists()).toBe(
      false,
    );
  });

  it("'Check now' is disabled while internet access is off", async () => {
    installMockIpc({
      get_app_version: "1.2.3",
      list_muted_notification_kinds: [],
      get_app_settings: {
        ...appSettingsFixture,
        settings: {
          ...appSettingsFixture.settings,
          network: { ...appSettingsFixture.settings.network, allowNetwork: false },
        },
      },
      get_settings: settingsFixture,
      set_settings: settingsFixture,
    });
    const pinia = createPinia();
    const wrapper = mountSettingsPage(pinia);
    await flush();
    await wrapper.vm.$nextTick();

    expect(
      wrapper.get('[data-testid="settings-check-for-update-now"]').attributes("disabled"),
    ).toBeDefined();
  });

  it("'Check now' calls check_for_update and renders its own result line", async () => {
    installMockIpc({
      get_app_version: "1.2.3",
      list_muted_notification_kinds: [],
      get_app_settings: appSettingsFixture,
      get_settings: settingsFixture,
      set_settings: settingsFixture,
      check_for_update: {
        kind: "ran",
        outcome: { kind: "updated", latestVersion: "0.2.0" },
      },
    });
    const pinia = createPinia();
    const wrapper = mountSettingsPage(pinia);
    await flush();
    await wrapper.vm.$nextTick();

    await wrapper.get('[data-testid="settings-check-for-update-now"]').trigger("click");
    await flush();
    await wrapper.vm.$nextTick();

    expect(wrapper.get('[data-testid="settings-check-for-update-result"]').text()).toContain(
      "0.2.0",
    );
  });

  it("the rate-limited result line renders", async () => {
    installMockIpc({
      get_app_version: "1.2.3",
      list_muted_notification_kinds: [],
      get_app_settings: appSettingsFixture,
      get_settings: settingsFixture,
      set_settings: settingsFixture,
      check_for_update: {
        kind: "skipped",
        reason: { rateLimitedUntil: { until: "2026-09-25T14:05:00Z" } },
      },
    });
    const pinia = createPinia();
    const wrapper = mountSettingsPage(pinia);
    await flush();
    await wrapper.vm.$nextTick();

    await wrapper.get('[data-testid="settings-check-for-update-now"]').trigger("click");
    await flush();
    await wrapper.vm.$nextTick();

    expect(wrapper.get('[data-testid="settings-check-for-update-result"]').text()).toContain(
      "GitHub's rate limit",
    );
  });

  it("lists muted kinds and unmute calls unmute_notification_kind with the exact kind", async () => {
    let unmuted: string | undefined;
    installMockIpc({
      get_app_version: "1.2.3",
      list_muted_notification_kinds: ["ruleDatabasesStale"],
      get_app_settings: appSettingsFixture,
      get_settings: settingsFixture,
      set_settings: settingsFixture,
      unmute_notification_kind: (payload: unknown) => {
        unmuted = (payload as { kind: string }).kind;
      },
    });
    const pinia = createPinia();
    const wrapper = mountSettingsPage(pinia);
    await flush();
    await wrapper.vm.$nextTick();

    expect(wrapper.find('[data-testid="settings-muted-kind-ruleDatabasesStale"]').exists()).toBe(
      true,
    );

    await wrapper.get('[data-testid="settings-unmute-ruleDatabasesStale"]').trigger("click");
    await flush();

    expect(unmuted).toBe("ruleDatabasesStale");
  });

  it("only the clicked row's Unmute button shows loading, not every row's", async () => {
    let resolveUnmute: (() => void) | undefined;
    installMockIpc({
      get_app_version: "1.2.3",
      list_muted_notification_kinds: ["ruleDatabasesStale", "importedRulesOutdated"],
      get_app_settings: appSettingsFixture,
      get_settings: settingsFixture,
      set_settings: settingsFixture,
      unmute_notification_kind: () =>
        new Promise<void>((resolve) => {
          resolveUnmute = resolve;
        }),
    });
    const pinia = createPinia();
    const wrapper = mountSettingsPage(pinia);
    await flush();
    await wrapper.vm.$nextTick();

    await wrapper.get('[data-testid="settings-unmute-ruleDatabasesStale"]').trigger("click");
    await wrapper.vm.$nextTick();

    // A `loading` PrimeVue `Button` also disables itself — the one
    // observable, version-stable signal from the DOM alone. The bug this
    // guards against shared one `isLoading` ref across every row, so
    // clicking one row's button would disable *both*.
    expect(
      wrapper.get('[data-testid="settings-unmute-ruleDatabasesStale"]').attributes("disabled"),
    ).toBeDefined();
    expect(
      wrapper.get('[data-testid="settings-unmute-importedRulesOutdated"]').attributes("disabled"),
    ).toBeUndefined();

    resolveUnmute?.();
    await flush();
  });

  it("shows the empty state when nothing is muted", async () => {
    installMockIpc({
      get_app_version: "1.2.3",
      list_muted_notification_kinds: [],
      get_app_settings: appSettingsFixture,
      get_settings: settingsFixture,
      set_settings: settingsFixture,
    });
    const pinia = createPinia();
    const wrapper = mountSettingsPage(pinia);
    await flush();
    await wrapper.vm.$nextTick();

    expect(wrapper.find('[data-testid="settings-muted-kinds-empty"]').exists()).toBe(true);
  });

  it("still prefills on a second mount that reuses an already-loaded query", async () => {
    // Regression case: `SettingsPage` syncs its form from the query's
    // `data` with a "sync once" watch — a first mount fetches normally,
    // but a *second* mount (a page revisit reusing the same Pinia Colada
    // cache, same `pinia`/`queryCache` instance here) sees `data` already
    // populated at setup time with no further change ever coming. A
    // non-immediate `watch(data, ...)` would never fire in that case and
    // leave the form stuck at its initial defaults.
    installMockIpc({
      get_app_version: "1.2.3",
      list_muted_notification_kinds: [],
      get_app_settings: appSettingsFixture,
      get_settings: settingsFixture,
      set_settings: settingsFixture,
    });
    const pinia = createPinia();
    const first = mountSettingsPage(pinia);
    await flush();
    await first.vm.$nextTick();
    first.unmount();

    const second = mountSettingsPage(pinia);
    await second.vm.$nextTick();

    expect(
      (second.get('[data-testid="settings-threshold"] input').element as HTMLInputElement).value,
    ).toBe("80");
  });

  it("no longer lists ModDependency or ParentTemplate as Awareness-strength examples", async () => {
    installMockIpc({
      get_app_version: "1.2.3",
      list_muted_notification_kinds: [],
      get_app_settings: appSettingsFixture,
      get_settings: settingsFixture,
      set_settings: settingsFixture,
    });
    const pinia = createPinia();
    const wrapper = mountSettingsPage(pinia);
    await flush();
    await wrapper.vm.$nextTick();

    // ModDependency is a Declared edge and ParentTemplate a Hard one (an
    // unresolvable ParentName drops the def at load time), so neither
    // belongs in this toggle's description.
    expect(wrapper.text()).not.toContain("ModDependency");
    expect(wrapper.text()).not.toContain("ParentTemplate");
    expect(wrapper.text()).toContain("UsesType");
  });

  it("disables Save while the threshold field is empty, and never calls set_settings from a click on it", async () => {
    let setSettingsCalled = false;
    installMockIpc({
      get_app_version: "1.2.3",
      list_muted_notification_kinds: [],
      get_app_settings: appSettingsFixture,
      get_settings: settingsFixture,
      set_settings: () => {
        setSettingsCalled = true;
        return settingsFixture;
      },
    });
    const pinia = createPinia();
    const wrapper = mountSettingsPage(pinia);
    // Deliberately not flushed: the threshold field is still `null`
    // (the query hasn't resolved yet), which is exactly the state
    // `submit` must refuse to save from.

    const saveButton = wrapper.get('[data-testid="settings-save"]');
    expect(saveButton.attributes("disabled")).toBeDefined();

    // A disabled native <button> doesn't dispatch "click" at all — this
    // is belt-and-suspenders on top of the `disabled` assertion above,
    // covering `submit`'s own `threshold === null` guard directly in
    // case anything (a keyboard activation, a future markup change)
    // ever bypasses the `disabled` attribute.
    await saveButton.trigger("click");
    await wrapper.vm.$nextTick();
    expect(setSettingsCalled).toBe(false);
  });

  // `Settings::default()` on the Rust side, mirrored here as a fixture —
  // `settingsFixture` above deliberately differs (`useImportedPairs:
  // false`) so a reset is observable.
  const defaultSettingsFixture: SettingsDto = {
    threshold: 80,
    enforceSoft: false,
    enforceAwareness: false,
    suggestMergeWhenClean: true,
    tieBreak: "rebuild",
    useImportedPairs: true,
    useImportedPlacements: true,
    enforceInferred: true,
    showDanglingDefReferences: false,
  };

  it("shows the dirty indicator once the form differs from the last-saved settings, not before", async () => {
    installMockIpc({
      get_app_version: "1.2.3",
      list_muted_notification_kinds: [],
      get_app_settings: appSettingsFixture,
      get_settings: settingsFixture,
      set_settings: settingsFixture,
    });
    const pinia = createPinia();
    const wrapper = mountSettingsPage(pinia);
    await flush();
    await wrapper.vm.$nextTick();

    expect(wrapper.find('[data-testid="settings-dirty-indicator"]').exists()).toBe(false);

    await wrapper.get('[data-testid="settings-use-imported-pairs"] input').setValue(true);
    await wrapper.vm.$nextTick();

    expect(wrapper.find('[data-testid="settings-dirty-indicator"]').exists()).toBe(true);
  });

  it("'Reset to defaults' confirms, then refills the form from get_default_settings without saving", async () => {
    let setSettingsCalled = false;
    installMockIpc({
      get_app_version: "1.2.3",
      list_muted_notification_kinds: [],
      get_app_settings: appSettingsFixture,
      get_settings: settingsFixture,
      get_default_settings: defaultSettingsFixture,
      set_settings: () => {
        setSettingsCalled = true;
        return settingsFixture;
      },
    });
    const pinia = createPinia();
    const wrapper = mountSettingsPage(pinia);
    await flush();
    await wrapper.vm.$nextTick();

    expect(
      wrapper.get('[data-testid="settings-use-imported-pairs"]').attributes("data-p-checked"),
    ).toBe("false");

    await wrapper.get('[data-testid="settings-reset-defaults-button"]').trigger("click");
    await wrapper.vm.$nextTick();
    expect(wrapper.find('[data-testid="settings-reset-confirm-dialog"]').exists()).toBe(true);

    await wrapper.get('[data-testid="settings-reset-confirm-submit"]').trigger("click");
    await flush();
    await wrapper.vm.$nextTick();

    // Refilled from the defaults fixture, not saved: the dirty indicator
    // shows the pending change, and `set_settings` was never called.
    expect(
      wrapper.get('[data-testid="settings-use-imported-pairs"]').attributes("data-p-checked"),
    ).toBe("true");
    expect(wrapper.find('[data-testid="settings-dirty-indicator"]').exists()).toBe(true);
    expect(setSettingsCalled).toBe(false);
  });

  it("cancelling the reset-confirm dialog leaves the form untouched", async () => {
    installMockIpc({
      get_app_version: "1.2.3",
      list_muted_notification_kinds: [],
      get_app_settings: appSettingsFixture,
      get_settings: settingsFixture,
      get_default_settings: defaultSettingsFixture,
      set_settings: settingsFixture,
    });
    const pinia = createPinia();
    const wrapper = mountSettingsPage(pinia);
    await flush();
    await wrapper.vm.$nextTick();

    await wrapper.get('[data-testid="settings-reset-defaults-button"]').trigger("click");
    await wrapper.get('[data-testid="settings-reset-confirm-cancel"]').trigger("click");
    await wrapper.vm.$nextTick();

    expect(
      wrapper.get('[data-testid="settings-use-imported-pairs"]').attributes("data-p-checked"),
    ).toBe("false");
    expect(wrapper.find('[data-testid="settings-reset-confirm-dialog"]').exists()).toBe(false);
  });
});
