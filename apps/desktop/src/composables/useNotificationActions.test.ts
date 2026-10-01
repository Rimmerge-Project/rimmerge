import { PiniaColada } from "@pinia/colada";
import { clearMocks } from "@tauri-apps/api/mocks";
import { mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import { afterEach, describe, expect, it } from "vitest";
import { defineComponent } from "vue";
import { createMemoryHistory, createRouter } from "vue-router";

import { useNotificationActions } from "@/composables/useNotificationActions";
import { installMockIpc } from "@/services/ipc.mock";
import type { AppSettingsDto } from "@/types/generated/AppSettingsDto";
import type { AppSettingsResponseDto } from "@/types/generated/AppSettingsResponseDto";
import type { NetworkPolicyDto } from "@/types/generated/NetworkPolicyDto";
import type { NotificationDto } from "@/types/generated/NotificationDto";

const NETWORK: NetworkPolicyDto = {
  allowNetwork: true,
  checkForUpdates: true,
  autoRefreshRuleDatabases: true,
  fetchCommunityRules: true,
  fetchSteamWorkshop: true,
  fetchRimmergeRules: true,
};

const LOADED_APP_SETTINGS: AppSettingsResponseDto = {
  settings: { network: NETWORK, reminders: { ruleDatabasesStaleAfterDays: 30 } },
  loadStatus: "loaded",
};

const RECOMMENDED_SOURCES_NOTIFICATION: NotificationDto = {
  key: { kind: "recommendedSourcesIncomplete", fingerprint: "steam_workshop@not_downloaded" },
  severity: "info",
  actions: ["enableRecommendedSources", "openRuleDatabases"],
  dismissal: "occurrenceOrMute",
  data: { kind: "recommendedSourcesIncomplete", sources: { steam: "notDownloaded" } },
};

const STALE_NOTIFICATION: NotificationDto = {
  key: { kind: "ruleDatabasesStale", fingerprint: "community_rules@never;rimmerge_rules@never" },
  severity: "info",
  actions: ["refreshRuleDatabases", "openSettings"],
  dismissal: "occurrenceOrMute",
  data: {
    kind: "ruleDatabasesStale",
    sources: {
      rimmerge: { freshness: { kind: "neverFetched" }, refresh: "automatic", lastFailure: null },
      community: { freshness: { kind: "neverFetched" }, refresh: "automatic", lastFailure: null },
    },
  },
};

const WELCOME_NOTIFICATION: NotificationDto = {
  key: { kind: "welcome", fingerprint: "welcome" },
  severity: "info",
  actions: ["keepNetworkSettings", "turnOffNetwork", "applyRecommendedSettings"],
  dismissal: "occurrence",
  data: { kind: "welcome", network: NETWORK, settingsMatchRecommended: true },
};

/** Mounts `useNotificationActions` inside a host component, mirroring `useMergeChoices.test.ts`'s own harness. */
function mountHarness() {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: "/", name: "dashboard", component: { template: "<div />" } },
      { path: "/rules", name: "rules", component: { template: "<div />" } },
      { path: "/settings", name: "settings", component: { template: "<div />" } },
      { path: "/patches", name: "patches", component: { template: "<div />" } },
    ],
  });

  let composable!: ReturnType<typeof useNotificationActions>;
  const Harness = defineComponent({
    setup() {
      composable = useNotificationActions();
      return {};
    },
    template: "<div />",
  });

  const pinia = createPinia();
  setActivePinia(pinia);
  const wrapper = mount(Harness, { global: { plugins: [pinia, PiniaColada, router] } });

  return {
    wrapper,
    get actions() {
      return composable;
    },
  };
}

async function flush(): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, 0));
}

