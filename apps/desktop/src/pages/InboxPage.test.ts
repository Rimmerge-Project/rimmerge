import { PiniaColada } from "@pinia/colada";
import Aura from "@primevue/themes/aura";
import { clearMocks } from "@tauri-apps/api/mocks";
import { mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import PrimeVue from "primevue/config";
import ToastService from "primevue/toastservice";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { createMemoryHistory, createRouter } from "vue-router";

import InboxPage from "@/pages/InboxPage.vue";
import { installMockIpc } from "@/services/ipc.mock";
import { useInboxStore } from "@/stores/inbox";
import type { DecideRequestDto } from "@/types/generated/DecideRequestDto";
import type { ResolutionDetailDto } from "@/types/generated/ResolutionDetailDto";
import type { ResolutionSummaryDto } from "@/types/generated/ResolutionSummaryDto";

function summary(key: string): ResolutionSummaryDto {
  return {
    key,
    finding: { kind: "missingMod", modId: `${key}.mod` },
    status: "needsInput",
    confidence: 50,
    effective: { kind: "removeMod", modId: `${key}.mod` },
    hasDecision: false,
    mergeState: null,
    structuralGuardField: null,
    scope: null,
    defRef: null,
  };
}

function detailFor(key: string): ResolutionDetailDto {
  return {
    key,
    finding: { kind: "missingMod", modId: `${key}.mod` },
    suggestion: {
      action: { kind: "removeMod", modId: `${key}.mod` },
      confidence: 85,
      rationale: "not found on disk",
      rationaleCode: { kind: "missingModNotInstalled" },
      alternatives: [],
    },
    status: "needsInput",
    effective: { kind: "removeMod", modId: `${key}.mod` },
    note: null,
    hasDecision: false,
    resolvedBySuggested: null,
    mergeState: null,
    structuralGuardField: null,
    scope: null,
    defRef: null,
  };
}

const DECIDE_RESULT = {
  stats: { auto: 0, needsInput: 0, overridden: 0, resolvedBySuggested: 0 },
  resorted: false,
  movedMods: 0,
};

function mountInboxPage(pinia: ReturnType<typeof createPinia>) {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: "/", name: "inbox", component: { template: "<div />" } },
      { path: "/order/:modId", name: "order-mod", component: { template: "<div />" } },
      { path: "/mods/:modId", name: "mod-detail", component: { template: "<div />" } },
    ],
  });

  return mount(InboxPage, {
    global: {
      plugins: [pinia, PiniaColada, router, [PrimeVue, { theme: { preset: Aura } }], ToastService],
      // The shortcut-help dialog and note editor both render PrimeVue
      // `Dialog`s through a real `<Teleport>`.
      stubs: { teleport: true },
    },
  });
}

async function flush(): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, 0));
}

