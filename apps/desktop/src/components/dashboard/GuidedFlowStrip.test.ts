// GuidedFlowStrip: the four steps are derived from facts; only the button actions change state.

import { PiniaColada } from "@pinia/colada";
import Aura from "@primevue/themes/aura";
import { clearMocks } from "@tauri-apps/api/mocks";
import { mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import PrimeVue from "primevue/config";
import ToastService from "primevue/toastservice";
import { afterEach, describe, expect, it } from "vitest";
import { createMemoryHistory, createRouter } from "vue-router";
import {
  dashboardFixture,
  EMPTY_PENDING_ACTIVE_CHANGES,
  EMPTY_RULE_SET,
  emptyMergeModFixture,
  flush,
  preflightFixture,
} from "@/components/apply/ApplyDialog.test-support";
import GuidedFlowStrip from "@/components/dashboard/GuidedFlowStrip.vue";
import { installMockIpc } from "@/services/ipc.mock";
import type { DashboardDto } from "@/types/generated/DashboardDto";
import type { OrderSourceDto } from "@/types/generated/OrderSourceDto";
import type { PendingActiveChangesDto } from "@/types/generated/PendingActiveChangesDto";
import type { RecommendedRulesStepDto } from "@/types/generated/RecommendedRulesStepDto";

const STALE: PendingActiveChangesDto = {
  unscanned: { added: ["new.mod"], removed: [] },
  unapplied: { added: [], removed: [] },
};

function dashboard(overrides: Partial<DashboardDto> = {}): DashboardDto {
  return { ...dashboardFixture(0), selected: "suggested", ...overrides };
}

function mountStrip(data: DashboardDto) {
  const pinia = createPinia();
  setActivePinia(pinia);
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: "/", component: { render: () => null } },
      { path: "/order", component: { render: () => null } },
      { path: "/inbox", component: { render: () => null } },
    ],
  });
  return mount(GuidedFlowStrip, {
    props: { data },
    attachTo: document.body,
    global: {
      plugins: [pinia, router, PiniaColada, [PrimeVue, { theme: { preset: Aura } }], ToastService],
      stubs: { teleport: true },
    },
  });
}

const RULES_DONE: RecommendedRulesStepDto = { kind: "done", importedRulesInUse: true };

function install(
  pending: PendingActiveChangesDto = EMPTY_PENDING_ACTIVE_CHANGES,
  rules: RecommendedRulesStepDto = RULES_DONE,
) {
  const selects: OrderSourceDto[] = [];
  installMockIpc({
    // Step 1 reads this; the default (done) leaves the other three steps as they were.
    get_recommended_rules_step: rules,
    // The strip owns an Apply dialog, which reads these while mounted (hidden).
    list_rules: EMPTY_RULE_SET,
    list_mod_names: {},
    get_dashboard: dashboardFixture(0),
    get_apply_preflight: preflightFixture(),
    get_merge_mod: emptyMergeModFixture(),
    get_def_cache_carrier: { carrierModId: null },
    list_order: [],
    get_pending_active_changes: pending,
    select_order: (payload: unknown) => {
      selects.push((payload as { source: OrderSourceDto }).source);
      return { auto: 0, needsInput: 0, overridden: 0, resolvedBySuggested: 0 };
    },
  });
  return selects;
}

const stepStatus = (wrapper: ReturnType<typeof mountStrip>, id: string) =>
  wrapper.get(`[data-testid="guide-step-${id}"]`).attributes("data-status");

