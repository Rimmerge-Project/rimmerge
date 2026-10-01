import { PiniaColada } from "@pinia/colada";
import Aura from "@primevue/themes/aura";
import { clearMocks } from "@tauri-apps/api/mocks";
import { mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import PrimeVue from "primevue/config";
import { afterEach, describe, expect, it } from "vitest";
import { createMemoryHistory, createRouter } from "vue-router";

import WelcomeCard from "@/components/notifications/WelcomeCard.vue";
import { installMockIpc } from "@/services/ipc.mock";
import type { AppSettingsDto } from "@/types/generated/AppSettingsDto";
import type { AppSettingsResponseDto } from "@/types/generated/AppSettingsResponseDto";
import type { NotificationDto } from "@/types/generated/NotificationDto";

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
    reminders: { ruleDatabasesStaleAfterDays: 30 },
  },
  loadStatus: "loaded",
};

function welcomeNotification(settingsMatchRecommended: boolean): NotificationDto {
  return {
    key: { kind: "welcome", fingerprint: "welcome" },
    severity: "info",
    actions: ["keepNetworkSettings", "turnOffNetwork", "applyRecommendedSettings"],
    dismissal: "occurrence",
    data: {
      kind: "welcome",
      network: appSettingsFixture.settings.network,
      settingsMatchRecommended,
    },
  };
}

function mountCard() {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [{ path: "/", name: "dashboard", component: { template: "<div />" } }],
  });
  return mount(WelcomeCard, {
    global: {
      plugins: [createPinia(), router, PiniaColada, [PrimeVue, { theme: { preset: Aura } }]],
    },
  });
}

async function flush(): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, 0));
}