describe("InboxPage", () => {
  // `@tanstack/vue-virtual` (used by `FindingList`) measures its scroll
  // container via `offsetWidth`/`offsetHeight`, which happy-dom always
  // reports as `0` — stubbed so the virtualizer renders every row instead
  // of an empty visible range.
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

  it("lets finding rows size themselves instead of imposing a fixed height", async () => {
    // A card's height depends on the locale (a wrapped title) and its chips,
    // so a fixed row height would draw one card over the next.
    installMockIpc({
      list_findings: () => ({ total: 2, items: [summary("a"), summary("b")] }),
      get_finding: (payload: unknown) => detailFor((payload as { key: string }).key),
      list_mod_names: {},
    });
    const pinia = createPinia();
    setActivePinia(pinia);
    const wrapper = mountInboxPage(pinia);
    await flush();
    await wrapper.vm.$nextTick();

    const rows = wrapper.findAll('[data-testid="finding-list"] [role="listitem"]');
    expect(rows.length).toBe(2);
    for (const [index, row] of rows.entries()) {
      expect(row.attributes("data-index")).toBe(String(index));
      expect(row.attributes("style")).not.toContain("height");
    }
  });

  it("clamps the cursor when deciding removes the last row it was pointing at", async () => {
    // `list_findings` reads this array live on every call — the same
    // shape a real backend has, where a decision that resolves a finding
    // (under the inbox's default `needsInput` filter) makes it vanish
    // from the very next page fetch.
    let items = [summary("a"), summary("b")];

    installMockIpc({
      list_findings: () => ({ total: items.length, items }),
      get_finding: (payload: unknown) => detailFor((payload as { key: string }).key),
      decide: (payload: unknown) => {
        const decidedKey = (payload as { request: DecideRequestDto }).request.key;
        items = items.filter((item) => item.key !== decidedKey);
        return DECIDE_RESULT;
      },
      // `useModLabel` (via `FindingCard`/`SuggestionPanel`) always queries
      // this — an unregistered command's rejection reaches Vitest as an
      // unhandled rejection, not just this query's own error state, so it
      // needs *some* fixture even though no test here asserts on names.
      list_mod_names: {},
    });

    // Created and activated here (not inside `mountInboxPage`) so the
    // `useInboxStore()` call below, from the test body rather than the
    // component tree, resolves to the exact same store instance the
    // mounted page reads and writes.
    const pinia = createPinia();
    setActivePinia(pinia);
    const wrapper = mountInboxPage(pinia);
    await flush();
    await wrapper.vm.$nextTick();

    // The cursor sits on "b", the last row of a two-row list.
    await wrapper.get('[data-testid="finding-card-b"]').trigger("click");
    const inbox = useInboxStore();
    expect(inbox.cursor).toBe(1);
    await flush();
    await wrapper.vm.$nextTick();

    await wrapper.get('[data-testid="accept-button"]').trigger("click");
    await flush();
    await wrapper.vm.$nextTick();
    await flush();
    await wrapper.vm.$nextTick();

    // Only "a" is left (index 0) — the watch on `items` must have pulled
    // the cursor back from the now out-of-bounds index 1, or the next
    // `get_finding` lookup would silently go stale.
    expect(inbox.cursor).toBe(0);
  });

  it("seeds the mod filter from a ?mod= query param, showing a removable chip", async () => {
    const calls: unknown[] = [];
    installMockIpc({
      list_findings: (payload: unknown) => {
        calls.push(payload);
        return { total: 1, items: [summary("a")] };
      },
      get_finding: () => detailFor("a"),
      list_mod_names: {},
    });

    const pinia = createPinia();
    setActivePinia(pinia);
    const router = createRouter({
      history: createMemoryHistory(),
      routes: [
        { path: "/", name: "inbox", component: { template: "<div />" } },
        { path: "/order/:modId", name: "order-mod", component: { template: "<div />" } },
        { path: "/mods/:modId", name: "mod-detail", component: { template: "<div />" } },
      ],
    });
    await router.push({ path: "/", query: { mod: "fixture.mod" } });
    const wrapper = mount(InboxPage, {
      global: {
        plugins: [
          pinia,
          PiniaColada,
          router,
          [PrimeVue, { theme: { preset: Aura } }],
          ToastService,
        ],
        stubs: { teleport: true },
      },
    });
    await flush();
    await wrapper.vm.$nextTick();

    const chip = wrapper.get('[data-testid="mod-filter-chip"]');
    expect(chip.text()).toContain("fixture.mod");
    expect(
      calls.some(
        (call) => (call as { filter: { modId: string | null } }).filter.modId === "fixture.mod",
      ),
    ).toBe(true);

    await wrapper.get('[data-testid="mod-filter-chip-clear"]').trigger("click");
    await flush();
    await wrapper.vm.$nextTick();

    expect(wrapper.find('[data-testid="mod-filter-chip"]').exists()).toBe(false);
    expect(useInboxStore().filter.modId).toBeNull();
  });

  it("keeps the kind-filter disclosure open across two chip clicks in a row", async () => {
    installMockIpc({
      list_findings: () => ({ total: 1, items: [summary("a")] }),
      get_finding: () => detailFor("a"),
      list_mod_names: {},
    });

    const pinia = createPinia();
    setActivePinia(pinia);
    const wrapper = mountInboxPage(pinia);
    await flush();
    await wrapper.vm.$nextTick();

    function isDisclosureOpen(): boolean {
      // Re-queried (not cached from an earlier `get`) on purpose: a
      // remount replaces the underlying DOM node, and a stale reference
      // to the old one would keep reporting its last state forever.
      return (wrapper.get('[data-testid="kind-filter-disclosure"]').element as HTMLDetailsElement)
        .open;
    }

    await wrapper.get('[data-testid="kind-filter-disclosure"] summary').trigger("click");
    expect(isDisclosureOpen()).toBe(true);

    // Each click below picks a filter combination `list_findings` was
    // never queried with before — a fresh Pinia Colada query key with no
    // cached page yet.
    await wrapper.get('[data-testid="kind-chip-defOverride"]').trigger("click");
    await flush();
    await wrapper.vm.$nextTick();
    expect(isDisclosureOpen()).toBe(true);

    await wrapper.get('[data-testid="kind-chip-patchCollision"]').trigger("click");
    await flush();
    await wrapper.vm.$nextTick();
    expect(isDisclosureOpen()).toBe(true);
  });
});
