import { PiniaColada } from "@pinia/colada";
import { clearMocks } from "@tauri-apps/api/mocks";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import { afterEach, describe, expect, it } from "vitest";
import { createMemoryHistory, createRouter } from "vue-router";

import MergeHeader from "@/components/merge/MergeHeader.vue";
import { installMockIpc } from "@/services/ipc.mock";
import type { MergePreviewDto } from "@/types/generated/MergePreviewDto";

/** So the "Inspect" link's `<RouterLink :to="{ name: 'def', ... }">` can resolve. */
const router = createRouter({
  history: createMemoryHistory(),
  routes: [{ path: "/defs/:defRef", name: "def", component: { template: "<div />" } }],
});

function preview(overrides: Partial<MergePreviewDto> = {}): MergePreviewDto {
  return {
    key: "def_override:ThingDef/Wall:[a.mod,b.mod]",
    defKey: { defType: "ThingDef", defName: "Wall" },
    kind: "defOverride",
    owners: [
      { modId: "a.mod", name: "Mod A", position: 0 },
      { modId: "b.mod", name: "Mod B", position: 1 },
    ],
    base: "a.mod",
    winner: "b.mod",
    totals: { fields: 1, unresolved: 0, conflicts: 0, auto: 1, unchanged: 0 },
    state: { kind: "complete", opCount: 1 },
    structuralGuard: null,
    caveats: [],
    fields: [],
    total: 0,
    resolvedXml: null,
    outOfScopeOwners: [],
    defRef: "ThingDef/Wall",
    ...overrides,
  };
}

/** `useModLabel` (via this component) always queries `list_mod_names` — see the identical note in `InboxPage.test.ts`. */
function mountHeader(previewDto: MergePreviewDto) {
  installMockIpc({ list_mod_names: { "c.mod": "Mod C" } });
  return mount(MergeHeader, {
    props: { preview: previewDto },
    global: { plugins: [createPinia(), PiniaColada, router] },
  });
}

describe("MergeHeader", () => {
  afterEach(() => clearMocks());

  it("links Inspect to the previewed def's own def page", () => {
    const wrapper = mountHeader(preview({ defRef: "ThingDef/Wall" }));
    expect(wrapper.get('[data-testid="merge-header-inspect-link"]').attributes("href")).toBe(
      "/defs/ThingDef%2FWall",
    );
  });

  it("shows no out-of-scope line when outOfScopeOwners is empty", () => {
    const wrapper = mountHeader(preview());
    expect(wrapper.find('[data-testid="merge-header-out-of-scope"]').exists()).toBe(false);
  });

  it("shows the out-of-scope caveat naming every excluded owner by its resolved label", async () => {
    const wrapper = mountHeader(preview({ outOfScopeOwners: ["c.mod"] }));
    await flushPromises();
    expect(wrapper.get('[data-testid="merge-header-out-of-scope"]').text()).toBe(
      "Out of scope, not merged: Mod C",
    );
  });

  it("joins multiple excluded owners through the locale's own list conjunction", async () => {
    const wrapper = mountHeader(preview({ outOfScopeOwners: ["c.mod", "d.mod"] }));
    await flushPromises();
    // `d.mod` has no `list_mod_names` entry, so `useModLabel` falls back to the bare id.
    expect(wrapper.get('[data-testid="merge-header-out-of-scope"]').text()).toBe(
      "Out of scope, not merged: Mod C and d.mod",
    );
  });

  it("shows no structural-guard banner and the ordinary field-count pill when structuralGuard is null", () => {
    const wrapper = mountHeader(
      preview({
        state: { kind: "needsFieldInput", unresolved: 2, total: 9 },
        structuralGuard: null,
      }),
    );
    expect(wrapper.find('[data-testid="merge-header-structural-guard"]').exists()).toBe(false);
    expect(wrapper.get('[data-testid="merge-header-state"]').text()).toBe("2 fields need input");
  });

  // A guarded def with zero real conflicts must never
  // read as "N fields need input" next to a totals line saying 0
  // unresolved. The banner names both the field and the differing
  // owner (one banner naming the field that triggered it);
  // the pill switches to the same "confirm the winner" wording the apply
  // dialog uses for a guarded skip.
  it("shows one banner naming the triggering field and owner, and the pill reads 'confirm the winner' instead of a field count", async () => {
    const wrapper = mountHeader(
      preview({
        totals: { fields: 9, unresolved: 0, conflicts: 0, auto: 9, unchanged: 0 },
        state: { kind: "needsFieldInput", unresolved: 9, total: 9 },
        // `c.mod` is the one id `mountHeader`'s own `list_mod_names` mock
        // resolves to a display name — using it here (rather than one of
        // `preview()`'s own `a.mod`/`b.mod` owners) proves the banner
        // resolves the differing owner through `useModLabel`, not just
        // echoing a raw id.
        structuralGuard: { field: "thingClass", by: "c.mod" },
      }),
    );
    await flushPromises();

    const banner = wrapper.get('[data-testid="merge-header-structural-guard"]');
    expect(banner.text()).toContain("thingClass");
    expect(banner.text()).toContain("Mod C");
    // The banner must point at an action
    // that actually exists on a fully-auto-resolved guarded def (zero
    // `Conflict` rows, nothing to click here) — accepting the finding
    // on the Findings page — never at a "below" this page may have nothing in.
    expect(banner.text()).toContain("Findings page");
    expect(banner.text()).not.toContain("below");
    expect(wrapper.get('[data-testid="merge-header-state"]').text()).toBe(
      "not auto-merged — confirm the winner",
    );
    // The totals line is untouched — still the real, literal per-field
    // counts (the "lesser lie" `rim-session`'s own state_from_plan
    // deliberately keeps); only the pill's own wording differs, so the
    // two never contradict each other.
    expect(wrapper.get('[data-testid="merge-header-totals"]').text()).toContain("0 unresolved");
  });
});
