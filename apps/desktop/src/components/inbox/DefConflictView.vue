<script setup lang="ts">
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";

import FieldRowsTable from "@/components/inbox/FieldRowsTable.vue";
import TouchersList from "@/components/inbox/TouchersList.vue";
import { useModLabel } from "@/composables/useModLabel";
import { usePagedRows } from "@/composables/usePagedRows";
import { useDefConflictViewQuery } from "@/queries/defs";
import { RimmergeError } from "@/services/ipc";
import { asFindingKey } from "@/types/brands";
import type { DefConflictKindDto } from "@/types/generated/DefConflictKindDto";
import type { FieldRowDto } from "@/types/generated/FieldRowDto";
import type { FieldRowFilterDto } from "@/types/generated/FieldRowFilterDto";
import type { FieldRowKindDto } from "@/types/generated/FieldRowKindDto";
import type { FindingDto } from "@/types/generated/FindingDto";
import { assertNever } from "@/utils/assertNever";
import { inspectRoute } from "@/utils/format";

/**
 * The field-by-field conflict view for a
 * `defOverride`/`patchCollision`/`duplicateTemplateName`
 * finding, fetched through `get_def_conflict_view`. Used for these three
 * kinds in *both* the profile inbox and a compat patch's own (`patchId`
 * set) — `get_def_conflict_view` shares `get_merge_preview`'s `patchId`
 * scoping, so `SuggestionPanel` never picks between this and
 * `ChangeSummary` by context.
 * The existing decision buttons (accept/ignore/revert/alternatives) and
 * the "Inspect" link to `/defs/:defRef` both stay in `SuggestionPanel`'s
 * own template, unchanged — this component owns only the header,
 * touchers, fields, and problems sections.
 */
// `finding` isn't read here — every field this panel shows comes from
// `get_def_conflict_view` itself, keyed by `findingKey` alone. The prop
// still exists so `SuggestionPanel`'s own `v-if`/`v-else-if` narrowing on
// `finding.kind` is what selects this component in the first place.
const { findingKey, patchId = null } = defineProps<{
  finding: Extract<
    FindingDto,
    { kind: "defOverride" } | { kind: "patchCollision" } | { kind: "duplicateTemplateName" }
  >;
  findingKey: string;
  /** Scopes the view to a compat patch's own preview/decisions — mirrors `ChangeSummary`'s own `patchId` doc comment. `null` (the default) for the profile inbox. */
  patchId?: string | null;
}>();

/** Matches `rim_session::MAX_PAGE_SIZE` — the server caps `filter.limit` to this. */
const PAGE_SIZE = 200;

const { t } = useI18n();
const modLabel = useModLabel();
const key = computed(() => asFindingKey(findingKey));

/**
 * `"changed"` (the default) hides `Unchanged` rows server-side, matching
 * `FieldRowFilterDto`'s own default — the other five options each narrow
 * to one `FieldRowKindDto`, client-side except `"unchanged"` itself
 * (which needs `onlyChanged: false` to be fetched at all, since the
 * server never sends an `Unchanged` row otherwise).
 */
type KindFilterOption = "changed" | FieldRowKindDto;
const kindFilter = ref<KindFilterOption>("changed");
const offset = ref(0);

/**
 * Read directly off `kindFilter`, never off `filter.value.onlyChanged` —
 * `filter` also depends on `offset`, so reading through it here would make
 * `onlyChanged` (and so `page`'s own reset watcher below) recompute on
 * every `loadMore`, not only on an actual `onlyChanged` flip.
 */
const onlyChanged = computed(() => kindFilter.value !== "unchanged");

const filter = computed<FieldRowFilterDto>(() => ({
  onlyChanged: onlyChanged.value,
  offset: offset.value,
  limit: PAGE_SIZE,
}));

const query = useDefConflictViewQuery(key, filter, () => patchId);

// A different finding or patch scope, or a filter change that actually
// changes the outgoing request (`onlyChanged`, only ever flipped by the
// `"unchanged"` option), starts the field list over from page one — see
// `usePagedRows` for why pages are kept by offset rather than appended
// (and its own doc comment for why `onlyChanged` is read as a standalone
// computed here, never through `filter.value`, which also depends on
// `offset`). Deliberately *not* keyed on `kindFilter` itself: switching
// between "changed" and one of its own non-unchanged kinds
// (`conflict`/`cleanMerge`/`listEntry`/`mapEntry`) sends the identical request, so
// Pinia Colada serves the same cached `query.data` back unchanged —
// resetting on that would empty `page.rows` with nothing arriving to
// refill it, since nothing actually refetched.
const page = usePagedRows(
  offset,
  computed(() =>
    query.data.value
      ? { items: query.data.value.fields, total: query.data.value.fieldsTotal }
      : null,
  ),
  () => [key.value, patchId, onlyChanged.value],
);

