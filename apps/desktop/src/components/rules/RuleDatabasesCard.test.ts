import { PiniaColada } from "@pinia/colada";
import Aura from "@primevue/themes/aura";
import { clearMocks } from "@tauri-apps/api/mocks";
import { mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import PrimeVue from "primevue/config";
import { afterEach, describe, expect, it } from "vitest";

import RuleDatabasesCard from "@/components/rules/RuleDatabasesCard.vue";
import { installMockIpc } from "@/services/ipc.mock";
import type { AppSettingsDto } from "@/types/generated/AppSettingsDto";
import type { AppSettingsResponseDto } from "@/types/generated/AppSettingsResponseDto";
import type { RuleDatabaseRefreshResultDto } from "@/types/generated/RuleDatabaseRefreshResultDto";
import type { RuleDatabaseViewDto } from "@/types/generated/RuleDatabaseViewDto";

const appSettingsFixture: AppSettingsResponseDto = {
  settings: {
    network: {
      allowNetwork: true,
      checkForUpdates: true,
      autoRefreshRuleDatabases: true,
      fetchCommunityRules: true,
      fetchSteamWorkshop: false,
      fetchRimmergeRules: true,
    },
    reminders: {
      ruleDatabasesStaleAfterDays: 30,
    },
  },
  loadStatus: "loaded",
};

function view(overrides: Partial<RuleDatabaseViewDto> = {}): RuleDatabaseViewDto {
  return {
    database: "community",
    enabled: true,
    cached: null,
    isStale: false,
    lastFailure: null,
    importedSha256: null,
    needsReimport: false,
    bundledSha256: null,
    ...overrides,
  };
}

function mountCard() {
  return mount(RuleDatabasesCard, {
    global: {
      plugins: [createPinia(), PiniaColada, [PrimeVue, { theme: { preset: Aura } }]],
    },
  });
}

async function flush(): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, 0));
}

