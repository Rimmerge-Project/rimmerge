// GuidedFlowStrip step 1, "Get the recommended rules": its states, button labels, Skip, progress.

import { PiniaColada } from "@pinia/colada";
import Aura from "@primevue/themes/aura";
import { emit } from "@tauri-apps/api/event";
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
import type { RecommendedRulesReportDto } from "@/types/generated/RecommendedRulesReportDto";
import type { RecommendedRulesStepDto } from "@/types/generated/RecommendedRulesStepDto";

const FAILURE = { cause: { kind: "transport" }, detail: "connection timed out" } as const;

const TURN_ON_AND_DOWNLOAD: RecommendedRulesStepDto = {
  kind: "needsAction",
  sources: [
    { database: "community", need: { kind: "download", lastFailure: null } },
    { database: "steam", need: { kind: "turnOn" } },
  ],
};
const COMMUNITY_ONLY: RecommendedRulesStepDto = {
  kind: "needsAction",
  sources: [
    { database: "community", need: { kind: "download", lastFailure: null } },
    { database: "steam", need: { kind: "import" } },
  ],
};
const IMPORT_ONLY: RecommendedRulesStepDto = {
  kind: "needsAction",
  sources: [{ database: "community", need: { kind: "import" } }],
};
const AFTER_FAILURE: RecommendedRulesStepDto = {
  kind: "needsAction",
  sources: [{ database: "steam", need: { kind: "download", lastFailure: FAILURE } }],
};

const SKIPPED_STEAM_OFF: RecommendedRulesStepDto = {
  kind: "skipped",
  sources: [
    { database: "community", need: { kind: "import" } },
    { database: "steam", need: { kind: "turnOn" } },
  ],
};
const SKIPPED_IMPORT_ONLY: RecommendedRulesStepDto = {
  kind: "skipped",
  sources: [{ database: "community", need: { kind: "import" } }],
};

const GOOD_REPORT: RecommendedRulesReportDto = {
  downloads: [],
  import: { kind: "notNeeded" },
};
const FAILED_DOWNLOAD_REPORT: RecommendedRulesReportDto = {
  downloads: [{ database: "steam", outcome: { kind: "failed", failure: FAILURE } }],
  import: { kind: "notNeeded" },
};

interface Calls {
  get: number;
  skip: number;
}

/**
 * Mocks the strip's commands. `rules` is the step the backend answers with, mutable through
 * the returned `state` so a test can model what a click or a skip leaves behind.
 */
function install(initial: RecommendedRulesStepDto | "never-answers" | "fails") {
  const calls: Calls = { get: 0, skip: 0 };
  const state: {
    rules: RecommendedRulesStepDto;
    release: (() => void) | null;
    /** What the step reads once a click has finished. */
    afterClick: RecommendedRulesStepDto;
    report: RecommendedRulesReportDto;
  } = {
    rules: typeof initial === "string" ? SKIPPED_IMPORT_ONLY : initial,
    release: null,
    afterClick: { kind: "done", importedRulesInUse: true },
    report: GOOD_REPORT,
  };
  installMockIpc({
    get_recommended_rules_step: () => {
      if (initial === "fails") {
        throw new Error("the step could not be read");
      }
      return initial === "never-answers" ? new Promise(() => undefined) : state.rules;
    },
    get_recommended_rules: async () => {
      calls.get += 1;
      await new Promise<void>((resolve) => {
        state.release = resolve;
      });
      state.rules = state.afterClick;
      return state.report;
    },
    skip_recommended_rules_step: () => {
      calls.skip += 1;
      state.rules = SKIPPED_IMPORT_ONLY;
    },
    list_rules: EMPTY_RULE_SET,
    list_mod_names: {},
    get_dashboard: dashboardFixture(0),
    get_apply_preflight: preflightFixture(),
    get_merge_mod: emptyMergeModFixture(),
    get_def_cache_carrier: { carrierModId: null },
    list_order: [],
    list_notifications: [],
    get_pending_active_changes: EMPTY_PENDING_ACTIVE_CHANGES,
    select_order: { auto: 0, needsInput: 0, overridden: 0, resolvedBySuggested: 0 },
  });
  return { calls, state };
}

