import { PiniaColada } from "@pinia/colada";
import { clearMocks } from "@tauri-apps/api/mocks";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia, type Pinia } from "pinia";
import { afterEach, describe, expect, it } from "vitest";
import { createMemoryHistory, createRouter } from "vue-router";

import DefConflictView from "@/components/inbox/DefConflictView.vue";
import { installMockIpc } from "@/services/ipc.mock";
import { usePreferencesStore } from "@/stores/preferences";
import type { DefConflictViewRequestDto } from "@/types/generated/DefConflictViewRequestDto";
import type { FieldRowDto } from "@/types/generated/FieldRowDto";
import type { FindingDto } from "@/types/generated/FindingDto";

const router = createRouter({
  history: createMemoryHistory(),
  routes: [{ path: "/defs/:defRef", name: "def", component: { template: "<div />" } }],
});

afterEach(() => clearMocks());

const DEF_OVERRIDE_FINDING: Extract<FindingDto, { kind: "defOverride" }> = {
  kind: "defOverride",
  key: { defType: "ThingDef", defName: "Widget" },
  owners: ["widget.mod.a", "widget.mod.b"],
  winner: "widget.mod.b",
};

/**
 * The list-merge fixture (mirrors `e2e/fixtures/scenario.ts`'s
 * `ThingDef/Widget` and `rim_session::test_support::two_mod_list_fixture`):
 * a `label` conflict plus two mods each adding their own distinct `li`
 * entry to `comps`, `widget.mod.b` loading after `widget.mod.a` (so it
 * wins the conflict) but the two `ListEntry` rows arrive in the def's own
 * final-list order, `Alpha` then `Beta` — not load order, not path order.
 */
function widgetFields(): FieldRowDto[] {
  return [
    {
      path: "label",
      kind: "conflict",
      values: [
        { modId: "widget.mod.a", value: "alpha widget" },
        { modId: "widget.mod.b", value: "beta widget" },
      ],
      agreedBy: [],
      inGame: { modId: "widget.mod.b", value: "beta widget" },
      afterMerge: null,
      preference: { kind: "loadOrder", winner: "widget.mod.b" },
    },
    {
      path: "comps/li[@Class=CompProperties_Alpha]",
      kind: "listEntry",
      values: [{ modId: "widget.mod.a", value: '<li Class="CompProperties_Alpha"/>' }],
      agreedBy: [],
      inGame: { modId: "widget.mod.a", value: '<li Class="CompProperties_Alpha"/>' },
      afterMerge: { modId: "widget.mod.a", value: '<li Class="CompProperties_Alpha"/>' },
      preference: { kind: "none" },
    },
    {
      path: "comps/li[@Class=CompProperties_Beta]",
      kind: "listEntry",
      values: [{ modId: "widget.mod.b", value: '<li Class="CompProperties_Beta"/>' }],
      agreedBy: [],
      inGame: { modId: "widget.mod.b", value: '<li Class="CompProperties_Beta"/>' },
      afterMerge: { modId: "widget.mod.b", value: '<li Class="CompProperties_Beta"/>' },
      preference: { kind: "none" },
    },
  ];
}

/**
 * The "list case" dedup fixture (mirrors `e2e/fixtures/scenario.ts`'s
 * `ThingDef/Sprocket` and `rim_session::test_support::same_identity_agreeing_list_item_fixture`):
 * `dup.mod.a` and `dup.mod.b` independently add the exact same `li` item
 * under a colliding identity — one `ListEntry` row, `dup.mod.a` credited,
 * `dup.mod.b` named only in `agreedBy`.
 */
function dedupedListFields(): FieldRowDto[] {
  return [
    {
      path: "comps/li[#0]",
      kind: "listEntry",
      values: [{ modId: "dup.mod.a", value: '<li Class="CompProperties_Shared"/>' }],
      agreedBy: ["dup.mod.b"],
      inGame: { modId: "dup.mod.a", value: '<li Class="CompProperties_Shared"/>' },
      afterMerge: { modId: "dup.mod.a", value: '<li Class="CompProperties_Shared"/>' },
      preference: { kind: "none" },
    },
  ];
}