describe("WelcomeCard", () => {
  afterEach(() => {
    clearMocks();
  });

  it("does not render while there is no active Welcome notice", async () => {
    installMockIpc({ list_notifications: [], get_app_settings: appSettingsFixture });
    const wrapper = mountCard();
    await flush();
    await wrapper.vm.$nextTick();

    expect(wrapper.find('[data-testid="welcome-card"]').exists()).toBe(false);
  });

  it("every switch is on by default", async () => {
    installMockIpc({
      list_notifications: [welcomeNotification(true)],
      get_app_settings: appSettingsFixture,
    });
    const wrapper = mountCard();
    await flush();
    await wrapper.vm.$nextTick();

    expect(
      (wrapper.get('[data-testid="welcome-card-allow-network"] input').element as HTMLInputElement)
        .checked,
    ).toBe(true);
    expect(
      (
        wrapper.get('[data-testid="welcome-card-check-for-updates"] input')
          .element as HTMLInputElement
      ).checked,
    ).toBe(true);
    expect(
      (wrapper.get('[data-testid="welcome-card-auto-refresh"] input').element as HTMLInputElement)
        .checked,
    ).toBe(true);
  });

  it("each switch reflects its own field, not a shared default", async () => {
    // A mixed fixture — unlike the all-true one above — is what actually
    // proves each `ToggleSwitch` is wired to the field its own label
    // names: an all-true fixture would pass even if every switch read
    // the same (or the wrong) field.
    const mixedNotification = welcomeNotification(true);
    if (mixedNotification.data.kind !== "welcome") {
      throw new Error("expected a welcome notification");
    }
    mixedNotification.data.network = {
      ...mixedNotification.data.network,
      allowNetwork: true,
      checkForUpdates: false,
      autoRefreshRuleDatabases: true,
    };
    installMockIpc({
      list_notifications: [mixedNotification],
      get_app_settings: appSettingsFixture,
    });
    const wrapper = mountCard();
    await flush();
    await wrapper.vm.$nextTick();

    expect(
      (wrapper.get('[data-testid="welcome-card-allow-network"] input').element as HTMLInputElement)
        .checked,
    ).toBe(true);
    expect(
      (
        wrapper.get('[data-testid="welcome-card-check-for-updates"] input')
          .element as HTMLInputElement
      ).checked,
    ).toBe(false);
    expect(
      (wrapper.get('[data-testid="welcome-card-auto-refresh"] input').element as HTMLInputElement)
        .checked,
    ).toBe(true);
  });

  it("'Use recommended settings' is hidden when settings already match", async () => {
    installMockIpc({
      list_notifications: [welcomeNotification(true)],
      get_app_settings: appSettingsFixture,
    });
    const wrapper = mountCard();
    await flush();
    await wrapper.vm.$nextTick();

    expect(wrapper.find('[data-testid="welcome-card-apply-recommended"]').exists()).toBe(false);
    expect(wrapper.find('[data-testid="welcome-card-settings-already-recommended"]').exists()).toBe(
      true,
    );
  });

  it("'Use recommended settings' shows when settings differ from the defaults", async () => {
    installMockIpc({
      list_notifications: [welcomeNotification(false)],
      get_app_settings: appSettingsFixture,
    });
    const wrapper = mountCard();
    await flush();
    await wrapper.vm.$nextTick();

    expect(wrapper.find('[data-testid="welcome-card-apply-recommended"]').exists()).toBe(true);
  });

  it("flips two switches in a row, each save built from the live app settings (reminders included), never a stale snapshot", async () => {
    // A stateful `get_app_settings` (unlike the module-level
    // `appSettingsFixture`) is the point: it proves the second toggle's
    // payload is built from what the *first* save just persisted, not
    // from `welcome.data.network`'s own one-time snapshot — the exact
    // bug this test guards against (see `WelcomeCard.vue`'s
    // `toggleNetwork`).
    let liveSettings: AppSettingsDto = appSettingsFixture.settings;
    const calls: AppSettingsDto[] = [];
    installMockIpc({
      list_notifications: [welcomeNotification(true)],
      get_app_settings: (): AppSettingsResponseDto => ({
        settings: liveSettings,
        loadStatus: "loaded",
      }),
      update_app_settings: (payload: unknown) => {
        liveSettings = (payload as { settings: AppSettingsDto }).settings;
        calls.push(liveSettings);
        return liveSettings;
      },
    });
    const wrapper = mountCard();
    await flush();
    await wrapper.vm.$nextTick();

    await wrapper.get('[data-testid="welcome-card-check-for-updates"] input').setValue(false);
    await flush();
    await wrapper.vm.$nextTick();

    await wrapper.get('[data-testid="welcome-card-auto-refresh"] input').setValue(false);
    await flush();
    await wrapper.vm.$nextTick();

    expect(calls).toEqual([
      {
        network: { ...appSettingsFixture.settings.network, checkForUpdates: false },
        reminders: appSettingsFixture.settings.reminders,
      },
      {
        network: {
          ...appSettingsFixture.settings.network,
          checkForUpdates: false,
          autoRefreshRuleDatabases: false,
        },
        reminders: appSettingsFixture.settings.reminders,
      },
    ]);
  });

  it("'Turn off internet access' calls update_app_settings with allowNetwork false, then complete_welcome, in that order", async () => {
    const calls: string[] = [];
    installMockIpc({
      list_notifications: [welcomeNotification(true)],
      get_app_settings: appSettingsFixture,
      update_app_settings: (payload: unknown) => {
        calls.push(
          `update_app_settings:${(payload as { settings: AppSettingsDto }).settings.network.allowNetwork}`,
        );
        return (payload as { settings: AppSettingsDto }).settings;
      },
      complete_welcome: () => {
        calls.push("complete_welcome");
      },
    });
    const wrapper = mountCard();
    await flush();
    await wrapper.vm.$nextTick();

    await wrapper.get('[data-testid="welcome-card-turn-off"]').trigger("click");
    await flush();

    expect(calls).toEqual(["update_app_settings:false", "complete_welcome"]);
  });

  it("shows the Steam Workshop switch with its size, reflecting its own field", async () => {
    const notification = welcomeNotification(true);
    if (notification.data.kind !== "welcome") {
      throw new Error("expected a welcome notification");
    }
    notification.data.network = { ...notification.data.network, fetchSteamWorkshop: false };
    installMockIpc({ list_notifications: [notification], get_app_settings: appSettingsFixture });
    const wrapper = mountCard();
    await flush();
    await wrapper.vm.$nextTick();

    expect(wrapper.get('[data-testid="welcome-card"]').text()).toContain(
      "Steam Workshop database (about 49 MB, downloaded only when you click Refresh)",
    );
    expect(
      (wrapper.get('[data-testid="welcome-card-steam-workshop"] input').element as HTMLInputElement)
        .checked,
    ).toBe(false);
  });

  it("toggling the Steam Workshop switch saves fetchSteamWorkshop on top of the live settings and answers nothing", async () => {
    const calls: AppSettingsDto[] = [];
    const answered: string[] = [];
    installMockIpc({
      list_notifications: [welcomeNotification(true)],
      get_app_settings: appSettingsFixture,
      update_app_settings: (payload: unknown) => {
        calls.push((payload as { settings: AppSettingsDto }).settings);
        return (payload as { settings: AppSettingsDto }).settings;
      },
      complete_welcome: () => {
        answered.push("complete_welcome");
      },
      refresh_rule_databases: () => {
        answered.push("refresh_rule_databases");
        return [];
      },
    });
    const wrapper = mountCard();
    await flush();
    await wrapper.vm.$nextTick();

    await wrapper.get('[data-testid="welcome-card-steam-workshop"] input').setValue(false);
    await flush();

    expect(calls).toEqual([
      {
        network: { ...appSettingsFixture.settings.network, fetchSteamWorkshop: false },
        reminders: appSettingsFixture.settings.reminders,
      },
    ]);
    expect(answered).toEqual([]);
  });

  it("locks the answer buttons and the switches while a switch's save is in flight, then frees them", async () => {
    let finishSave: () => void = () => {};
    const answered: string[] = [];
    installMockIpc({
      list_notifications: [welcomeNotification(true)],
      get_app_settings: appSettingsFixture,
      update_app_settings: (payload: unknown) =>
        new Promise((resolve) => {
          finishSave = () => resolve((payload as { settings: AppSettingsDto }).settings);
        }),
      complete_welcome: () => {
        answered.push("complete_welcome");
      },
    });
    const wrapper = mountCard();
    await flush();
    await wrapper.vm.$nextTick();

    await wrapper.get('[data-testid="welcome-card-check-for-updates"] input').setValue(false);
    await flush();
    await wrapper.vm.$nextTick();

    const keep = wrapper.get('[data-testid="welcome-card-keep"]');
    expect(keep.attributes("disabled")).toBeDefined();
    expect(
      wrapper.get('[data-testid="welcome-card-turn-off"]').attributes("disabled"),
    ).toBeDefined();
    expect(
      wrapper.get('[data-testid="welcome-card-auto-refresh"] input').attributes("disabled"),
    ).toBeDefined();
    await keep.trigger("click");
    await flush();
    expect(answered).toEqual([]);

    finishSave();
    await flush();
    await wrapper.vm.$nextTick();

    expect(wrapper.get('[data-testid="welcome-card-keep"]').attributes("disabled")).toBeUndefined();
    await wrapper.get('[data-testid="welcome-card-keep"]').trigger("click");
    await flush();
    expect(answered).toEqual(["complete_welcome"]);
  });

  it("shows a failed switch save on the card instead of dropping it", async () => {
    installMockIpc({
      list_notifications: [welcomeNotification(true)],
      get_app_settings: appSettingsFixture,
      update_app_settings: () => {
        throw { code: "internal", message: "disk full" };
      },
    });
    const wrapper = mountCard();
    await flush();
    await wrapper.vm.$nextTick();

    await wrapper.get('[data-testid="welcome-card-check-for-updates"] input').setValue(false);
    await flush();
    await wrapper.vm.$nextTick();

    expect(wrapper.find('[data-testid="welcome-card-save-error"]').exists()).toBe(true);
  });
});