describe("RuleDatabasesCard", () => {
  afterEach(() => {
    clearMocks();
  });

  // -- the four status states --

  it("renders 'never fetched' for a database with no cache", async () => {
    installMockIpc({
      get_rule_databases: [view({ database: "steam", enabled: false, cached: null })],
      get_app_settings: appSettingsFixture,
    });
    const wrapper = mountCard();
    await flush();
    await wrapper.vm.$nextTick();

    expect(wrapper.get('[data-testid="rule-database-status-steam"]').text()).toBe("Never fetched");
  });

  it("renders the embedded snapshot's sha next to the rimmerge row, never on the others", async () => {
    installMockIpc({
      get_rule_databases: [
        view({ database: "rimmerge", cached: null, bundledSha256: "abc123def456abc123def456" }),
        view({ database: "steam", cached: null, bundledSha256: null }),
      ],
      get_app_settings: appSettingsFixture,
    });
    const wrapper = mountCard();
    await flush();
    await wrapper.vm.$nextTick();

    expect(wrapper.get('[data-testid="rule-database-status-rimmerge"]').text()).toBe(
      "Never fetched · Bundled abc123def456",
    );
    expect(wrapper.get('[data-testid="rule-database-status-steam"]').text()).toBe("Never fetched");
  });

  it("renders the up-to-date status line with relative time, size, and sha12", async () => {
    installMockIpc({
      get_rule_databases: [
        view({
          cached: {
            sha256: "b00fa152965643381ecedcf9043fc0d46b272cb9",
            bytes: 393_897,
            fetchedAt: "2026-08-01T00:00:00Z",
          },
          isStale: false,
        }),
      ],
      get_app_settings: appSettingsFixture,
    });
    const wrapper = mountCard();
    await flush();
    await wrapper.vm.$nextTick();

    const text = wrapper.get('[data-testid="rule-database-status-community"]').text();
    expect(text).toContain("Updated");
    expect(text).toContain("KB");
    expect(text).toContain("b00fa1529656");
  });

  it("renders the stale status line", async () => {
    installMockIpc({
      get_rule_databases: [
        view({
          cached: { sha256: "abc123", bytes: 10, fetchedAt: "2026-01-01T00:00:00Z" },
          isStale: true,
        }),
      ],
      get_app_settings: appSettingsFixture,
    });
    const wrapper = mountCard();
    await flush();
    await wrapper.vm.$nextTick();

    expect(wrapper.get('[data-testid="rule-database-status-community"]').text()).toContain("Stale");
  });

  it("renders the last-refresh-failed status line as a suffix and shows the still-in-use banner", async () => {
    installMockIpc({
      get_rule_databases: [
        view({
          cached: { sha256: "abc123", bytes: 10, fetchedAt: "2026-08-01T00:00:00Z" },
          lastFailure: { cause: { kind: "transport" }, detail: "connection timed out" },
        }),
      ],
      get_app_settings: appSettingsFixture,
    });
    const wrapper = mountCard();
    await flush();
    await wrapper.vm.$nextTick();

    const status = wrapper.get('[data-testid="rule-database-status-community"]').text();
    expect(status).toContain("Last refresh failed: Couldn't reach GitHub");
    expect(status).not.toContain("connection timed out");
    expect(wrapper.get('[data-testid="rule-database-failure-detail-community"]').text()).toBe(
      "Technical details: connection timed out",
    );
    expect(wrapper.get('[data-testid="rule-databases-failure-banner"]').text()).toContain(
      "still in use",
    );
  });

  // -- enable toggles --

  it("toggling a source's switch saves app settings with only that field changed", async () => {
    const calls: AppSettingsDto[] = [];
    installMockIpc({
      get_rule_databases: [
        view({ database: "community" }),
        view({ database: "steam", enabled: false }),
      ],
      get_app_settings: appSettingsFixture,
      update_app_settings: (payload: unknown) => {
        calls.push((payload as { settings: AppSettingsDto }).settings);
        return appSettingsFixture.settings;
      },
    });
    const wrapper = mountCard();
    await flush();
    await wrapper.vm.$nextTick();

    await wrapper.get('[data-testid="rule-database-toggle-steam"] input').setValue(true);
    await flush();

    expect(calls).toEqual([
      {
        ...appSettingsFixture.settings,
        network: { ...appSettingsFixture.settings.network, fetchSteamWorkshop: true },
      },
    ]);
  });

  // The third source is a
  // row on this card like the other two, with its own label and its own
  // `fetchRimmergeRules` toggle — never a hidden always-on fetch.
  it("renders the rimmerge-rules row and toggles only its own setting", async () => {
    const calls: AppSettingsDto[] = [];
    installMockIpc({
      get_rule_databases: [
        view({ database: "community" }),
        view({ database: "steam", enabled: false }),
        view({ database: "rimmerge" }),
      ],
      get_app_settings: appSettingsFixture,
      update_app_settings: (payload: unknown) => {
        calls.push((payload as { settings: AppSettingsDto }).settings);
        return appSettingsFixture.settings;
      },
    });
    const wrapper = mountCard();
    await flush();
    await wrapper.vm.$nextTick();

    expect(wrapper.get('[data-testid="rule-database-row-rimmerge"]').text()).toContain(
      "Rimmerge rules",
    );

    await wrapper.get('[data-testid="rule-database-toggle-rimmerge"] input').setValue(false);
    await flush();

    expect(calls).toEqual([
      {
        ...appSettingsFixture.settings,
        network: { ...appSettingsFixture.settings.network, fetchRimmergeRules: false },
      },
    ]);
  });

  it("per-source toggles stay interactive when network refresh is off", async () => {
    installMockIpc({
      get_rule_databases: [
        view({ database: "community" }),
        view({ database: "steam", enabled: false }),
      ],
      get_app_settings: {
        ...appSettingsFixture,
        settings: {
          ...appSettingsFixture.settings,
          network: { ...appSettingsFixture.settings.network, allowNetwork: false },
        },
      },
    });
    const wrapper = mountCard();
    await flush();
    await wrapper.vm.$nextTick();

    expect(
      wrapper.get('[data-testid="rule-database-toggle-community"]').attributes("data-p-disabled"),
    ).not.toBe("true");
  });

  // -- the re-import signal: badge only, never a modal --

  it("shows the re-import indicator only when the cached sha differs from the imported one", async () => {
    installMockIpc({
      get_rule_databases: [
        view({
          cached: { sha256: "new-sha", bytes: 10, fetchedAt: "2026-08-01T00:00:00Z" },
          importedSha256: "old-sha",
          needsReimport: true,
        }),
        view({ database: "steam", enabled: false }),
      ],
      get_app_settings: appSettingsFixture,
    });
    const wrapper = mountCard();
    await flush();
    await wrapper.vm.$nextTick();

    expect(wrapper.find('[data-testid="rule-database-needs-reimport-community"]').exists()).toBe(
      true,
    );
    expect(wrapper.find('[data-testid="rule-database-needs-reimport-steam"]').exists()).toBe(false);
    // No dialog/modal of any kind renders anywhere in
    // this component regardless of the re-import signal.
    expect(wrapper.find('[role="dialog"]').exists()).toBe(false);
  });

  it("shows no re-import indicator when the cached sha matches what was imported", async () => {
    installMockIpc({
      get_rule_databases: [
        view({
          cached: { sha256: "same-sha", bytes: 10, fetchedAt: "2026-08-01T00:00:00Z" },
          importedSha256: "same-sha",
          needsReimport: false,
        }),
      ],
      get_app_settings: appSettingsFixture,
    });
    const wrapper = mountCard();
    await flush();
    await wrapper.vm.$nextTick();

    expect(wrapper.find('[data-testid="rule-database-needs-reimport-community"]').exists()).toBe(
      false,
    );
  });

  // -- allowNetwork = false, in full --

  it("disables Refresh with a tooltip, shows the explanatory line, keeps toggles interactive and status lines rendering", async () => {
    installMockIpc({
      get_rule_databases: [
        view({
          cached: { sha256: "abc123", bytes: 10, fetchedAt: "2026-08-01T00:00:00Z" },
        }),
        view({ database: "steam", enabled: false }),
      ],
      get_app_settings: {
        ...appSettingsFixture,
        settings: {
          ...appSettingsFixture.settings,
          network: { ...appSettingsFixture.settings.network, allowNetwork: false },
        },
      },
    });
    const wrapper = mountCard();
    await flush();
    await wrapper.vm.$nextTick();

    const refreshButton = wrapper.get('[data-testid="rule-databases-refresh"]');
    expect(refreshButton.attributes("disabled")).toBeDefined();

    // The tooltip lives on the wrapper, not the disabled
    // button itself (a disabled control is keyboard-unreachable and can
    // suppress hover in some browsers) — and that wrapper is keyboard-
    // reachable via `tabindex="0"` exactly when there's a tooltip to show.
    const refreshTooltip = wrapper.get('[data-testid="rule-databases-refresh-tooltip"]');
    expect(refreshTooltip.attributes("title")).toContain("Allow internet access");
    expect(refreshTooltip.attributes("tabindex")).toBe("0");

    expect(wrapper.get('[data-testid="rule-databases-network-disabled-notice"]').text()).toContain(
      "Internet access is off, so refresh is disabled. Cached databases are still in use.",
    );

    expect(wrapper.find('[data-testid="rule-database-toggle-community"]').exists()).toBe(true);
    expect(wrapper.get('[data-testid="rule-database-status-community"]').text()).toContain(
      "Updated",
    );
  });

  it("carries no tooltip or extra tab stop on the wrapper when network refresh is allowed", async () => {
    installMockIpc({
      get_rule_databases: [view()],
      get_app_settings: appSettingsFixture,
    });
    const wrapper = mountCard();
    await flush();
    await wrapper.vm.$nextTick();

    const refreshTooltip = wrapper.get('[data-testid="rule-databases-refresh-tooltip"]');
    expect(refreshTooltip.attributes("title")).toBeUndefined();
    expect(refreshTooltip.attributes("tabindex")).toBeUndefined();
  });

  // -- the refresh flow itself --

  it("refreshing renders a per-source result summary", async () => {
    const results: RuleDatabaseRefreshResultDto[] = [
      {
        database: "community",
        outcome: { kind: "updated", sha256: "newsha123456", bytes: 400_000 },
      },
      { database: "steam", outcome: { kind: "skipped", reason: "sourceDisabled" } },
    ];
    installMockIpc({
      get_rule_databases: [
        view({ database: "community" }),
        view({ database: "steam", enabled: false }),
      ],
      get_app_settings: appSettingsFixture,
      refresh_rule_databases: results,
    });
    const wrapper = mountCard();
    await flush();
    await wrapper.vm.$nextTick();

    await wrapper.get('[data-testid="rule-databases-refresh"]').trigger("click");
    await flush();
    await wrapper.vm.$nextTick();

    expect(wrapper.get('[data-testid="rule-database-refresh-result-community"]').text()).toContain(
      "updated",
    );
    expect(wrapper.get('[data-testid="rule-database-refresh-result-steam"]').text()).toContain(
      "skipped (source disabled)",
    );
  });

  it("the Refresh button names no sources, so every enabled one is refreshed", async () => {
    const payloads: unknown[] = [];
    installMockIpc({
      get_rule_databases: [view({ database: "community" }), view({ database: "steam" })],
      get_app_settings: appSettingsFixture,
      refresh_rule_databases: (payload: unknown) => {
        payloads.push(payload);
        return [];
      },
    });
    const wrapper = mountCard();
    await flush();
    await wrapper.vm.$nextTick();

    await wrapper.get('[data-testid="rule-databases-refresh"]').trigger("click");
    await flush();

    expect(payloads).toHaveLength(1);
    expect(payloads[0]).not.toHaveProperty("request");
  });

  // -- the automatic/manual refresh-mode label --

  it("labels an eligible source automatic when auto-refresh is on, and steam manual regardless", async () => {
    installMockIpc({
      get_rule_databases: [
        view({ database: "community" }),
        view({ database: "steam", enabled: false }),
      ],
      get_app_settings: appSettingsFixture,
    });
    const wrapper = mountCard();
    await flush();
    await wrapper.vm.$nextTick();

    expect(wrapper.get('[data-testid="rule-database-refresh-mode-community"]').text()).toBe(
      "Refreshes automatically",
    );
    expect(wrapper.get('[data-testid="rule-database-refresh-mode-steam"]').text()).toBe(
      "Manual refresh only",
    );
  });

  it("labels every source manual once auto-refresh is turned off", async () => {
    installMockIpc({
      get_rule_databases: [view({ database: "community" })],
      get_app_settings: {
        ...appSettingsFixture,
        settings: {
          ...appSettingsFixture.settings,
          network: { ...appSettingsFixture.settings.network, autoRefreshRuleDatabases: false },
        },
      },
    });
    const wrapper = mountCard();
    await flush();
    await wrapper.vm.$nextTick();

    expect(wrapper.get('[data-testid="rule-database-refresh-mode-community"]').text()).toBe(
      "Manual refresh only",
    );
  });
});
