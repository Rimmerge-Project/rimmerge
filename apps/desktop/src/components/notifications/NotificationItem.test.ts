import { PiniaColada } from "@pinia/colada";
import Aura from "@primevue/themes/aura";
import { clearMocks } from "@tauri-apps/api/mocks";
import { mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import PrimeVue from "primevue/config";
import { afterEach, describe, expect, it } from "vitest";
import { createMemoryHistory, createRouter } from "vue-router";

import NotificationItem from "@/components/notifications/NotificationItem.vue";
import { installMockIpc } from "@/services/ipc.mock";
import type { AppSettingsResponseDto } from "@/types/generated/AppSettingsResponseDto";
import type { NotificationDto } from "@/types/generated/NotificationDto";
import type { NotificationKeyDto } from "@/types/generated/NotificationKeyDto";
import type { NotificationKindDto } from "@/types/generated/NotificationKindDto";

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

const importedRulesOutdated: NotificationDto = {
  key: { kind: "importedRulesOutdated", fingerprint: "community@abc" },
  severity: "info",
  actions: ["openRuleDatabases"],
  dismissal: "occurrenceOrMute",
  data: { kind: "importedRulesOutdated", sources: { community: "abc123456789" } },
};

const updateAvailable: NotificationDto = {
  key: { kind: "updateAvailable", fingerprint: "0.2.0" },
  severity: "info",
  actions: ["showReleasePage", "openSettings"],
  dismissal: "occurrence",
  data: {
    kind: "updateAvailable",
    running: "0.1.0",
    latestVersion: "0.2.0",
    latestPublishedAt: "2026-01-01T00:00:00Z",
  },
};

const recommendedSourcesIncomplete: NotificationDto = {
  key: {
    kind: "recommendedSourcesIncomplete",
    fingerprint: "community_rules@off;steam_workshop@not_downloaded",
  },
  severity: "info",
  actions: ["enableRecommendedSources", "openRuleDatabases"],
  dismissal: "occurrenceOrMute",
  data: {
    kind: "recommendedSourcesIncomplete",
    sources: { community: "off", steam: "notDownloaded" },
  },
};

function mountItem(notification: NotificationDto) {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: "/", name: "dashboard", component: { template: "<div />" } },
      { path: "/settings", name: "settings", component: { template: "<div />" } },
      { path: "/rules", name: "rules", component: { template: "<div />" } },
      { path: "/patches", name: "patches", component: { template: "<div />" } },
    ],
  });
  return mount(NotificationItem, {
    props: { notification },
    global: {
      plugins: [createPinia(), router, PiniaColada, [PrimeVue, { theme: { preset: Aura } }]],
    },
  });
}

async function flush(): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, 0));
}

describe("NotificationItem", () => {
  afterEach(() => {
    clearMocks();
  });

  it("dismiss calls dismiss_notification with the exact key", async () => {
    const calls: NotificationKeyDto[] = [];
    installMockIpc({
      dismiss_notification: (payload: unknown) => {
        calls.push((payload as { key: NotificationKeyDto }).key);
      },
      get_app_settings: appSettingsFixture,
    });
    const wrapper = mountItem(importedRulesOutdated);

    await wrapper
      .get('[data-testid="notification-dismiss-importedRulesOutdated"]')
      .trigger("click");
    await flush();

    expect(calls).toEqual([importedRulesOutdated.key]);
  });

  it("mute calls mute_notification_kind with the exact kind, only offered when dismissal allows it", async () => {
    const calls: NotificationKindDto[] = [];
    installMockIpc({
      mute_notification_kind: (payload: unknown) => {
        calls.push((payload as { kind: NotificationKindDto }).kind);
      },
      get_app_settings: appSettingsFixture,
    });
    const wrapper = mountItem(importedRulesOutdated);

    await wrapper.get('[data-testid="notification-mute-importedRulesOutdated"]').trigger("click");
    await flush();

    expect(calls).toEqual(["importedRulesOutdated"]);
  });

  it("never offers mute for a dismiss-only notice", () => {
    installMockIpc({ get_app_settings: appSettingsFixture });
    const wrapper = mountItem(updateAvailable);

    expect(wrapper.find('[data-testid="notification-mute-updateAvailable"]').exists()).toBe(false);
  });

  it("shows a stale source's failure headline and its technical detail on separate lines", () => {
    installMockIpc({ get_app_settings: appSettingsFixture });
    const wrapper = mountItem({
      key: { kind: "ruleDatabasesStale", fingerprint: "community" },
      severity: "info",
      actions: ["openRuleDatabases"],
      dismissal: "occurrenceOrMute",
      data: {
        kind: "ruleDatabasesStale",
        sources: {
          community: {
            freshness: { kind: "neverFetched" },
            refresh: "manual",
            lastFailure: { cause: { kind: "transport" }, detail: "connection timed out" },
          },
        },
      },
    });

    const item = wrapper.get('[data-testid="notification-item-ruleDatabasesStale"]').text();
    expect(item).toContain("Last attempt failed: Couldn't reach GitHub");
    expect(item).not.toContain("(connection timed out)");
    expect(wrapper.get('[data-testid="notification-failure-detail-community"]').text()).toBe(
      "Technical details: connection timed out",
    );
  });

  it("renders the update-available title with the latest version", () => {
    installMockIpc({ get_app_settings: appSettingsFixture });
    const wrapper = mountItem(updateAvailable);

    expect(wrapper.get('[data-testid="notification-item-updateAvailable"]').text()).toContain(
      "0.2.0",
    );
  });

  it("lists each recommended source that is not set up with its own state, and states the size when Steam is in the set", () => {
    installMockIpc({ get_app_settings: appSettingsFixture });
    const wrapper = mountItem(recommendedSourcesIncomplete);

    const lines = wrapper
      .get('[data-testid="notification-sources-recommendedSourcesIncomplete"]')
      .findAll("li")
      .map((line) => line.text());
    expect(lines).toEqual(["Community rules: off", "Steam Workshop: on, not downloaded yet"]);
    const item = wrapper.get('[data-testid="notification-item-recommendedSourcesIncomplete"]');
    expect(item.text()).toContain("about 49 MB");
    expect(
      item
        .get(
          '[data-testid="notification-action-recommendedSourcesIncomplete-enableRecommendedSources"]',
        )
        .text(),
    ).toBe("Turn on and download");
  });

  it("omits the size sentence when Steam is not in the set, and offers mute", () => {
    installMockIpc({ get_app_settings: appSettingsFixture });
    const wrapper = mountItem({
      ...recommendedSourcesIncomplete,
      data: { kind: "recommendedSourcesIncomplete", sources: { rimmerge: "off" } },
    });

    expect(wrapper.text()).not.toContain("49 MB");
    expect(
      wrapper.find('[data-testid="notification-mute-recommendedSourcesIncomplete"]').exists(),
    ).toBe(true);
  });
});