/**
 * Every other fixture in this file
 * (and the Rust/mock ones) happens to have `inGame`/`afterMerge` equal —
 * a component bug swapping the two chip bindings would pass every
 * existing test. This one field row pins the real, distinguishing shape:
 * a stored `MergeChoice::From` decision sends `afterMerge` to `widget.mod.a`
 * even though `widget.mod.b` (the load-order winner) is what actually
 * runs in-game today.
 */
function widgetFieldWithStoredChoice(): FieldRowDto[] {
  return [
    {
      path: "label",
      kind: "conflict",
      values: [
        { modId: "widget.mod.a", value: "alpha widget" },
        { modId: "widget.mod.b", value: "beta widget" },
      ],
      agreedBy: [],
      inGame: { modId: "widget.mod.b", value: "beta widget" },
      afterMerge: { modId: "widget.mod.a", value: "alpha widget" },
      preference: {
        kind: "mergeChoice",
        choice: { choice: "from", modId: "widget.mod.a" },
      },
    },
  ];
}

/**
 * Finding 12's own real case:
 * `BiomeDef/AridShrubland`'s `wildAnimals` tag-keyed map, three mods each
 * adding their own disjoint animal plus one shared, agreed-on key
 * (`Cobra`) and one genuine conflict (`Raptor`) — mirrors
 * `e2e/fixtures/scenario.ts`'s own `wildAnimals` fixture. Before this
 * step, `FieldRowKind::MapEntry` rows matched none of `FieldRowsTable`'s
 * three grouping loops and were silently dropped from the table
 * entirely — every assertion below regresses that gap, not just the new
 * grouping/agreed-by behavior.
 */
function wildAnimalsFields(): FieldRowDto[] {
  return [
    {
      path: "wildAnimals/Raptor",
      kind: "conflict",
      values: [
        { modId: "wildlife.mod.a", value: "0.3" },
        { modId: "wildlife.mod.b", value: "0.3" },
        { modId: "wildlife.mod.c", value: "0.5" },
      ],
      agreedBy: [],
      inGame: { modId: "wildlife.mod.c", value: "0.5" },
      afterMerge: null,
      preference: { kind: "loadOrder", winner: "wildlife.mod.c" },
    },
    {
      path: "wildAnimals/Allosaurus",
      kind: "mapEntry",
      values: [{ modId: "wildlife.mod.a", value: "0.6" }],
      agreedBy: [],
      inGame: { modId: "wildlife.mod.a", value: "0.6" },
      afterMerge: { modId: "wildlife.mod.a", value: "0.6" },
      preference: { kind: "none" },
    },
    {
      path: "wildAnimals/Mammoth",
      kind: "mapEntry",
      values: [{ modId: "wildlife.mod.b", value: "0.1" }],
      agreedBy: [],
      inGame: { modId: "wildlife.mod.b", value: "0.1" },
      afterMerge: { modId: "wildlife.mod.b", value: "0.1" },
      preference: { kind: "none" },
    },
    {
      path: "wildAnimals/Cobra",
      kind: "mapEntry",
      values: [{ modId: "wildlife.mod.a", value: "0.2" }],
      agreedBy: ["wildlife.mod.b"],
      inGame: { modId: "wildlife.mod.a", value: "0.2" },
      afterMerge: { modId: "wildlife.mod.a", value: "0.2" },
      preference: { kind: "none" },
    },
  ];
}

/**
 * The unsupported-op fixture (mirrors `ThingDef/Wall`): `wall.c.mod`'s own
 * `description` patch survives (before the stopper), `wall.x.mod` ships
 * the unsupported op, and `wall.d.mod`'s own `label` patch — past the
 * stopper — never ran (`inGame: null`).
 */
function wallFields(): FieldRowDto[] {
  return [
    {
      path: "description",
      kind: "cleanMerge",
      values: [{ modId: "wall.c.mod", value: "a reinforced wall" }],
      agreedBy: [],
      inGame: { modId: "wall.c.mod", value: "a reinforced wall" },
      afterMerge: { modId: "wall.c.mod", value: "a reinforced wall" },
      preference: { kind: "none" },
    },
    {
      path: "label",
      kind: "conflict",
      values: [],
      agreedBy: [],
      inGame: null,
      afterMerge: null,
      preference: { kind: "loadOrder", winner: "wall.d.mod" },
    },
  ];
}

