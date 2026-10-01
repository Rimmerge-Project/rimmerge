import { PiniaColada } from "@pinia/colada";
import { clearMocks } from "@tauri-apps/api/mocks";
import { mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import { afterEach, describe, expect, it } from "vitest";

import FindingCard from "@/components/inbox/FindingCard.vue";
import { installMockIpc } from "@/services/ipc.mock";
import type { ResolutionSummaryDto } from "@/types/generated/ResolutionSummaryDto";

const resolution: ResolutionSummaryDto = {
  key: "missing_mod:a.mod",
  finding: { kind: "missingMod", modId: "a.mod" },
  status: "needsInput",
  confidence: 85,
  effective: { kind: "removeMod", modId: "a.mod" },
  hasDecision: false,
  mergeState: null,
  structuralGuardField: null,
  scope: null,
  defRef: null,
};

/** Every mount needs Pinia + Pinia Colada: `useModLabel` reads the preferences store and the `list_mod_names` query. */
function mountCard(props: InstanceType<typeof FindingCard>["$props"]) {
  return mount(FindingCard, {
    props,
    global: { plugins: [createPinia(), PiniaColada] },
  });
}

async function flush(): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, 0));
}

describe("FindingCard", () => {
  afterEach(() => {
    clearMocks();
  });

  it("renders the key, status, effective action, and confidence", () => {
    installMockIpc({ list_mod_names: {} });
    const wrapper = mountCard({ resolution });

    expect(wrapper.text()).toContain("missing_mod:a.mod");
    expect(wrapper.text()).toContain("Remove a.mod");
    expect(wrapper.get('[data-testid="status-pill-needsInput"]').text()).toBe("Needs input");
    expect(wrapper.text()).toContain("85%");
  });

  it("shows the mod's display name, with the id in the title, once the names map resolves", async () => {
    installMockIpc({ list_mod_names: { "a.mod": "A Mod" } });
    const wrapper = mountCard({ resolution });
    await flush();
    await wrapper.vm.$nextTick();

    const actionSpan = wrapper.find("span.font-medium");
    expect(actionSpan.text()).toBe("Remove A Mod from the active list");
    expect(actionSpan.attributes("title")).toBe("a.mod");
  });

  it("shows a 'decided' chip only when hasDecision is true", () => {
    installMockIpc({ list_mod_names: {} });
    const decided = mountCard({ resolution: { ...resolution, hasDecision: true } });
    expect(decided.find('[data-testid="finding-card-decided-chip"]').exists()).toBe(true);

    const undecided = mountCard({ resolution });
    expect(undecided.find('[data-testid="finding-card-decided-chip"]').exists()).toBe(false);
  });

  it("shows the resolved-by-suggested chip only when the prop is true", () => {
    installMockIpc({ list_mod_names: {} });
    const wrapper = mountCard({ resolution, resolvedBySuggested: true });
    expect(wrapper.find('[data-testid="resolved-by-suggested-chip"]').exists()).toBe(true);
  });

  it("emits select when clicked", async () => {
    installMockIpc({ list_mod_names: {} });
    const wrapper = mountCard({ resolution });
    await wrapper.trigger("click");
    expect(wrapper.emitted("select")).toHaveLength(1);
  });

  it("renders no merge-state pill when mergeState is null", () => {
    installMockIpc({ list_mod_names: {} });
    const wrapper = mountCard({ resolution });
    expect(wrapper.find('[data-testid="merge-state-pill"]').exists()).toBe(false);
  });

  it("shows 'merged' for a complete merge state", () => {
    installMockIpc({ list_mod_names: {} });
    const wrapper = mountCard({
      resolution: { ...resolution, mergeState: { kind: "complete", opCount: 3 } },
    });
    expect(wrapper.get('[data-testid="merge-state-pill"]').text()).toBe("merged");
  });

  it("shows the unresolved count for a needsFieldInput merge state", () => {
    installMockIpc({ list_mod_names: {} });
    const wrapper = mountCard({
      resolution: {
        ...resolution,
        mergeState: { kind: "needsFieldInput", unresolved: 3, total: 10 },
      },
    });
    expect(wrapper.get('[data-testid="merge-state-pill"]').text()).toBe("3 fields need input");
  });

  // A pre-existing
  // `Merge` decision on a def that has since become guarded must read
  // the same "confirm the winner" wording the merge editor/apply dialog
  // use, never "N fields need input" for a merge that can never
  // complete.
  it("shows 'confirm the winner' for a guarded needsFieldInput merge state, not a field count", () => {
    installMockIpc({ list_mod_names: {} });
    const wrapper = mountCard({
      resolution: {
        ...resolution,
        mergeState: { kind: "needsFieldInput", unresolved: 9, total: 9 },
        structuralGuardField: "thingClass",
      },
    });
    expect(wrapper.get('[data-testid="merge-state-pill"]').text()).toBe(
      "not auto-merged — confirm the winner",
    );
  });

  it("shows 'cannot merge' for a cannotMerge merge state", () => {
    installMockIpc({ list_mod_names: {} });
    const wrapper = mountCard({
      resolution: { ...resolution, mergeState: { kind: "cannotMerge", reason: "unsupported" } },
    });
    expect(wrapper.get('[data-testid="merge-state-pill"]').text()).toBe("cannot merge");
  });

  it("renders no scope badge when scope is null (the profile inbox)", () => {
    installMockIpc({ list_mod_names: {} });
    const wrapper = mountCard({ resolution });
    expect(wrapper.find('[data-testid="scope-badge"]').exists()).toBe(false);
  });

  it("renders no scope badge when scope is full", () => {
    installMockIpc({ list_mod_names: {} });
    const wrapper = mountCard({ resolution: { ...resolution, scope: { kind: "full" } } });
    expect(wrapper.find('[data-testid="scope-badge"]').exists()).toBe(false);
  });

  it("renders the scope badge naming the out-of-scope owners when scope is partial", () => {
    installMockIpc({ list_mod_names: {} });
    const wrapper = mountCard({
      resolution: { ...resolution, scope: { kind: "partial", outside: ["c.mod"] } },
    });
    expect(wrapper.get('[data-testid="scope-badge"]').text()).toContain("c.mod");
  });

  it("shows the finding itself, plus an 'Undecided' label, for an undecided scoped finding — never the patch's Ignore rewrite", () => {
    installMockIpc({ list_mod_names: {} });
    const wrapper = mountCard({
      resolution: {
        ...resolution,
        key: "def_override:ThingDef/Fixture_Wall:[a.mod,b.mod]",
        finding: {
          kind: "defOverride",
          key: { defType: "ThingDef", defName: "Fixture_Wall" },
          owners: ["a.mod", "b.mod"],
          winner: "b.mod",
        },
        scope: { kind: "full" },
        hasDecision: false,
        effective: { kind: "ignore" },
      },
    });

    expect(wrapper.get('[data-testid="card-action-text"]').text()).toBe("ThingDef/Fixture_Wall");
    expect(wrapper.get('[data-testid="card-undecided-label"]').text()).toBe("Undecided");
  });

  it("shows the decision, not the finding, once a scoped finding is decided", () => {
    installMockIpc({ list_mod_names: {} });
    const wrapper = mountCard({
      resolution: {
        ...resolution,
        key: "def_override:ThingDef/Fixture_Wall:[a.mod,b.mod]",
        scope: { kind: "full" },
        hasDecision: true,
        effective: {
          kind: "merge",
          key: { defType: "ThingDef", defName: "Fixture_Wall" },
          choices: {},
        },
      },
    });

    expect(wrapper.get('[data-testid="card-action-text"]').text()).toBe(
      "Merge ThingDef/Fixture_Wall",
    );
    expect(wrapper.find('[data-testid="card-undecided-label"]').exists()).toBe(false);
  });

  it("never shows the 'Undecided' label in the profile inbox (scope null), even though effective is a real, non-ignore action", () => {
    installMockIpc({ list_mod_names: {} });
    const wrapper = mountCard({ resolution });
    expect(wrapper.find('[data-testid="card-undecided-label"]').exists()).toBe(false);
  });

  it("describes the finding, not the bare 'Accept' action, for a runtime-patch collision still at its suggested Accept", () => {
    installMockIpc({ list_mod_names: {} });
    const wrapper = mountCard({
      resolution: {
        ...resolution,
        key: "runtime_patch_collision:Verse.Pawn:Kill:[a.mod,b.mod,c.mod]",
        finding: {
          kind: "runtimePatchCollision",
          targetType: "Verse.Pawn",
          targetMethod: "Kill",
          owners: ["a.mod", "b.mod", "c.mod"],
        },
        effective: { kind: "accept" },
      },
    });

    expect(wrapper.get('[data-testid="card-action-text"]').text()).toBe(
      "Verse.Pawn.Kill patched by 3 mods",
    );
  });

  it("shows the decision, not the finding, once a runtime-patch collision is decided", () => {
    installMockIpc({ list_mod_names: {} });
    const wrapper = mountCard({
      resolution: {
        ...resolution,
        key: "runtime_patch_collision:Verse.Pawn:Kill:[a.mod,b.mod,c.mod]",
        hasDecision: true,
        effective: {
          kind: "preferWinner",
          key: { defType: "runtime_target", defName: "Verse.Pawn::Kill" },
          winner: "a.mod",
        },
      },
    });

    expect(wrapper.get('[data-testid="card-action-text"]').text()).toBe(
      "Prefer a.mod's runtime_target/Verse.Pawn::Kill",
    );
  });

  it("describes a rule-overruled finding by its own before/after, not 'Accept'", () => {
    installMockIpc({ list_mod_names: {} });
    const wrapper = mountCard({
      resolution: {
        ...resolution,
        key: "rule_overruled:a.mod:b.mod:steam_db",
        finding: {
          kind: "ruleOverruled",
          after: "a.mod",
          before: "b.mod",
          origin: "steamDb",
          comment: null,
          winner: null,
          witnessCycle: [],
        },
        effective: { kind: "accept" },
      },
    });

    expect(wrapper.get('[data-testid="card-action-text"]').text()).toBe(
      "Rule b.mod before a.mod overruled",
    );
  });
});