describe("useNotificationActions", () => {
  afterEach(() => {
    clearMocks();
  });

  it("'turnOffNetwork' throws and never completes Welcome when app settings never load", async () => {
    const completeWelcomeCalls: number[] = [];
    installMockIpc({
      get_app_settings: () => {
        throw { code: "internal", message: "app-settings.json could not be read" };
      },
      update_app_settings: () => {
        throw new Error("must not be called: settings never loaded");
      },
      complete_welcome: () => {
        completeWelcomeCalls.push(1);
      },
    });
    const { actions, wrapper } = mountHarness();
    await flush();

    await expect(actions.runAction("turnOffNetwork", WELCOME_NOTIFICATION)).rejects.toThrow();

    expect(completeWelcomeCalls).toHaveLength(0);
    wrapper.unmount();
  });

  it("'applyRecommendedSettings' completes Welcome only after the reset succeeds", async () => {
    const calls: string[] = [];
    installMockIpc({
      get_app_settings: (): AppSettingsResponseDto => LOADED_APP_SETTINGS,
      reset_settings: () => {
        calls.push("reset_settings");
      },
      complete_welcome: () => {
        calls.push("complete_welcome");
      },
    });
    const { actions, wrapper } = mountHarness();
    await flush();

    await actions.runAction("applyRecommendedSettings", WELCOME_NOTIFICATION);

    expect(calls).toEqual(["reset_settings", "complete_welcome"]);
    wrapper.unmount();
  });

  it("'applyRecommendedSettings' never completes Welcome when the reset fails", async () => {
    const calls: string[] = [];
    installMockIpc({
      get_app_settings: (): AppSettingsResponseDto => LOADED_APP_SETTINGS,
      reset_settings: () => {
        throw { code: "internal", message: "save failed" };
      },
      complete_welcome: () => {
        calls.push("complete_welcome");
      },
    });
    const { actions, wrapper } = mountHarness();
    await flush();

    await expect(
      actions.runAction("applyRecommendedSettings", WELCOME_NOTIFICATION),
    ).rejects.toThrow();

    expect(calls).toHaveLength(0);
    wrapper.unmount();
  });

  it("'turnOffNetwork' saves allowNetwork false built from the loaded settings, then completes Welcome", async () => {
    const calls: string[] = [];
    let savedNetwork: NetworkPolicyDto | null = null;
    installMockIpc({
      get_app_settings: (): AppSettingsResponseDto => LOADED_APP_SETTINGS,
      update_app_settings: (payload: unknown) => {
        savedNetwork = (payload as { settings: AppSettingsDto }).settings.network;
        calls.push("update_app_settings");
        return (payload as { settings: AppSettingsDto }).settings;
      },
      complete_welcome: () => {
        calls.push("complete_welcome");
      },
    });
    const { actions, wrapper } = mountHarness();
    await flush();

    await actions.runAction("turnOffNetwork", WELCOME_NOTIFICATION);

    expect(calls).toEqual(["update_app_settings", "complete_welcome"]);
    expect(savedNetwork).toEqual({ ...NETWORK, allowNetwork: false });
    wrapper.unmount();
  });

  it("'enableRecommendedSources' turns the sources on in Rust, then runs the manual refresh, in that order", async () => {
    const calls: string[] = [];
    installMockIpc({
      get_app_settings: (): AppSettingsResponseDto => LOADED_APP_SETTINGS,
      enable_recommended_rule_databases: () => {
        calls.push("enable_recommended_rule_databases");
        return LOADED_APP_SETTINGS.settings;
      },
      refresh_rule_databases: () => {
        calls.push("refresh_rule_databases");
        return [{ database: "steam", outcome: { kind: "updated", sha256: "abc", bytes: 1 } }];
      },
    });
    const { actions, wrapper } = mountHarness();
    await flush();

    await actions.runAction("enableRecommendedSources", RECOMMENDED_SOURCES_NOTIFICATION);

    expect(calls).toEqual(["enable_recommended_rule_databases", "refresh_rule_databases"]);
    wrapper.unmount();
  });

  it("'refreshRuleDatabases' refreshes exactly the sources the stale notice lists, sorted", async () => {
    const payloads: unknown[] = [];
    installMockIpc({
      get_app_settings: (): AppSettingsResponseDto => LOADED_APP_SETTINGS,
      refresh_rule_databases: (payload: unknown) => {
        payloads.push(payload);
        return [];
      },
    });
    const { actions, wrapper } = mountHarness();
    await flush();

    await actions.runAction("refreshRuleDatabases", STALE_NOTIFICATION);

    expect(payloads).toEqual([{ request: { sources: ["community", "rimmerge"] } }]);
    wrapper.unmount();
  });

  it("'refreshRuleDatabases' on a notice that is not the stale one refreshes nothing", async () => {
    const payloads: unknown[] = [];
    installMockIpc({
      get_app_settings: (): AppSettingsResponseDto => LOADED_APP_SETTINGS,
      refresh_rule_databases: (payload: unknown) => {
        payloads.push(payload);
        return [];
      },
    });
    const { actions, wrapper } = mountHarness();
    await flush();

    await actions.runAction("refreshRuleDatabases", RECOMMENDED_SOURCES_NOTIFICATION);

    expect(payloads).toHaveLength(0);
    wrapper.unmount();
  });

  it("'enableRecommendedSources' never refreshes when turning the sources on fails", async () => {
    const calls: string[] = [];
    installMockIpc({
      get_app_settings: (): AppSettingsResponseDto => LOADED_APP_SETTINGS,
      enable_recommended_rule_databases: () => {
        throw { code: "internal", message: "save failed" };
      },
      refresh_rule_databases: () => {
        calls.push("refresh_rule_databases");
        return [];
      },
    });
    const { actions, wrapper } = mountHarness();
    await flush();

    await expect(
      actions.runAction("enableRecommendedSources", RECOMMENDED_SOURCES_NOTIFICATION),
    ).rejects.toThrow();

    expect(calls).toHaveLength(0);
    wrapper.unmount();
  });
});