interface ConflictViewOverrides {
  defRef?: string;
  touchers?: unknown[];
  problems?: unknown[];
  completeness?: unknown;
  injectedNodeRelations?: unknown[];
}

/**
 * Mocks `get_def_conflict_view` over a fixed field list, replicating the
 * real command's `onlyChanged`/offset/limit paging so the filter/paging
 * tests exercise real request round-trips instead of a single canned
 * response. Returns the requests seen so far — a live array the caller
 * can keep reading as more requests land.
 */
function installConflictView(
  fields: FieldRowDto[],
  overrides: ConflictViewOverrides = {},
): DefConflictViewRequestDto[] {
  const requests: DefConflictViewRequestDto[] = [];
  installMockIpc({
    list_mod_names: {},
    get_def_conflict_view: (payload: unknown) => {
      const request = (payload as { request: DefConflictViewRequestDto }).request;
      requests.push(request);
      const matching = request.filter.onlyChanged
        ? fields.filter((f) => f.kind !== "unchanged")
        : fields;
      const limit = Math.min(request.filter.limit, 200);
      return {
        defRef: overrides.defRef ?? "ThingDef/Widget",
        kind: { kind: "patchCollision", subPath: "label" },
        touchers: overrides.touchers ?? [
          { modId: "widget.mod.a", position: 0, isGenerated: false, role: "patcher", opCount: 2 },
          { modId: "widget.mod.b", position: 1, isGenerated: false, role: "patcher", opCount: 2 },
        ],
        fields: matching.slice(request.filter.offset, request.filter.offset + limit),
        fieldsTotal: matching.length,
        problems: overrides.problems ?? [],
        effectiveCompleteness: overrides.completeness ?? { kind: "complete" },
        injectedNodeRelations: overrides.injectedNodeRelations ?? [],
      };
    },
  });
  return requests;
}

type ConflictViewFinding = Extract<
  FindingDto,
  { kind: "defOverride" } | { kind: "patchCollision" } | { kind: "duplicateTemplateName" }
>;

function mountView(
  finding: ConflictViewFinding = DEF_OVERRIDE_FINDING,
  pinia: Pinia = createPinia(),
) {
  return mount(DefConflictView, {
    props: { finding, findingKey: "some:key" },
    global: { plugins: [pinia, PiniaColada, router] },
  });
}