describe("FindingCard action title", () => {
  afterEach(() => {
    clearMocks();
  });

  /**
   * A bare `Accept` names no mod of its own, so `primaryModIdOfAction`
   * returns `null` and the row would otherwise have no hover text at
   * all — every `patch_collision` card in the inbox reads just "Accept".
   */
  it("falls back to the finding's description when the action names no mod", () => {
    installMockIpc({ list_mod_names: {} });
    const wrapper = mountCard({
      resolution: {
        ...resolution,
        key: "patch_collision:ThingDef/Wall10:Contested:none:[mod.011,mod.011]",
        finding: {
          kind: "patchCollision",
          key: { defType: "ThingDef", defName: "Wall10" },
          selector: "defName",
          subPath: null,
          mods: ["mod.011"],
        },
        effective: { kind: "accept" },
      },
    });

    expect(wrapper.get('[data-testid="card-action-text"]').text()).toBe("Accept");
    expect(wrapper.get('[data-testid="card-action-text"]').attributes("title")).toBe(
      "ThingDef/Wall10",
    );
  });

  /**
   * The kinds `describesFindingWhenAccepted` covers already *render* the
   * description as the row's own text, so a title repeating it would be
   * noise — pinned so the fallback above stays scoped to rows that need it.
   */
  it("adds no title when the description is already the visible text", () => {
    installMockIpc({ list_mod_names: {} });
    const wrapper = mountCard({
      resolution: {
        ...resolution,
        key: "runtime_patch_collision:Verse.Pawn.Kill",
        finding: {
          kind: "runtimePatchCollision",
          targetType: "Verse.Pawn",
          targetMethod: "Kill",
          owners: ["a.mod", "b.mod", "c.mod"],
        },
        effective: { kind: "accept" },
      },
    });

    expect(wrapper.get('[data-testid="card-action-text"]').text()).toBe(
      "Verse.Pawn.Kill patched by 3 mods",
    );
    expect(wrapper.get('[data-testid="card-action-text"]').attributes("title")).toBeUndefined();
  });
});