function matchesKindFilter(row: FieldRowDto): boolean {
  return kindFilter.value === "changed" ? row.kind !== "unchanged" : row.kind === kindFilter.value;
}
const visibleRows = computed(() => page.rows.value.filter(matchesKindFilter));

const sourceFailed = computed(() =>
  query.error.value instanceof RimmergeError && query.error.value.code === "merge_source_failed"
    ? query.error.value
    : null,
);
const otherError = computed(() => (sourceFailed.value === null ? query.error.value : null));

type CompletenessTone = "auto" | "warning";

/**
 * `effectiveCompleteness.kind`
 * alone can read `"complete"` while `problems` is non-empty — a
 * `DefOverride` whose *losing* owner's template chain is broken still
 * completes the winner's own fold (`Problem::PlanFailed` documents the
 * gap without downgrading completeness, see `def_conflict_view.rs`), so
 * a bare "Complete" pill would sit directly above a problem the reader
 * has no reason to expect. Fold both signals into one label/tone pair
 * instead of branching on `effectiveCompleteness.kind` alone.
 */
const completenessDisplay = computed<{ label: string; tone: CompletenessTone } | null>(() => {
  const data = query.data.value;
  if (!data) {
    return null;
  }
  const isComplete = data.effectiveCompleteness.kind === "complete";
  const hasProblems = data.problems.length > 0;
  if (!isComplete) {
    return { label: t("inbox.defConflictView.completenessPartial"), tone: "warning" };
  }
  return hasProblems
    ? { label: t("inbox.defConflictView.completenessCompleteWithProblems"), tone: "warning" }
    : { label: t("inbox.defConflictView.completenessComplete"), tone: "auto" };
});

function kindLabel(kind: DefConflictKindDto): string {
  switch (kind.kind) {
    case "defOverride":
      return t("finding.kind.defOverride");
    case "patchCollision":
      return kind.subPath
        ? t("inbox.defConflictView.patchCollisionWithSubPath", { subPath: kind.subPath })
        : t("finding.kind.patchCollision");
    case "duplicateTemplateName":
      return t("finding.kind.duplicateTemplateName");
  }
}
</script>

