import { PiniaColada } from "@pinia/colada";
import Aura from "@primevue/themes/aura";
import { clearMocks } from "@tauri-apps/api/mocks";
import { mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import PrimeVue from "primevue/config";
import Select from "primevue/select";
import ToastService from "primevue/toastservice";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import AddPairRuleDialog from "@/components/rules/AddPairRuleDialog.vue";
import AddPlacementRuleDialog from "@/components/rules/AddPlacementRuleDialog.vue";
import RulesPage from "@/pages/RulesPage.vue";
import { installMockIpc } from "@/services/ipc.mock";
import type { AppSettingsResponseDto } from "@/types/generated/AppSettingsResponseDto";
import type { RuleKeyDto } from "@/types/generated/RuleKeyDto";
import type { RuleSetDto } from "@/types/generated/RuleSetDto";
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

const rulesFixture: RuleSetDto = {
  pairs: [
    {
      after: "a.mod",
      before: "b.mod",
      origin: "rimSortCommunity",
      comment: null,
      promotedFrom: null,
      alreadyPromoted: false,
      overridesDeclared: false,
    },
  ],
  placements: [],
  incompatibles: [],
  warnings: [],
};

function mountRulesPage(rules: RuleSetDto = rulesFixture) {
  installMockIpc({
    list_rules: rules,
    get_settings: settingsFixture,
    get_app_settings: appSettingsFixture,
    list_tags: { assignments: [] },
    list_orphaned_decisions: [],
    get_rule_databases: [],
    get_default_rimsort_paths: null,
    list_mod_names: {},
    // `AddPlacementRuleDialog` (via its own reused `ModPicker`) and its
    // dependent-count query both live unconditionally in this page's own
    // template — same reason `get_default_rimsort_paths` above is needed
    // even though no test here opens the import dialog either.
    list_findings: { total: 0, items: [] },
    list_mods: { total: 0, items: [] },
  });
  return mount(RulesPage, {
    global: {
      plugins: [createPinia(), PiniaColada, [PrimeVue, { theme: { preset: Aura } }], ToastService],
      stubs: { teleport: true },
    },
  });
}

async function flush(): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, 0));
}