describe("GuidedFlowStrip", () => {
  afterEach(() => {
    clearMocks();
  });

  it("marks step one current, with the only aria-current, while Current is selected", async () => {
    install();
    const wrapper = mountStrip(dashboard({ selected: "current" }));
    await flush();

    expect(stepStatus(wrapper, "choose")).toBe("current");
    expect(stepStatus(wrapper, "apply")).toBe("upcoming");
    expect(wrapper.findAll('[aria-current="step"]')).toHaveLength(1);
    expect(wrapper.find('[data-testid="guide-choose-button"]').exists()).toBe(true);
    wrapper.unmount();
  });

  it("selects the suggested order through select_order when step one is clicked", async () => {
    const selects = install();
    const wrapper = mountStrip(dashboard({ selected: "current" }));
    await flush();

    await wrapper.get('[data-testid="guide-choose-button"]').trigger("click");
    await flush();

    expect(selects).toEqual(["suggested"]);
    wrapper.unmount();
  });

  it("moves focus to the Apply button once the suggested order is selected", async () => {
    install();
    const wrapper = mountStrip(dashboard({ selected: "current" }));
    await flush();
    await wrapper.get('[data-testid="guide-choose-button"]').trigger("click");
    await flush();

    await wrapper.setProps({ data: dashboard({ selected: "suggested" }) });
    await flush();

    expect(document.activeElement).toBe(
      wrapper.get('[data-testid="dashboard-apply-button"]').element,
    );
    wrapper.unmount();
  });

  it("puts Apply current once Suggested is selected and the file differs", async () => {
    install();
    const wrapper = mountStrip(dashboard());
    await flush();

    expect(stepStatus(wrapper, "choose")).toBe("done");
    expect(stepStatus(wrapper, "apply")).toBe("current");
    wrapper.unmount();
  });

  it("offers Rescan instead of Apply while the working set is stale", async () => {
    install(STALE);
    const wrapper = mountStrip(dashboard());
    await flush();
    await flush();

    expect(wrapper.find('[data-testid="guide-rescan-button"]').exists()).toBe(true);
    expect(wrapper.find('[data-testid="dashboard-apply-button"]').exists()).toBe(false);
    expect(wrapper.get('[data-testid="guide-stale-message"]').text()).toContain("Rescan");
    wrapper.unmount();
  });

  it("shows every step done, and no Apply button, once the file matches", async () => {
    install();
    const wrapper = mountStrip(dashboard({ fileMatchesSuggested: true }));
    await flush();

    expect(["choose", "apply", "confirm"].map((id) => stepStatus(wrapper, id))).toEqual([
      "done",
      "done",
      "done",
    ]);
    expect(wrapper.get('[data-testid="guide-done"]').text()).toContain("As of the last scan");
    expect(wrapper.find('[data-testid="dashboard-apply-button"]').exists()).toBe(false);
    wrapper.unmount();
  });

  it("offers Rescan, not done, when the file matches but the working set is stale", async () => {
    install(STALE);
    const wrapper = mountStrip(dashboard({ fileMatchesSuggested: true }));
    await flush();
    await flush();

    expect(wrapper.find('[data-testid="guide-done"]').exists()).toBe(false);
    expect(wrapper.find('[data-testid="guide-rescan-button"]').exists()).toBe(true);
    expect(stepStatus(wrapper, "apply")).toBe("current");
    wrapper.unmount();
  });

  it("words step one's done state by what made it done", async () => {
    install();
    const selected = mountStrip(dashboard());
    await flush();
    expect(selected.get('[data-testid="guide-step-choose"]').text()).toContain(
      "The suggested order is selected.",
    );
    selected.unmount();

    const fromFile = mountStrip(dashboard({ selected: "current", fileMatchesSuggested: true }));
    await flush();
    const text = fromFile.get('[data-testid="guide-step-choose"]').text();
    expect(text).toContain("ModsConfig.xml already matches the suggested order.");
    expect(text).not.toContain("The suggested order is selected.");
    fromFile.unmount();
  });

  it("speaks only on the transition into done, never on first render", async () => {
    install();
    const wrapper = mountStrip(dashboard({ fileMatchesSuggested: true }));
    await flush();
    expect(wrapper.get('[data-testid="guide-live-region"]').text()).toBe("");
    wrapper.unmount();

    const second = mountStrip(dashboard());
    await flush();
    expect(second.get('[data-testid="guide-live-region"]').text()).toBe("");

    await second.setProps({ data: dashboard({ fileMatchesSuggested: true }) });
    await flush();

    expect(second.get('[data-testid="guide-live-region"]').text()).toContain("Applied");
    second.unmount();
  });

  it("hides the needs-input hint at zero and links to the Inbox otherwise", async () => {
    install();
    const none = mountStrip(dashboard());
    await flush();
    expect(none.find('[data-testid="guide-needs-input"]').exists()).toBe(false);
    none.unmount();

    const some = mountStrip(
      dashboard({
        selected: "current",
        ledgerStats: {
          current: { auto: 0, needsInput: 2, overridden: 0, resolvedBySuggested: 0 },
          suggested: { auto: 0, needsInput: 0, overridden: 0, resolvedBySuggested: 0 },
        },
      }),
    );
    await flush();
    expect(some.get('[data-testid="guide-needs-input"]').text()).toContain("2 findings");
    expect(some.find('[data-testid="guide-inbox-link"]').exists()).toBe(true);
    some.unmount();
  });
});