describe("DefConflictView", () => {
  it("shows a conflict row's per-mod values and the load-order winner", async () => {
    installConflictView(widgetFields());
    const wrapper = mountView();
    await flushPromises();

    expect(wrapper.get('[data-testid="def-conflict-view-def-ref"]').text()).toBe("ThingDef/Widget");
    const conflictRow = wrapper
      .findAll('[data-testid="def-conflict-field-row"]')
      .find((row) => row.attributes("data-path") === "label");
    expect(conflictRow).toBeDefined();
    expect(conflictRow?.text()).toContain("widget.mod.a");
    expect(conflictRow?.text()).toContain("widget.mod.b");
    expect(conflictRow?.get('[data-testid="def-conflict-preference"]').text()).toContain(
      "widget.mod.b wins (load order)",
    );
  });

  it("shows a stored merge choice's after-merge value distinct from what runs in-game today", async () => {
    installConflictView(widgetFieldWithStoredChoice());
    const wrapper = mountView();
    await flushPromises();

    const row = wrapper.get('[data-testid="def-conflict-field-row"]');
    const inGame = row.get('[data-testid="def-conflict-in-game"]').text();
    const afterMerge = row.get('[data-testid="def-conflict-after-merge"]').text();

    expect(inGame).not.toBe(afterMerge);
    expect(inGame).toContain("widget.mod.b");
    expect(inGame).toContain("beta widget");
    expect(afterMerge).toContain("widget.mod.a");
    expect(afterMerge).toContain("alpha widget");
    expect(row.get('[data-testid="def-conflict-preference"]').text()).toContain("merge choice");
  });

  it("shows a list-merge's two entries, each with its own mod, in final order", async () => {
    installConflictView(widgetFields());
    const wrapper = mountView();
    await flushPromises();

    const rows = wrapper.findAll('[data-testid="def-conflict-field-row"]');
    const listRows = rows.filter((row) => row.attributes("data-path")?.startsWith("comps/li["));
    expect(listRows).toHaveLength(2);
    expect(listRows[0]?.attributes("data-path")).toBe("comps/li[@Class=CompProperties_Alpha]");
    expect(listRows[0]?.text()).toContain("widget.mod.a");
    expect(listRows[1]?.attributes("data-path")).toBe("comps/li[@Class=CompProperties_Beta]");
    expect(listRows[1]?.text()).toContain("widget.mod.b");
    expect(wrapper.get('[data-testid="def-conflict-group-header"]').text()).toContain("comps");
  });

  it("folds two mods adding the exact same list item into one agreed-on row", async () => {
    installConflictView(dedupedListFields(), { defRef: "ThingDef/Sprocket" });
    const wrapper = mountView();
    await flushPromises();

    const rows = wrapper.findAll('[data-testid="def-conflict-field-row"]');
    expect(rows).toHaveLength(1);
    const row = rows[0];
    expect(row?.attributes("data-path")).toBe("comps/li[#0]");
    expect(row?.text()).toContain("dup.mod.a");

    const agreedBy = row?.get('[data-testid="def-conflict-agreed-by"]');
    expect(agreedBy?.text()).toContain("dup.mod.b");
    // Never a second, indistinguishable-looking row for the agreeing mod.
    expect(wrapper.findAll('[data-path="comps/li[#1]"]')).toHaveLength(0);
  });

  // -- keyed-map entries --

  it("groups a keyed map's own uncontested keys under one container header, alongside its real conflict", async () => {
    installConflictView(wildAnimalsFields(), { defRef: "BiomeDef/AridShrubland" });
    const wrapper = mountView();
    await flushPromises();

    const rows = wrapper.findAll('[data-testid="def-conflict-field-row"]');
    // Grouping includes `mapEntry` rows: without it, none of the three map
    // keys below would render — only the `Raptor` conflict row would show
    // up here.
    expect(rows).toHaveLength(4);

    const mapRows = rows.filter((row) => row.attributes("data-path") !== "wildAnimals/Raptor");
    expect(mapRows).toHaveLength(3);
    expect(mapRows.every((row) => row.text().includes("Map entry"))).toBe(true);
    expect(wrapper.get('[data-testid="def-conflict-group-header"]').text()).toContain(
      "wildAnimals",
    );
    expect(wrapper.get('[data-testid="def-conflict-group-header"]').text()).toContain("3 entries");

    const cobraRow = rows.find((row) => row.attributes("data-path") === "wildAnimals/Cobra");
    expect(cobraRow?.get('[data-testid="def-conflict-agreed-by"]').text()).toContain(
      "wildlife.mod.b",
    );

    const raptorRow = rows.find((row) => row.attributes("data-path") === "wildAnimals/Raptor");
    expect(raptorRow?.text()).toContain("wildlife.mod.c wins (load order)");
  });

  it("the mapEntry kind filter narrows to just the keyed map's own uncontested keys", async () => {
    installConflictView(wildAnimalsFields(), { defRef: "BiomeDef/AridShrubland" });
    const wrapper = mountView();
    await flushPromises();

    await wrapper.get('[data-testid="def-conflict-view-filter"]').setValue("mapEntry");
    await flushPromises();

    const rows = wrapper.findAll('[data-testid="def-conflict-field-row"]');
    expect(rows).toHaveLength(3);
    expect(rows.every((row) => row.attributes("data-path")?.startsWith("wildAnimals/"))).toBe(true);
    expect(wrapper.findAll('[data-path="wildAnimals/Raptor"]')).toHaveLength(0);
  });

  it("an unsupported op shows the problem and marks the toucher past it as not reached", async () => {
    installConflictView(wallFields(), {
      defRef: "ThingDef/Wall",
      touchers: [
        { modId: "wall.c.mod", position: 0, isGenerated: false, role: "patcher", opCount: 1 },
        { modId: "wall.x.mod", position: 1, isGenerated: false, role: "patcher", opCount: 1 },
        { modId: "wall.d.mod", position: 2, isGenerated: false, role: "patcher", opCount: 1 },
      ],
      problems: [
        {
          kind: "unsupportedOp",
          modId: "wall.x.mod",
          opIndex: 1,
          class: "SomeThirdParty.WeirdOperation",
          xpath: 'Defs/ThingDef[defName="Wall"]/fillPercent',
          reason: "unsupported operation class SomeThirdParty.WeirdOperation",
        },
      ],
      completeness: {
        kind: "partial",
        stoppedAt: {
          kind: "replay",
          modId: "wall.x.mod",
          opIndex: 1,
          error: "unsupported operation class SomeThirdParty.WeirdOperation",
        },
      },
    });
    const wrapper = mountView({
      kind: "patchCollision",
      key: { defType: "ThingDef", defName: "Wall" },
      selector: "defName",
      subPath: "label",
      mods: ["wall.c.mod", "wall.d.mod"],
    });
    await flushPromises();

    expect(wrapper.get('[data-testid="def-conflict-view-completeness"]').text()).toBe("Partial");
    const problem = wrapper.get('[data-testid="def-conflict-problem-row"]');
    expect(problem.text()).toContain("wall.x.mod");
    expect(problem.text()).toContain("SomeThirdParty.WeirdOperation");

    const reachedCells = wrapper.findAll('[data-testid="def-conflict-toucher-reached"]');
    const rows = wrapper.findAll('[data-testid="def-conflict-toucher-row"]');
    const dModRow = rows.find((row) => row.attributes("data-mod-id") === "wall.d.mod");
    const cModRow = rows.find((row) => row.attributes("data-mod-id") === "wall.c.mod");
    expect(dModRow?.get('[data-testid="def-conflict-toucher-reached"]').text()).toBe("not reached");
    expect(cModRow?.get('[data-testid="def-conflict-toucher-reached"]').text()).toBe("reached");
    expect(reachedCells.length).toBe(rows.length);

    const labelRow = wrapper
      .findAll('[data-testid="def-conflict-field-row"]')
      .find((row) => row.attributes("data-path") === "label");
    expect(labelRow?.text()).toContain("—");
  });

  it("a plan-failed problem (a losing owner's own broken chain) renders its reason", async () => {
    // `Problem::PlanFailed` is the variant `problem_from_plan_failure`
    // falls back to for anything that isn't a structured `InheritError`
    // gap — this pins that the panel actually renders it, not just the
    // other two kinds.
    installConflictView(widgetFields(), {
      problems: [
        {
          kind: "planFailed",
          reason: "the source index has no record of ReallyMissing",
        },
      ],
    });
    const wrapper = mountView();
    await flushPromises();

    const problem = wrapper.get('[data-testid="def-conflict-problem-row"]');
    expect(problem.text()).toContain("the source index has no record of ReallyMissing");
  });

  it("a complete view with a plan-failed problem shows a warning pill, not a plain Complete", async () => {
    // `effectiveCompleteness.kind`
    // stays `"complete"` for exactly this shape (a losing owner's broken
    // chain never downgrades the winner's own fold), so the pill must
    // fold `problems` in too or it reads as if nothing were wrong.
    installConflictView(widgetFields(), {
      completeness: { kind: "complete" },
      problems: [{ kind: "planFailed", reason: "the source index has no record of ReallyMissing" }],
    });
    const wrapper = mountView();
    await flushPromises();

    const pill = wrapper.get('[data-testid="def-conflict-view-completeness"]');
    expect(pill.text()).toBe("Complete, with problems");
    expect(pill.classes()).toContain("text-status-input");
    expect(pill.classes()).not.toContain("text-status-auto");
  });

  it("shows no injected-node-relations section when the view carries none", async () => {
    installConflictView(widgetFields());
    const wrapper = mountView();
    await flushPromises();

    expect(wrapper.find('[data-testid="def-conflict-view-injected-node-relations"]').exists()).toBe(
      false,
    );
  });

  it("names the selector and injector for a patch-selects-injected-node relation", async () => {
    installConflictView(widgetFields(), {
      injectedNodeRelations: [
        {
          selector: "widget.mod.a",
          injector: "widget.mod.b",
          subject: "ThingDef/Widget/comps",
        },
      ],
    });
    const wrapper = mountView();
    await flushPromises();

    const row = wrapper.get('[data-testid="def-conflict-injected-node-relation-row"]');
    expect(row.text()).toContain("widget.mod.a");
    expect(row.text()).toContain("widget.mod.b");
    expect(row.text()).toContain("ThingDef/Widget/comps");
  });

  it("the kind filter narrows the visible rows", async () => {
    installConflictView(widgetFields());
    const wrapper = mountView();
    await flushPromises();

    expect(wrapper.findAll('[data-testid="def-conflict-field-row"]')).toHaveLength(3);

    await wrapper.get('[data-testid="def-conflict-view-filter"]').setValue("conflict");
    await flushPromises();
    let rows = wrapper.findAll('[data-testid="def-conflict-field-row"]');
    expect(rows).toHaveLength(1);
    expect(rows[0]?.attributes("data-path")).toBe("label");

    await wrapper.get('[data-testid="def-conflict-view-filter"]').setValue("listEntry");
    await flushPromises();
    rows = wrapper.findAll('[data-testid="def-conflict-field-row"]');
    expect(rows).toHaveLength(2);
    expect(rows.every((row) => row.attributes("data-path")?.startsWith("comps/li["))).toBe(true);
  });

  it("selecting the unchanged filter refetches with onlyChanged: false", async () => {
    const fields: FieldRowDto[] = [
      ...widgetFields(),
      {
        path: "defName",
        kind: "unchanged",
        values: [],
        agreedBy: [],
        inGame: { modId: "widget.mod.a", value: "Widget" },
        afterMerge: null,
        preference: { kind: "none" },
      },
    ];
    const requests = installConflictView(fields);
    const wrapper = mountView();
    await flushPromises();
    expect(requests.every((request) => request.filter.onlyChanged)).toBe(true);

    await wrapper.get('[data-testid="def-conflict-view-filter"]').setValue("unchanged");
    await flushPromises();

    expect(requests.some((request) => !request.filter.onlyChanged)).toBe(true);
    const rows = wrapper.findAll('[data-testid="def-conflict-field-row"]');
    expect(rows).toHaveLength(1);
    expect(rows[0]?.attributes("data-path")).toBe("defName");
  });

  it("pages beyond the first 200 rows via show more", async () => {
    const bigFields: FieldRowDto[] = Array.from({ length: 210 }, (_, index) => ({
      path: `field${index}`,
      kind: "cleanMerge" as const,
      values: [{ modId: "widget.mod.a", value: `v${index}` }],
      agreedBy: [],
      inGame: { modId: "widget.mod.a", value: `v${index}` },
      afterMerge: { modId: "widget.mod.a", value: `v${index}` },
      preference: { kind: "none" as const },
    }));
    installConflictView(bigFields);
    const wrapper = mountView();
    await flushPromises();

    expect(wrapper.findAll('[data-testid="def-conflict-field-row"]')).toHaveLength(200);
    const showMore = wrapper.get('[data-testid="def-conflict-view-show-more"]');
    expect(showMore.text()).toContain("200 of 210");

    await showMore.trigger("click");
    await flushPromises();

    expect(wrapper.findAll('[data-testid="def-conflict-field-row"]')).toHaveLength(210);
    expect(wrapper.find('[data-testid="def-conflict-view-show-more"]').exists()).toBe(false);
  });

  it("reacts to the shared mod-label-mode toggle", async () => {
    installMockIpc({
      list_mod_names: { "widget.mod.a": "Widget Mod Alpha", "widget.mod.b": "Widget Mod Beta" },
      get_def_conflict_view: () => ({
        defRef: "ThingDef/Widget",
        kind: { kind: "patchCollision", subPath: "label" },
        touchers: [
          { modId: "widget.mod.a", position: 0, isGenerated: false, role: "patcher", opCount: 2 },
          { modId: "widget.mod.b", position: 1, isGenerated: false, role: "patcher", opCount: 2 },
        ],
        fields: widgetFields(),
        fieldsTotal: 3,
        problems: [],
        effectiveCompleteness: { kind: "complete" },
        injectedNodeRelations: [],
      }),
    });
    const pinia = createPinia();
    const wrapper = mountView(DEF_OVERRIDE_FINDING, pinia);
    await flushPromises();

    expect(wrapper.get('[data-testid="def-conflict-touchers"]').text()).toContain(
      "Widget Mod Alpha",
    );

    usePreferencesStore(pinia).toggleModLabelMode();
    await flushPromises();

    expect(wrapper.get('[data-testid="def-conflict-touchers"]').text()).toContain("widget.mod.a");
    expect(wrapper.get('[data-testid="def-conflict-touchers"]').text()).not.toContain(
      "Widget Mod Alpha",
    );
  });

  it("shows a finding's fields again after switching away and back within the cache's staleTime", async () => {
    // `SuggestionPanel` reuses this component across findings instead of
    // remounting it — a `findingKey` prop change alone, via `setProps`,
    // mirrors that. Each key gets its own distinct `defRef`/fields so a
    // stale "still showing Widget while pointed at Sprocket" bug would be
    // visible, not masked by identical fixtures.
    const byKey: Record<string, { defRef: string; fields: FieldRowDto[] }> = {
      "widget:key": { defRef: "ThingDef/Widget", fields: widgetFields() },
      "sprocket:key": { defRef: "ThingDef/Sprocket", fields: dedupedListFields() },
    };
    installMockIpc({
      list_mod_names: {},
      get_def_conflict_view: (payload: unknown) => {
        const request = (payload as { request: DefConflictViewRequestDto }).request;
        const fixture = byKey[request.key];
        if (!fixture) {
          throw new Error(`unexpected key: ${request.key}`);
        }
        return {
          defRef: fixture.defRef,
          kind: { kind: "patchCollision", subPath: "label" },
          touchers: [
            { modId: "widget.mod.a", position: 0, isGenerated: false, role: "patcher", opCount: 2 },
            { modId: "widget.mod.b", position: 1, isGenerated: false, role: "patcher", opCount: 2 },
          ],
          fields: fixture.fields,
          fieldsTotal: fixture.fields.length,
          problems: [],
          effectiveCompleteness: { kind: "complete" },
          injectedNodeRelations: [],
        };
      },
    });

    const wrapper = mount(DefConflictView, {
      props: { finding: DEF_OVERRIDE_FINDING, findingKey: "widget:key" },
      global: { plugins: [createPinia(), PiniaColada, router] },
    });
    await flushPromises();
    expect(wrapper.get('[data-testid="def-conflict-view-def-ref"]').text()).toBe("ThingDef/Widget");
    expect(wrapper.findAll('[data-testid="def-conflict-field-row"]').length).toBeGreaterThan(0);

    await wrapper.setProps({ findingKey: "sprocket:key" });
    await flushPromises();
    expect(wrapper.get('[data-testid="def-conflict-view-def-ref"]').text()).toBe(
      "ThingDef/Sprocket",
    );

    // Back to the first finding within its 5s `staleTime` — Pinia Colada
    // serves the cached "widget:key" entry synchronously here, the exact
    // ordering hazard `usePagedRows`'s own doc comment describes.
    await wrapper.setProps({ findingKey: "widget:key" });
    await flushPromises();

    expect(wrapper.get('[data-testid="def-conflict-view-def-ref"]').text()).toBe("ThingDef/Widget");
    expect(wrapper.find('[data-testid="def-conflict-fields-empty"]').exists()).toBe(false);
    expect(wrapper.findAll('[data-testid="def-conflict-field-row"]').length).toBeGreaterThan(0);
  });
});
