import { PiniaColada } from "@pinia/colada";
import Aura from "@primevue/themes/aura";
import { clearMocks } from "@tauri-apps/api/mocks";
import { mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import PrimeVue from "primevue/config";
import { afterEach, describe, expect, it } from "vitest";

import NotificationBell from "@/components/notifications/NotificationBell.vue";
import { installMockIpc } from "@/services/ipc.mock";
import type { NotificationDto } from "@/types/generated/NotificationDto";

function welcomeNotification(): NotificationDto {
  return {
    key: { kind: "welcome", fingerprint: "welcome" },
    severity: "info",
    actions: ["keepNetworkSettings", "turnOffNetwork", "applyRecommendedSettings"],
    dismissal: "occurrence",
    data: {
      kind: "welcome",
      network: {
        allowNetwork: true,
        checkForUpdates: true,
        autoRefreshRuleDatabases: true,
        fetchCommunityRules: true,
        fetchSteamWorkshop: true,
        fetchRimmergeRules: true,
      },
      settingsMatchRecommended: true,
    },
  };
}

function mountBell() {
  return mount(NotificationBell, {
    global: {
      plugins: [createPinia(), PiniaColada, [PrimeVue, { theme: { preset: Aura } }]],
    },
  });
}

async function flush(): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, 0));
}

describe("NotificationBell", () => {
  afterEach(() => {
    clearMocks();
  });

  it("the accessible name says 'none' when there are no active notices", async () => {
    installMockIpc({ list_notifications: [] });
    const wrapper = mountBell();
    await flush();
    await wrapper.vm.$nextTick();

    expect(wrapper.get('[data-testid="notification-bell"]').attributes("aria-label")).toBe(
      "Notifications, none",
    );
  });

  it("the accessible name includes the count", async () => {
    installMockIpc({ list_notifications: [welcomeNotification()] });
    const wrapper = mountBell();
    await flush();
    await wrapper.vm.$nextTick();

    expect(wrapper.get('[data-testid="notification-bell"]').attributes("aria-label")).toBe(
      "Notifications, 1",
    );
  });
});