function mountStrip() {
  const pinia = createPinia();
  setActivePinia(pinia);
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: "/", component: { render: () => null } },
      { path: "/order", component: { render: () => null } },
      { path: "/inbox", component: { render: () => null } },
      { path: "/settings", component: { render: () => null } },
    ],
  });
  return mount(GuidedFlowStrip, {
    props: { data: { ...dashboardFixture(0), selected: "suggested" } },
    attachTo: document.body,
    global: {
      plugins: [pinia, router, PiniaColada, [PrimeVue, { theme: { preset: Aura } }], ToastService],
      stubs: { teleport: true },
    },
  });
}

const stepStatus = (wrapper: ReturnType<typeof mountStrip>, id: string) =>
  wrapper.get(`[data-testid="guide-step-${id}"]`).attributes("data-status");

async function mountSettled(initial: RecommendedRulesStepDto | "never-answers" | "fails") {
  const mocks = install(initial);
  const wrapper = mountStrip();
  await flush();
  await flush();
  return { wrapper, ...mocks };
}

describe("GuidedFlowStrip step 1", () => {
  afterEach(() => {
    clearMocks();
    document.body.innerHTML = "";
  });

  it("lists four steps, numbered one to four, under a four-step intro", async () => {
    const { wrapper } = await mountSettled(TURN_ON_AND_DOWNLOAD);

    const numbers = wrapper
      .findAll('[data-testid^="guide-step-"]')
      .map((row) => row.get("span").text());
    expect(numbers).toEqual(["1", "2", "3", "4"]);
    expect(wrapper.text()).toContain("Four steps");
    wrapper.unmount();
  });

  it("holds the flow on step 1 while rules are offered, with the only aria-current", async () => {
    const { wrapper } = await mountSettled(TURN_ON_AND_DOWNLOAD);

    expect(stepStatus(wrapper, "rules")).toBe("current");
    expect(stepStatus(wrapper, "apply")).toBe("upcoming");
    expect(wrapper.findAll('[aria-current="step"]')).toHaveLength(1);
    expect(wrapper.get('[data-testid="guide-step-rules"]').attributes("aria-current")).toBe("step");
    wrapper.unmount();
  });

  it("names each source's need on its own line", async () => {
    const { wrapper } = await mountSettled(TURN_ON_AND_DOWNLOAD);

    expect(wrapper.get('[data-testid="guide-rules-source-community"]').text()).toBe(
      "Community rules: not downloaded yet.",
    );
    expect(wrapper.get('[data-testid="guide-rules-source-steam"]').text()).toBe(
      "Steam Workshop: off. It will be turned on and downloaded.",
    );
    wrapper.unmount();
  });

  it("falls back to the other steps, with a loading line, until the step has answered", async () => {
    const { wrapper } = await mountSettled("never-answers");

    expect(wrapper.get('[data-testid="guide-rules-loading"]').text()).toBe("Loading…");
    expect(stepStatus(wrapper, "rules")).toBe("upcoming");
    expect(stepStatus(wrapper, "apply")).toBe("current");
    expect(wrapper.find('[data-testid="guide-rules-button"]').exists()).toBe(false);
    wrapper.unmount();
  });

  it.each([
    ["steam must be downloaded", TURN_ON_AND_DOWNLOAD, "Download and import (about 49 MB)"],
    ["only community must be downloaded", COMMUNITY_ONLY, "Download and import"],
    ["only an import is left", IMPORT_ONLY, "Import"],
    ["steam failed last time", AFTER_FAILURE, "Try again (about 49 MB)"],
  ] as const)("labels the button by what a click does when %s", async (_name, rules, label) => {
    const { wrapper } = await mountSettled(rules);

    expect(wrapper.get('[data-testid="guide-rules-button"]').text()).toBe(label);
    wrapper.unmount();
  });

  it("runs the click once and shows the done text after it", async () => {
    const { wrapper, calls, state } = await mountSettled(TURN_ON_AND_DOWNLOAD);

    await wrapper.get('[data-testid="guide-rules-button"]').trigger("click");
    await flush();
    state.release?.();
    await flush();
    await flush();

    expect(calls.get).toBe(1);
    expect(stepStatus(wrapper, "rules")).toBe("done");
    expect(wrapper.get('[data-testid="guide-rules-done-text"]').text()).toBe(
      "Imported. The suggested order uses them.",
    );
    expect(document.activeElement).toBe(
      wrapper.get('[data-testid="dashboard-apply-button"]').element,
    );
    wrapper.unmount();
  });

  it("shows a loading, disabled button and no Skip while the click runs", async () => {
    const { wrapper } = await mountSettled(TURN_ON_AND_DOWNLOAD);

    await wrapper.get('[data-testid="guide-rules-button"]').trigger("click");
    await flush();

    expect(stepStatus(wrapper, "rules")).toBe("current");
    expect(wrapper.get('[data-testid="guide-rules-button"]').attributes("disabled")).toBeDefined();
    expect(wrapper.find('[data-testid="guide-rules-skip"]').exists()).toBe(false);
    wrapper.unmount();
  });

  it("follows the latest progress event, with the slow note only while Steam downloads", async () => {
    const { wrapper, state } = await mountSettled(TURN_ON_AND_DOWNLOAD);
    await wrapper.get('[data-testid="guide-rules-button"]').trigger("click");
    await flush();

    await emit("rules://recommended-progress", { kind: "downloading", database: "community" });
    await flush();
    expect(wrapper.get('[data-testid="guide-rules-progress"]').text()).toBe(
      "Downloading the Community rules database…",
    );
    expect(wrapper.find('[data-testid="guide-rules-slow-note"]').exists()).toBe(false);

    await emit("rules://recommended-progress", { kind: "downloading", database: "steam" });
    await flush();
    expect(wrapper.get('[data-testid="guide-rules-progress"]').text()).toBe(
      "Downloading the Steam Workshop database…",
    );
    expect(wrapper.get('[data-testid="guide-rules-slow-note"]').text()).toContain(
      "slow connection",
    );
    expect(wrapper.get('[data-testid="guide-rules-live-region"]').text()).toBe(
      "Downloading the Steam Workshop database…",
    );

    await emit("rules://recommended-progress", { kind: "importing", databases: ["steam"] });
    await flush();
    expect(wrapper.get('[data-testid="guide-rules-progress"]').text()).toBe("Importing…");
    expect(wrapper.find('[data-testid="guide-rules-slow-note"]').exists()).toBe(false);
    state.release?.();
    await flush();
    wrapper.unmount();
  });

  it("shows no button, only a Settings link and Skip, while internet access is off", async () => {
    const { wrapper } = await mountSettled({ kind: "unavailable", reason: "networkOff" });

    expect(stepStatus(wrapper, "rules")).toBe("unavailable");
    expect(wrapper.find('[data-testid="guide-rules-button"]').exists()).toBe(false);
    expect(wrapper.get('[data-testid="guide-rules-settings-link"]').attributes("href")).toBe(
      "/settings",
    );
    expect(wrapper.find('[data-testid="guide-rules-skip"]').exists()).toBe(true);
    expect(stepStatus(wrapper, "apply")).toBe("current");
    wrapper.unmount();
  });

  it("asks to answer the first-run notice, with no button and no Settings link", async () => {
    const { wrapper } = await mountSettled({ kind: "unavailable", reason: "awaitingFirstRun" });

    expect(wrapper.get('[data-testid="guide-rules-unavailable"]').text()).toBe(
      "Answer the Welcome notice above first.",
    );
    expect(wrapper.find('[data-testid="guide-rules-button"]').exists()).toBe(false);
    expect(wrapper.find('[data-testid="guide-rules-settings-link"]').exists()).toBe(false);
    wrapper.unmount();
  });

  it("calls the skip command from a real button, then reads skipped and moves focus on", async () => {
    const { wrapper, calls } = await mountSettled(TURN_ON_AND_DOWNLOAD);
    const skip = wrapper.get('[data-testid="guide-rules-skip"]');
    expect(skip.element.tagName).toBe("BUTTON");

    await skip.trigger("click");
    await flush();
    await flush();

    expect(calls.skip).toBe(1);
    expect(stepStatus(wrapper, "rules")).toBe("skipped");
    expect(stepStatus(wrapper, "apply")).toBe("current");
    expect(document.activeElement).toBe(
      wrapper.get('[data-testid="dashboard-apply-button"]').element,
    );
    wrapper.unmount();
  });

  it("offers Get them now once skipped, and runs the same click", async () => {
    const { wrapper, calls } = await mountSettled(SKIPPED_IMPORT_ONLY);

    expect(wrapper.get('[data-testid="guide-rules-skipped-text"]').text()).toContain("Skipped.");
    await wrapper.get('[data-testid="guide-rules-get-now"]').trigger("click");
    await flush();

    expect(calls.get).toBe(1);
    wrapper.unmount();
  });

  it("lists the skipped step's sources like the offered step does", async () => {
    const { wrapper } = await mountSettled(SKIPPED_STEAM_OFF);

    expect(wrapper.get('[data-testid="guide-rules-source-community"]').text()).toBe(
      "Community rules: downloaded, not imported yet.",
    );
    expect(wrapper.get('[data-testid="guide-rules-source-steam"]').text()).toBe(
      "Steam Workshop: off. It will be turned on and downloaded.",
    );
    wrapper.unmount();
  });

  it.each([
    ["steam is off and would be downloaded", SKIPPED_STEAM_OFF, "Get them now (about 49 MB)"],
    ["only an import is left", SKIPPED_IMPORT_ONLY, "Get them now"],
  ] as const)("labels Get them now with the size only when %s", async (_name, rules, label) => {
    const { wrapper } = await mountSettled(rules);

    expect(wrapper.get('[data-testid="guide-rules-get-now"]').text()).toBe(label);
    wrapper.unmount();
  });

  it("keeps the label the click started with while it runs", async () => {
    const { wrapper } = await mountSettled(SKIPPED_STEAM_OFF);

    await wrapper.get('[data-testid="guide-rules-get-now"]').trigger("click");
    await flush();

    expect(wrapper.get('[data-testid="guide-rules-button"]').text()).toBe(
      "Get them now (about 49 MB)",
    );
    wrapper.unmount();
  });

  it("keeps an import-only click's label while it runs", async () => {
    const { wrapper } = await mountSettled(IMPORT_ONLY);

    await wrapper.get('[data-testid="guide-rules-button"]').trigger("click");
    await flush();

    expect(wrapper.get('[data-testid="guide-rules-button"]').text()).toBe("Import");
    wrapper.unmount();
  });

  it("returns focus to the button after a click that left step 1 open", async () => {
    const { wrapper, state } = await mountSettled(TURN_ON_AND_DOWNLOAD);
    state.afterClick = AFTER_FAILURE;
    state.report = FAILED_DOWNLOAD_REPORT;

    await wrapper.get('[data-testid="guide-rules-button"]').trigger("click");
    await flush();
    state.release?.();
    await flush();
    await flush();

    expect(stepStatus(wrapper, "rules")).toBe("current");
    expect(document.activeElement).toBe(wrapper.get('[data-testid="guide-rules-button"]').element);
    wrapper.unmount();
  });

  it("falls back to Skip after a refused click leaves step 1 unavailable", async () => {
    const { wrapper, state } = await mountSettled(TURN_ON_AND_DOWNLOAD);
    state.afterClick = { kind: "unavailable", reason: "networkOff" };

    await wrapper.get('[data-testid="guide-rules-button"]').trigger("click");
    await flush();
    state.release?.();
    await flush();
    await flush();

    expect(stepStatus(wrapper, "rules")).toBe("unavailable");
    expect(document.activeElement).toBe(wrapper.get('[data-testid="guide-rules-skip"]').element);
    wrapper.unmount();
  });

  it("says the step could not be read and moves on to the other steps when its query fails", async () => {
    const { wrapper } = await mountSettled("fails");

    expect(wrapper.get('[data-testid="guide-rules-load-failed"]').text()).toBe(
      "Couldn't check whether the recommended rules are set up.",
    );
    expect(wrapper.find('[data-testid="guide-rules-button"]').exists()).toBe(false);
    expect(stepStatus(wrapper, "rules")).toBe("upcoming");
    expect(stepStatus(wrapper, "apply")).toBe("current");
    wrapper.unmount();
  });

  it("words the done step by whether imported rules are in use", async () => {
    const inUse = await mountSettled({ kind: "done", importedRulesInUse: true });
    expect(inUse.wrapper.get('[data-testid="guide-rules-done-text"]').text()).toBe(
      "Imported. The suggested order uses them.",
    );
    inUse.wrapper.unmount();
    clearMocks();

    const notInUse = await mountSettled({ kind: "done", importedRulesInUse: false });
    expect(notInUse.wrapper.get('[data-testid="guide-rules-done-text"]').text()).toContain(
      "turned off in Settings",
    );
    notInUse.wrapper.unmount();
  });

  it("shows a loading button and no Skip when another window's click is running", async () => {
    const { wrapper } = await mountSettled({ kind: "inProgress" });

    expect(stepStatus(wrapper, "rules")).toBe("current");
    expect(wrapper.get('[data-testid="guide-rules-button"]').attributes("disabled")).toBeDefined();
    expect(wrapper.find('[data-testid="guide-rules-skip"]').exists()).toBe(false);
    wrapper.unmount();
  });
});