describe("RulesPage", () => {
  beforeEach(() => {
    Object.defineProperty(HTMLElement.prototype, "offsetHeight", {
      configurable: true,
      value: 400,
    });
    Object.defineProperty(HTMLElement.prototype, "offsetWidth", {
      configurable: true,
      value: 600,
    });
  });

  afterEach(() => {
    clearMocks();
    Reflect.deleteProperty(HTMLElement.prototype, "offsetHeight");
    Reflect.deleteProperty(HTMLElement.prototype, "offsetWidth");
  });

  it("states both imported-rule toggles' current state next to the import button", async () => {
    const wrapper = mountRulesPage();
    await flush();
    await wrapper.vm.$nextTick();

    expect(wrapper.get('[data-testid="rules-import-toggle-state"]').text()).toBe(
      "Imported pairs: off · Imported placements: on",
    );
  });

  it("shows a rules-load warning when the loaded rule set carries one", async () => {
    const wrapper = mountRulesPage({
      ...rulesFixture,
      warnings: ["dropped 2 pre-migration cluster rule(s) on load: r1, r2"],
    });
    await flush();
    await wrapper.vm.$nextTick();

    expect(wrapper.get('[data-testid="rules-load-warning"]').text()).toContain(
      "dropped 2 pre-migration cluster rule(s)",
    );
  });

  it("shows no rules-load warning when the loaded rule set carries none", async () => {
    const wrapper = mountRulesPage();
    await flush();
    await wrapper.vm.$nextTick();

    expect(wrapper.find('[data-testid="rules-load-warning"]').exists()).toBe(false);
  });

  // A not-imported source must read as
  // a warning ("not imported"), never as an imported zero — the exact
  // distinction `ImportReportDto`'s `null`-vs-`0` wire shape exists for.
  it("shows the import report distinguishing a not-imported source from a real zero", async () => {
    installMockIpc({
      list_rules: rulesFixture,
      get_settings: settingsFixture,
      get_app_settings: appSettingsFixture,
      list_tags: { assignments: [] },
      list_orphaned_decisions: [],
      get_rule_databases: [],
      get_default_rimsort_paths: null,
      list_mod_names: {},
      list_findings: { total: 0, items: [] },
      list_mods: { total: 0, items: [] },
      import_rimsort: () => ({
        userRules: null,
        communityRules: 0,
        steamDependencies: 3,
        skippedInactiveRules: 0,
        skippedInactiveSteam: 0,
      }),
    });
    const wrapper = mount(RulesPage, {
      global: {
        plugins: [
          createPinia(),
          PiniaColada,
          [PrimeVue, { theme: { preset: Aura } }],
          ToastService,
        ],
        stubs: { teleport: true },
      },
    });
    await flush();
    await wrapper.vm.$nextTick();

    await wrapper.get('[data-testid="open-import-dialog"]').trigger("click");
    await wrapper.vm.$nextTick();
    await wrapper.get('[data-testid="import-rimsort-form"]').trigger("submit");
    await flush();
    await wrapper.vm.$nextTick();

    expect(wrapper.get('[data-testid="import-report-user-rules"]').text()).toContain(
      "not imported",
    );
    expect(wrapper.get('[data-testid="import-report-community-rules"]').text()).toContain(
      "Community rules: 0",
    );
    expect(wrapper.get('[data-testid="import-report-steam-dependencies"]').text()).toContain(
      "Steam dependencies: 3",
    );
    // A `null` source (here, `userRules`) must escalate the banner's own
    // severity to `warn`, not just its text. `data-testid` lands on Vue Test
    // Utils' own `<transition-stub>` wrapper, not PrimeVue's real root
    // (`.p-message`), which is where `data-p` (the severity flag) is.
    expect(wrapper.get('[data-testid="import-report"] .p-message').attributes("data-p")).toContain(
      "warn",
    );
  });

  it("shows the import report as plain info when every source actually imported", async () => {
    installMockIpc({
      list_rules: rulesFixture,
      get_settings: settingsFixture,
      get_app_settings: appSettingsFixture,
      list_tags: { assignments: [] },
      list_orphaned_decisions: [],
      get_rule_databases: [],
      get_default_rimsort_paths: null,
      list_mod_names: {},
      list_findings: { total: 0, items: [] },
      list_mods: { total: 0, items: [] },
      import_rimsort: () => ({
        userRules: 2,
        communityRules: 1,
        steamDependencies: 0,
        skippedInactiveRules: 0,
        skippedInactiveSteam: 0,
      }),
    });
    const wrapper = mount(RulesPage, {
      global: {
        plugins: [
          createPinia(),
          PiniaColada,
          [PrimeVue, { theme: { preset: Aura } }],
          ToastService,
        ],
        stubs: { teleport: true },
      },
    });
    await flush();
    await wrapper.vm.$nextTick();

    await wrapper.get('[data-testid="open-import-dialog"]').trigger("click");
    await wrapper.vm.$nextTick();
    await wrapper.get('[data-testid="import-rimsort-form"]').trigger("submit");
    await flush();
    await wrapper.vm.$nextTick();

    expect(
      wrapper.get('[data-testid="import-report"] .p-message').attributes("data-p"),
    ).not.toContain("warn");
  });

  // The re-import hint is a plain inline badge next to
  // the import button, never a dialog — asserted directly, not merely
  // assumed from the absence of a spec that opens one.
  it("shows the re-import hint next to the import button only on a sha mismatch, and it is never a modal", async () => {
    installMockIpc({
      list_rules: rulesFixture,
      get_settings: settingsFixture,
      get_app_settings: appSettingsFixture,
      list_tags: { assignments: [] },
      list_orphaned_decisions: [],
      get_rule_databases: [
        {
          database: "community",
          enabled: true,
          cached: { sha256: "new-sha", bytes: 10, fetchedAt: "2026-08-01T00:00:00Z" },
          isStale: false,
          lastFailure: null,
          importedSha256: "old-sha",
          needsReimport: true,
        },
      ],
      get_default_rimsort_paths: null,
      list_mod_names: {},
      list_findings: { total: 0, items: [] },
      list_mods: { total: 0, items: [] },
    });
    const wrapper = mount(RulesPage, {
      global: {
        plugins: [
          createPinia(),
          PiniaColada,
          [PrimeVue, { theme: { preset: Aura } }],
          ToastService,
        ],
        stubs: { teleport: true },
      },
    });
    await flush();
    await wrapper.vm.$nextTick();

    expect(wrapper.get('[data-testid="rule-databases-reimport-hint"]').text()).toContain(
      "Cached databases have changed since this profile last imported them",
    );
    expect(wrapper.find('[role="dialog"]').exists()).toBe(false);
  });

  it("shows no re-import hint when nothing needs reimporting", async () => {
    const wrapper = mountRulesPage();
    await flush();
    await wrapper.vm.$nextTick();

    expect(wrapper.find('[data-testid="rule-databases-reimport-hint"]').exists()).toBe(false);
  });

  it("dismisses the rules-load warning when its close button is clicked", async () => {
    const wrapper = mountRulesPage({
      ...rulesFixture,
      warnings: ["dropped 2 pre-migration cluster rule(s) on load: r1, r2"],
    });
    await flush();
    await wrapper.vm.$nextTick();

    const warning = wrapper.get('[data-testid="rules-load-warning"]');
    await warning.get("button").trigger("click");
    await wrapper.vm.$nextTick();

    expect(wrapper.find('[data-testid="rules-load-warning"]').exists()).toBe(false);
  });

  it("promoting an imported pair rule calls promote_imported_rule with its key", async () => {
    const calls: RuleKeyDto[] = [];
    installMockIpc({
      list_rules: rulesFixture,
      get_settings: settingsFixture,
      get_app_settings: appSettingsFixture,
      list_tags: { assignments: [] },
      list_orphaned_decisions: [],
      get_rule_databases: [],
      get_default_rimsort_paths: null,
      list_mod_names: {},
      list_findings: { total: 0, items: [] },
      list_mods: { total: 0, items: [] },
      promote_imported_rule: (payload: unknown) => {
        calls.push((payload as { key: RuleKeyDto }).key);
        return rulesFixture;
      },
    });
    const wrapper = mount(RulesPage, {
      global: {
        plugins: [
          createPinia(),
          PiniaColada,
          [PrimeVue, { theme: { preset: Aura } }],
          ToastService,
        ],
        stubs: { teleport: true },
      },
    });
    await flush();
    await wrapper.vm.$nextTick();

    await wrapper
      .get('[data-testid="pair-rule-a.mod-b.mod-rimSortCommunity-promote"]')
      .trigger("click");
    await flush();

    expect(calls).toEqual([{ kind: "pair", after: "a.mod", before: "b.mod" }]);
  });

  it("adding a placement rule calls upsert_rule with the exact payload and closes the dialog", async () => {
    const calls: unknown[] = [];
    installMockIpc({
      list_rules: rulesFixture,
      get_settings: settingsFixture,
      get_app_settings: appSettingsFixture,
      list_tags: { assignments: [] },
      list_orphaned_decisions: [],
      get_rule_databases: [],
      get_default_rimsort_paths: null,
      list_mod_names: {},
      list_findings: { total: 0, items: [] },
      list_mods: { total: 1, items: [{ modId: "c.mod", name: "Mod C" }] },
      upsert_rule: (payload: unknown) => {
        calls.push((payload as { rule: unknown }).rule);
        return rulesFixture;
      },
    });
    // Not `mountRulesPage` — its own internal `installMockIpc` call would
    // overwrite the fixtures (`upsert_rule`, a non-empty `list_mods`)
    // this test just registered above.
    const wrapper = mount(RulesPage, {
      global: {
        plugins: [
          createPinia(),
          PiniaColada,
          [PrimeVue, { theme: { preset: Aura } }],
          ToastService,
        ],
        stubs: { teleport: true },
      },
    });
    await flush();
    await wrapper.vm.$nextTick();

    await wrapper.get('[data-testid="open-add-rule-dialog"]').trigger("click");
    await wrapper.vm.$nextTick();
    await wrapper.get('[data-testid="add-rule-mod-search"]').setValue("mod");
    await flush();
    await wrapper.vm.$nextTick();
    await wrapper.get('[data-testid="add-rule-mod-add-c.mod"]').trigger("click");
    // Scoped to the dialog's own subtree, then to `Select` within it — a
    // bare `wrapper.findComponent(Select)` would grab whichever `Select`
    // appears first in the whole page (`TagRuleEditor`'s own mode select
    // renders earlier in the template) instead of this dialog's own.
    await wrapper
      .getComponent(AddPlacementRuleDialog)
      .findComponent(Select)
      .vm.$emit("update:modelValue", "bottom");
    await wrapper.get('[data-testid="add-rule-form"]').trigger("submit");
    await flush();
    await wrapper.vm.$nextTick();

    expect(calls).toEqual([
      {
        kind: "placement",
        modId: "c.mod",
        placement: "bottom",
        origin: "userDecision",
        comment: null,
        promotedFrom: null,
        alreadyPromoted: false,
      },
    ]);
    expect(wrapper.find('[data-testid="add-rule-dialog"]').exists()).toBe(false);
  });

  it("adding a pair rule calls upsert_rule with the exact payload and closes the dialog", async () => {
    const calls: unknown[] = [];
    installMockIpc({
      list_rules: rulesFixture,
      get_settings: settingsFixture,
      get_app_settings: appSettingsFixture,
      list_tags: { assignments: [] },
      list_orphaned_decisions: [],
      get_rule_databases: [],
      get_default_rimsort_paths: null,
      list_mod_names: {},
      list_findings: { total: 0, items: [] },
      list_mods: {
        total: 2,
        items: [
          { modId: "c.mod", name: "Mod C" },
          { modId: "d.mod", name: "Mod D" },
        ],
      },
      upsert_rule: (payload: unknown) => {
        calls.push((payload as { rule: unknown }).rule);
        return rulesFixture;
      },
    });
    // Not `mountRulesPage` — same reason as the placement test above.
    const wrapper = mount(RulesPage, {
      global: {
        plugins: [
          createPinia(),
          PiniaColada,
          [PrimeVue, { theme: { preset: Aura } }],
          ToastService,
        ],
        stubs: { teleport: true },
      },
    });
    await flush();
    await wrapper.vm.$nextTick();

    await wrapper.get('[data-testid="open-add-pair-rule-dialog"]').trigger("click");
    await wrapper.vm.$nextTick();
    const dialog = wrapper.getComponent(AddPairRuleDialog);
    await dialog.get('[data-testid="add-pair-rule-after-search"]').setValue("mod");
    await flush();
    await wrapper.vm.$nextTick();
    await dialog.get('[data-testid="add-pair-rule-after-add-c.mod"]').trigger("click");
    await dialog.get('[data-testid="add-pair-rule-before-search"]').setValue("mod");
    await flush();
    await wrapper.vm.$nextTick();
    await dialog.get('[data-testid="add-pair-rule-before-add-d.mod"]').trigger("click");
    await dialog.get('[data-testid="add-pair-rule-form"]').trigger("submit");
    await flush();
    await wrapper.vm.$nextTick();

    expect(calls).toEqual([
      {
        kind: "pair",
        after: "c.mod",
        before: "d.mod",
        origin: "userDecision",
        comment: null,
        promotedFrom: null,
        alreadyPromoted: false,
        overridesDeclared: false,
      },
    ]);
    expect(wrapper.find('[data-testid="add-pair-rule-dialog"]').exists()).toBe(false);
  });

  // `list_findings`'s own summary carries no `modId` field for any
  // finding kind (`usePlacementDependentCountsQuery`'s own doc comment),
  // so this also pins the second-round-trip `get_finding` call the page
  // makes to actually read the count off the full finding.
  it("shows the promoted-dependent count next to a pin that has one", async () => {
    const withPlacement: RuleSetDto = {
      ...rulesFixture,
      placements: [
        {
          modId: "basement.optimizer.mod",
          placement: "bottom",
          origin: "userDecision",
          comment: null,
          promotedFrom: null,
          alreadyPromoted: false,
        },
      ],
    };
    installMockIpc({
      list_rules: withPlacement,
      get_settings: settingsFixture,
      get_app_settings: appSettingsFixture,
      list_tags: { assignments: [] },
      list_orphaned_decisions: [],
      get_rule_databases: [],
      get_default_rimsort_paths: null,
      list_mod_names: {},
      list_mods: { total: 0, items: [] },
      list_findings: {
        total: 1,
        items: [
          {
            key: "placement_promotes_dependents:basement.optimizer.mod:bottom",
            title: "basement.optimizer.mod's placement promotes 2 other mods",
            status: "auto",
            confidence: 90,
            effective: { kind: "accept" },
            hasDecision: false,
            mergeState: null,
            scope: null,
            defRef: null,
          },
        ],
      },
      get_finding: () => ({
        key: "placement_promotes_dependents:basement.optimizer.mod:bottom",
        title: "basement.optimizer.mod's placement promotes 2 other mods",
        finding: {
          kind: "placementPromotesDependents",
          modId: "basement.optimizer.mod",
          placement: "bottom",
          promoted: ["dependent.a", "dependent.b"],
        },
        suggestion: { confidence: 90, alternatives: [] },
        status: "auto",
        effective: { kind: "accept" },
        note: null,
        hasDecision: false,
        resolvedBySuggested: null,
        mergeState: null,
        scope: null,
        defRef: null,
      }),
    });
    // Not `mountRulesPage` — same reason as the test above.
    const wrapper = mount(RulesPage, {
      global: {
        plugins: [
          createPinia(),
          PiniaColada,
          [PrimeVue, { theme: { preset: Aura } }],
          ToastService,
        ],
        stubs: { teleport: true },
      },
    });
    await flush();
    await wrapper.vm.$nextTick();
    await flush();
    await wrapper.vm.$nextTick();

    expect(
      wrapper
        .get('[data-testid="placement-rule-basement.optimizer.mod-userDecision-promotes"]')
        .text(),
    ).toContain("2 other mods");
  });
});