<template>
  <div
    class="flex flex-col gap-3"
    data-testid="def-conflict-view"
  >
    <p
      v-if="sourceFailed"
      class="text-status-danger text-sm"
      data-testid="def-conflict-view-source-failed"
    >
      {{ t("inbox.defConflictView.sourceFailed") }}
    </p>
    <p
      v-else-if="otherError"
      class="text-status-danger text-sm"
      data-testid="def-conflict-view-error"
    >
      {{
        otherError instanceof Error
          ? otherError.message
          : t("inbox.defConflictView.loadFailed")
      }}
    </p>
    <p
      v-else-if="query.isPending.value"
      class="text-text-muted text-sm"
      data-testid="def-conflict-view-loading"
    >
      {{ t("common.loading") }}
    </p>

    <template v-else-if="query.data.value">
      <header class="flex flex-wrap items-center justify-between gap-2">
        <div class="flex items-center gap-2">
          <RouterLink
            :to="inspectRoute(query.data.value.defRef)"
            class="text-accent font-mono text-sm underline"
            data-testid="def-conflict-view-def-ref"
          >
            {{ query.data.value.defRef }}
          </RouterLink>
          <span
            class="text-text-muted text-xs"
            data-testid="def-conflict-view-kind"
          >{{ kindLabel(query.data.value.kind) }}</span>
        </div>
        <span
          v-if="completenessDisplay"
          class="shrink-0 rounded-full px-2 py-0.5 text-xs font-medium"
          :class="
            completenessDisplay.tone === 'auto'
              ? 'bg-status-auto-soft text-status-auto'
              : 'bg-status-input-soft text-status-input'
          "
          data-testid="def-conflict-view-completeness"
        >{{ completenessDisplay.label }}</span>
      </header>

      <TouchersList
        :touchers="query.data.value.touchers"
        :problems="query.data.value.problems"
      />

      <section
        v-if="query.data.value.injectedNodeRelations.length > 0"
        class="flex flex-col gap-1"
        data-testid="def-conflict-view-injected-node-relations"
      >
        <h3 class="text-text text-xs font-semibold">
          {{ t("edgeKind.patchSelectsInjectedNode") }}
        </h3>
        <ul class="flex flex-col gap-1 text-xs">
          <li
            v-for="relation in query.data.value.injectedNodeRelations"
            :key="`${relation.selector}|${relation.injector}|${relation.subject}`"
            class="border-border-subtle bg-surface-1 text-text-muted rounded border p-2"
            data-testid="def-conflict-injected-node-relation-row"
          >
            {{
              t("inbox.defConflictView.injectedNodeRelation", {
                selector: modLabel.label(relation.selector),
                injector: modLabel.label(relation.injector),
              })
            }}
            <span class="text-text-faint break-all">(<code>{{ relation.subject }}</code>)</span>
          </li>
        </ul>
      </section>

      <label class="text-text-muted flex items-center gap-1 self-start text-xs">
        {{ t("inbox.defConflictView.showLabel") }}
        <select
          v-model="kindFilter"
          class="border-border-subtle bg-surface-0 text-text rounded border px-1 py-0.5 text-xs"
          data-testid="def-conflict-view-filter"
        >
          <option value="changed">
            {{ t("inbox.defConflictView.filterAllChanged") }}
          </option>
          <option value="conflict">
            {{ t("inbox.defConflictView.filterConflict") }}
          </option>
          <option value="cleanMerge">
            {{ t("inbox.defConflictView.filterCleanMerge") }}
          </option>
          <option value="listEntry">
            {{ t("inbox.defConflictView.filterListEntries") }}
          </option>
          <option value="mapEntry">
            {{ t("inbox.defConflictView.filterMapEntries") }}
          </option>
          <option value="unchanged">
            {{ t("inbox.defConflictView.filterUnchanged") }}
          </option>
        </select>
      </label>

      <FieldRowsTable :rows="visibleRows" />

      <button
        v-if="page.hasMore.value"
        type="button"
        class="border-border-subtle text-text-muted hover:bg-surface-2 hover:text-text cursor-pointer self-start rounded border px-2 py-1 text-xs"
        data-testid="def-conflict-view-show-more"
        @click="page.loadMore"
      >
        {{
          t("inbox.defConflictView.showMore", {
            shown: page.rows.value.length,
            total: page.total.value,
          })
        }}
      </button>

      <section
        v-if="query.data.value.problems.length > 0"
        class="flex flex-col gap-1"
        data-testid="def-conflict-view-problems"
      >
        <h3 class="text-text text-xs font-semibold">
          {{ t("inbox.defConflictView.problemsHeading") }}
        </h3>
        <ul class="flex flex-col gap-1 text-xs">
          <li
            v-for="(problem, index) in query.data.value.problems"
            :key="index"
            class="border-status-input bg-status-input-soft text-status-input rounded border p-2"
            data-testid="def-conflict-problem-row"
          >
            <template v-if="problem.kind === 'unsupportedOp'">
              {{
                t("inbox.defConflictView.problemUnsupportedOp", {
                  modId: modLabel.label(problem.modId),
                  opIndex: problem.opIndex,
                })
              }}
              (<code>{{ problem.class }}</code>) — {{ problem.reason }}
              <div
                v-if="problem.xpath"
                class="text-text-faint break-all"
              >
                {{ problem.xpath }}
              </div>
              <div class="text-text-faint">
                {{ t("inbox.defConflictView.problemReplayStopped") }}
              </div>
            </template>
            <template v-else-if="problem.kind === 'missingTemplate'">
              <i18n-t
                keypath="inbox.defConflictView.problemMissingTemplate"
                tag="span"
              >
                <template #name>
                  <code>{{ problem.name }}</code>
                </template>
                <template #defType>
                  {{ problem.defType }}
                </template>
              </i18n-t>
            </template>
            <template v-else-if="problem.kind === 'cycle'">
              {{ t("inbox.defConflictView.problemCycle", { chain: problem.chain.join(" → ") }) }}
            </template>
            <template v-else-if="problem.kind === 'planFailed'">
              {{ t("inbox.defConflictView.problemPlanFailed", { reason: problem.reason }) }}
            </template>
            <template v-else>
              {{ assertNever(problem) }}
            </template>
          </li>
        </ul>
      </section>
    </template>
  </div>
</template>
