// The router, detail builders, and mount helper shared by the SuggestionPanel test files.

import { PiniaColada } from "@pinia/colada";
import Aura from "@primevue/themes/aura";
import { mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import PrimeVue from "primevue/config";
import type { Plugin } from "vue";
import { createMemoryHistory, createRouter } from "vue-router";
import SuggestionPanel from "@/components/inbox/SuggestionPanel.vue";
import type { DefConflictViewDto } from "@/types/generated/DefConflictViewDto";
import type { FindingDto } from "@/types/generated/FindingDto";
import type { ResolutionDetailDto } from "@/types/generated/ResolutionDetailDto";

/** A memory-history router just so `DefConflictView`'s own "def ref" `<RouterLink>` can resolve. */
export const router = createRouter({
  history: createMemoryHistory(),
  routes: [
    { path: "/inbox", name: "inbox", component: { template: "<div />" } },
    { path: "/merge/:key", name: "merge-editor", component: { template: "<div />" } },
    {
      path: "/patches/:patchId/merge/:key",
      name: "patch-merge-editor",
      component: { template: "<div />" },
    },
    { path: "/defs/:defRef", name: "def", component: { template: "<div />" } },
  ],
});

export function buildDetail(
  finding: FindingDto,
  overrides: Partial<ResolutionDetailDto> = {},
): ResolutionDetailDto {
  return {
    key: "some:key",
    finding,
    suggestion: {
      action: { kind: "accept" },
      confidence: 90,
      rationale: "because reasons",
      rationaleCode: { kind: "keepCurrentWinner" },
      alternatives: [],
    },
    status: "needsInput",
    effective: { kind: "accept" },
    note: null,
    hasDecision: false,
    resolvedBySuggested: null,
    mergeState: null,
    structuralGuardField: null,
    scope: null,
    defRef: null,
    ...overrides,
  };
}

/**
 * Every mount needs the PrimeVue plugin (`SuggestionPanel`/`DecisionNote`
 * use `Button`/`Textarea`, which read injected PrimeVue config) and
 * Pinia + Pinia Colada (`useModLabel`). `extraPlugins` is appended last —
 * `@vue/test-utils` installs `config.global.plugins` (the app-wide `en`
 * i18n instance from `i18n/test-setup.ts`) before a mount's own `global.
 * plugins`, so a caller passing its own i18n instance here (see
 * `SuggestionPanel.i18n.test.ts`) installs after it and its `provide()`
 * for the same injection key wins.
 */
export function mountPanel(
  detail: ResolutionDetailDto,
  props: Record<string, unknown> = {},
  extraPlugins: Plugin[] = [],
) {
  return mount(SuggestionPanel, {
    props: { detail, ...props },
    global: {
      plugins: [
        createPinia(),
        PiniaColada,
        [PrimeVue, { theme: { preset: Aura } }],
        router,
        ...extraPlugins,
      ],
    },
  });
}

/** A minimal `DefConflictViewDto` — `DefConflictView` renders `defOverride`/`patchCollision`/`duplicateTemplateName` in both the profile inbox and a compat patch's own, so every test in this file mocks `get_def_conflict_view` (never `get_merge_preview`) for these three kinds. */
export function buildConflictView(overrides: Partial<DefConflictViewDto> = {}): DefConflictViewDto {
  return {
    defRef: "ThingDef/Wall",
    kind: { kind: "defOverride" },
    touchers: [],
    fields: [],
    fieldsTotal: 0,
    problems: [],
    effectiveCompleteness: { kind: "complete" },
    injectedNodeRelations: [],
    ...overrides,
  };
}
