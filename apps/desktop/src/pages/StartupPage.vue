<script setup lang="ts">
import { useVirtualizer, type VirtualItem } from "@tanstack/vue-virtual";
import Message from "primevue/message";
import { computed, ref, useTemplateRef } from "vue";
import { useI18n } from "vue-i18n";
import ImportedLogSummary from "@/components/startup/ImportedLogSummary.vue";
import ImportGameLogButton from "@/components/startup/ImportGameLogButton.vue";
import { useLoggedOrderCheck } from "@/composables/useLoggedOrderCheck";
import { useModLabel } from "@/composables/useModLabel";
import { useStartupCostsQuery } from "@/queries/startup";
import { useSessionStore } from "@/stores/session";
import type { ModCostRowDto } from "@/types/generated/ModCostRowDto";
import { formatBytes } from "@/utils/format";
import {
  hasLoggingGaps,
  isConsoleSnapshot,
  isCutOffBeforePatchPhase,
} from "@/utils/gameLogCoverage";
import {
  badDimensionDdsLookup,
  type ImportedCount,
  type ImportedTimers,
  observedTimersLookup,
} from "@/utils/startupMetrics";

const { t } = useI18n();
const session = useSessionStore();
const modLabel = useModLabel();
const { data: rows, isPending, error } = useStartupCostsQuery();
const loggedOrder = useLoggedOrderCheck();
const isSnapshotImported = computed(
  () => session.gameLogSummary !== null && isConsoleSnapshot(session.gameLogSummary),
);
// A snapshot's own note already says its counts are lower bounds; a whole
// `Player.log` with a logging gap gets its own.
const hasGapsNote = computed(
  () =>
    session.gameLogSummary !== null &&
    !isConsoleSnapshot(session.gameLogSummary) &&
    hasLoggingGaps(session.gameLogSummary),
);

// A cut-off log's note comes first, matching the apply dialog's caveat order;
// a cut-off log can also have gaps, and then both notes show.
const hasCutOffNote = computed(
  () =>
    session.gameLogSummary !== null && isCutOffBeforePatchPhase(session.gameLogSummary.coverage),
);

const badDimensionDdsFor = computed(() => badDimensionDdsLookup(session.gameLogSummary));
const observedTimersFor = computed(() => observedTimersLookup(session.gameLogSummary));

interface DisplayRow {
  cost: ModCostRowDto;
  badDimensionDds: ImportedCount;
  observedTimers: ImportedTimers;
}

const displayRows = computed<DisplayRow[]>(() =>
  (rows.value ?? []).map((cost) => ({
    cost,
    badDimensionDds: badDimensionDdsFor.value(cost.modId),
    observedTimers: observedTimersFor.value(cost.modId),
  })),
);

type SortKey =
  | "textureBytes"
  | "textureFiles"
  | "ddsFiles"
  | "patchOps"
  | "slowXpathOps"
  | "assemblyBytes"
  | "assemblyCount"
  | "defCount"
  | "overriddenTextureBytes"
  | "badDimensionDds"
  | "observedTimers";

/** Evidence says texture loading is the dominant startup cost — the default sort. */
const sortKey = ref<SortKey>("textureBytes");
const sortDir = ref<"asc" | "desc">("desc");

function toggleSort(key: SortKey): void {
  if (sortKey.value === key) {
    sortDir.value = sortDir.value === "desc" ? "asc" : "desc";
    return;
  }
  sortKey.value = key;
  sortDir.value = "desc";
}

function ariaSortFor(key: SortKey): "ascending" | "descending" | "none" {
  if (sortKey.value !== key) {
    return "none";
  }
  return sortDir.value === "asc" ? "ascending" : "descending";
}

/**
 * `-1` for "not imported" — every real count is `>= 0`, so this always
 * sorts below every real value in descending order (the default sort
 * direction for every numeric column here). Ascending, it sorts
 * "not imported" rows *first*, not last — that's fine today because
 * every row shares the same imported-or-not state for a given column
 * (one summary, one set of "not imported" rows), so the two orderings
 * never actually interleave real and placeholder values.
 */
function sortValue(row: DisplayRow, key: SortKey): number {
  switch (key) {
    case "textureBytes":
      return row.cost.textureBytes;
    case "textureFiles":
      return row.cost.textureFiles;
    case "ddsFiles":
      return row.cost.ddsFiles;
    case "patchOps":
      return row.cost.patchOps;
    case "slowXpathOps":
      return row.cost.slowXpathOps;
    case "assemblyBytes":
      return row.cost.assemblyBytes;
    case "assemblyCount":
      return row.cost.assemblyCount;
    case "defCount":
      return row.cost.defCount;
    case "overriddenTextureBytes":
      return row.cost.overriddenTextureBytes;
    case "badDimensionDds":
      return row.badDimensionDds.imported ? row.badDimensionDds.count : -1;
    case "observedTimers":
      return row.observedTimers.imported ? row.observedTimers.totalMilliseconds : -1;
  }
}

const sortedRows = computed(() => {
  const direction = sortDir.value === "asc" ? 1 : -1;
  return [...displayRows.value].sort(
    (a, b) => (sortValue(a, sortKey.value) - sortValue(b, sortKey.value)) * direction,
  );
});

const totals = computed(() => {
  const acc = {
    patchOps: 0,
    slowXpathOps: 0,
    textureFiles: 0,
    textureBytes: 0,
    ddsFiles: 0,
    assemblyCount: 0,
    assemblyBytes: 0,
    defCount: 0,
    overriddenTextureBytes: 0,
  };
  for (const row of displayRows.value) {
    acc.patchOps += row.cost.patchOps;
    acc.slowXpathOps += row.cost.slowXpathOps;
    acc.textureFiles += row.cost.textureFiles;
    acc.textureBytes += row.cost.textureBytes;
    acc.ddsFiles += row.cost.ddsFiles;
    acc.assemblyCount += row.cost.assemblyCount;
    acc.assemblyBytes += row.cost.assemblyBytes;
    acc.defCount += row.cost.defCount;
    acc.overriddenTextureBytes += row.cost.overriddenTextureBytes;
  }
  return acc;
});

/**
 * Virtualized: every row times 13 columns, rendered unvirtualized, would
 * re-render thousands of cells on every sort toggle on a large install.
 * Follows `components/order/OrderTable.vue`'s own precedent for the same
 * large mod-list dataset, adapted to a real `<table>`: rather than the
 * absolute-positioned `translateY` rows `OrderTable`/`MergeFieldTable`
 * use (which would pull each `<tr>` out of table layout and break column
 * alignment with the totals row), this renders only the visible slice of
 * real `<tr>` elements bracketed by two height-only spacer rows — the
 * padding-row technique `@tanstack/vue-virtual`'s own table examples use,
 * so `<table>`'s normal row-layout algorithm still sizes every column
 * consistently across the totals row and every rendered body row.
 */
const scrollElementRef = useTemplateRef<HTMLDivElement>("scrollElement");
const ROW_HEIGHT_PX = 36;

const virtualizer = useVirtualizer(
  computed(() => ({
    count: sortedRows.value.length,
    getScrollElement: () => scrollElementRef.value,
    estimateSize: () => ROW_HEIGHT_PX,
    overscan: 12,
  })),
);

interface VisibleRow {
  virtualRow: VirtualItem;
  row: DisplayRow;
}

// `noUncheckedIndexedAccess` makes `sortedRows.value[virtualRow.index]` an
// `DisplayRow | undefined` — resolved once here, matching `OrderTable`'s
// own `visibleRows` computed.
const visibleRows = computed<VisibleRow[]>(() =>
  virtualizer.value
    .getVirtualItems()
    .map((virtualRow) => ({ virtualRow, row: sortedRows.value[virtualRow.index] }))
    .filter((entry): entry is VisibleRow => entry.row !== undefined),
);

const totalSize = computed(() => virtualizer.value.getTotalSize());
/** Height of the unrendered slice above the visible window, as a single spacer `<tr>`. */
const paddingTop = computed(() => virtualizer.value.getVirtualItems()[0]?.start ?? 0);
/** Height of the unrendered slice below the visible window, as a single spacer `<tr>`. */
const paddingBottom = computed(() => {
  const items = virtualizer.value.getVirtualItems();
  const last = items[items.length - 1];
  return last === undefined ? 0 : totalSize.value - last.end;
});

/** Column count the totals row and both spacer rows must span — keep in sync with `<thead>`'s own `<th>` count. */
const COLUMN_COUNT = 13;
</script>

<template>
  <div class="mx-auto flex h-full max-w-6xl min-h-0 flex-col gap-6 p-6">
    <div class="flex shrink-0 items-center justify-between gap-2">
      <h1 class="text-text text-xl font-semibold">
        {{ t("startup.heading") }}
      </h1>
      <ImportGameLogButton />
    </div>

    <!-- eslint-disable vue/no-bare-strings-in-template -- a literal
         filename inside `<code>`, never translated, the same category
         as a keyboard-shortcut label; every other string in this block
         already goes through `t()`. -->
    <i18n-t
      keypath="startup.description"
      tag="p"
      class="text-text-muted text-sm"
    >
      <template #playerLog>
        <code>Player.log</code>
      </template>
    </i18n-t>
    <!-- eslint-enable vue/no-bare-strings-in-template -->

    <ImportedLogSummary />

    <Message
      v-if="isSnapshotImported"
      severity="info"
      data-testid="startup-snapshot-note"
    >
      {{ t("startup.snapshotNote") }}
    </Message>
    <Message
      v-if="hasCutOffNote"
      severity="warn"
      data-testid="startup-cut-off-note"
    >
      {{ t("startup.cutOffNote") }}
    </Message>
    <Message
      v-if="hasGapsNote"
      severity="warn"
      data-testid="startup-gaps-note"
    >
      {{ t("startup.gapsNote") }}
    </Message>
    <Message
      v-if="loggedOrder.differs.value"
      severity="warn"
      data-testid="startup-order-mismatch-warning"
    >
      {{ t("startup.orderMismatchWarning") }}
    </Message>
    <Message
      v-else-if="loggedOrder.noOrderBlock.value"
      severity="warn"
      data-testid="startup-order-unknown-warning"
    >
      {{ t("startup.noOrderBlockWarning") }}
    </Message>

    <p
      v-if="isPending"
      class="text-text-muted text-sm"
      data-testid="startup-loading"
    >
      {{ t("common.loading") }}
    </p>
    <p
      v-else-if="error"
      class="text-status-danger text-sm"
      data-testid="startup-error"
    >
      {{ error instanceof Error ? error.message : t("startup.loadFailed") }}
    </p>

    <div
      v-else
      ref="scrollElement"
      class="surface-card min-h-0 flex-1 overflow-auto"
      data-testid="startup-table-scroll"
    >
      <table
        class="w-full min-w-max border-collapse text-sm whitespace-nowrap"
        data-testid="startup-table"
      >
        <thead>
          <tr class="table-head text-right">
            <th
              scope="col"
              class="bg-surface-1 sticky left-0 z-10 py-2 pr-3 pl-3 text-left font-medium"
            >
              {{ t("startup.modHeader") }}
            </th>
            <th
              scope="col"
              class="py-2 pr-3 font-medium"
              :aria-sort="ariaSortFor('patchOps')"
            >
              <button
                type="button"
                class="hover:text-text cursor-pointer focus-visible:outline"
                data-testid="startup-sort-patchOps"
                @click="toggleSort('patchOps')"
              >
                {{ t("startup.patchOpsHeader") }}
              </button>
            </th>
            <th
              scope="col"
              class="py-2 pr-3 font-medium"
              :aria-sort="ariaSortFor('slowXpathOps')"
            >
              <button
                type="button"
                class="hover:text-text cursor-pointer focus-visible:outline"
                data-testid="startup-sort-slowXpathOps"
                @click="toggleSort('slowXpathOps')"
              >
                {{ t("startup.slowXpathsHeader") }}
              </button>
            </th>
            <th
              scope="col"
              class="py-2 pr-3 font-medium"
              :aria-sort="ariaSortFor('textureFiles')"
            >
              <button
                type="button"
                class="hover:text-text cursor-pointer focus-visible:outline"
                data-testid="startup-sort-textureFiles"
                @click="toggleSort('textureFiles')"
              >
                {{ t("startup.texFilesHeader") }}
              </button>
            </th>
            <th
              scope="col"
              class="py-2 pr-3 font-medium"
              :aria-sort="ariaSortFor('textureBytes')"
            >
              <button
                type="button"
                class="hover:text-text cursor-pointer focus-visible:outline"
                data-testid="startup-sort-textureBytes"
                @click="toggleSort('textureBytes')"
              >
                {{ t("startup.texBytesHeader") }}
              </button>
            </th>
            <th
              scope="col"
              class="py-2 pr-3 font-medium"
              :aria-sort="ariaSortFor('ddsFiles')"
            >
              <button
                type="button"
                class="hover:text-text cursor-pointer focus-visible:outline"
                data-testid="startup-sort-ddsFiles"
                @click="toggleSort('ddsFiles')"
              >
                {{ t("startup.ddsFilesHeader") }}
              </button>
            </th>
            <th
              scope="col"
              class="py-2 pr-3 font-medium"
              :aria-sort="ariaSortFor('badDimensionDds')"
            >
              <button
                type="button"
                class="hover:text-text cursor-pointer focus-visible:outline"
                data-testid="startup-sort-badDimensionDds"
                @click="toggleSort('badDimensionDds')"
              >
                {{ t("startup.badDimDdsHeader") }}
              </button>
            </th>
            <th
              scope="col"
              class="py-2 pr-3 font-medium"
              :aria-sort="ariaSortFor('assemblyCount')"
            >
              <button
                type="button"
                class="hover:text-text cursor-pointer focus-visible:outline"
                data-testid="startup-sort-assemblyCount"
                @click="toggleSort('assemblyCount')"
              >
                {{ t("startup.dllsHeader") }}
              </button>
            </th>
            <th
              scope="col"
              class="py-2 pr-3 font-medium"
              :aria-sort="ariaSortFor('assemblyBytes')"
            >
              <button
                type="button"
                class="hover:text-text cursor-pointer focus-visible:outline"
                data-testid="startup-sort-assemblyBytes"
                @click="toggleSort('assemblyBytes')"
              >
                {{ t("startup.dllBytesHeader") }}
              </button>
            </th>
            <th
              scope="col"
              class="py-2 pr-3 font-medium"
              :aria-sort="ariaSortFor('defCount')"
            >
              <button
                type="button"
                class="hover:text-text cursor-pointer focus-visible:outline"
                data-testid="startup-sort-defCount"
                @click="toggleSort('defCount')"
              >
                {{ t("startup.defsHeader") }}
              </button>
            </th>
            <th
              scope="col"
              class="py-2 pr-3 font-medium"
              :aria-sort="ariaSortFor('overriddenTextureBytes')"
            >
              <button
                type="button"
                class="hover:text-text cursor-pointer focus-visible:outline"
                data-testid="startup-sort-overriddenTextureBytes"
                @click="toggleSort('overriddenTextureBytes')"
              >
                {{ t("startup.overriddenTexHeader") }}
              </button>
            </th>
            <th
              scope="col"
              class="py-2 pr-3 font-medium"
              :aria-sort="ariaSortFor('observedTimers')"
            >
              <button
                type="button"
                class="hover:text-text cursor-pointer focus-visible:outline"
                data-testid="startup-sort-observedTimers"
                @click="toggleSort('observedTimers')"
              >
                {{ t("startup.observedTimersHeader") }}
              </button>
            </th>
            <th
              scope="col"
              class="py-2 pr-3 font-medium"
            >
              {{ t("startup.contentOnlyHeader") }}
            </th>
          </tr>
        </thead>
        <tbody>
          <tr
            class="border-border-subtle bg-surface-2 border-b text-right font-semibold *:pr-3"
            data-testid="startup-totals-row"
          >
            <th
              scope="row"
              class="bg-surface-2 sticky left-0 z-10 py-2 pl-3 text-left"
            >
              {{ t("startup.totalRow", { count: displayRows.length }, displayRows.length) }}
            </th>
            <td>{{ totals.patchOps }}</td>
            <td>{{ totals.slowXpathOps }}</td>
            <td>{{ totals.textureFiles }}</td>
            <td>{{ formatBytes(totals.textureBytes) }}</td>
            <td>{{ totals.ddsFiles }}</td>
            <td>—</td>
            <td>{{ totals.assemblyCount }}</td>
            <td>{{ formatBytes(totals.assemblyBytes) }}</td>
            <td>{{ totals.defCount }}</td>
            <td>{{ formatBytes(totals.overriddenTextureBytes) }}</td>
            <td>—</td>
            <td>
              —
            </td>
          </tr>
          <tr
            v-if="paddingTop > 0"
            aria-hidden="true"
          >
            <td
              :colspan="COLUMN_COUNT"
              class="p-0"
              :style="{ height: `${paddingTop}px` }"
            />
          </tr>
          <tr
            v-for="{ virtualRow, row } in visibleRows"
            :key="row.cost.modId"
            :data-index="virtualRow.index"
            class="border-border-subtle hover:bg-surface-2 group border-b text-right *:pr-3"
            :data-testid="`startup-row-${row.cost.modId}`"
          >
            <th
              scope="row"
              class="text-text bg-surface-1 group-hover:bg-surface-2 sticky left-0 z-10 py-1.5 pl-3 text-left font-normal"
              :title="modLabel.titleFor(row.cost.modId)"
            >
              {{ modLabel.label(row.cost.modId) }}
            </th>
            <td>{{ row.cost.patchOps }}</td>
            <td>{{ row.cost.slowXpathOps }}</td>
            <td>{{ row.cost.textureFiles }}</td>
            <td>{{ formatBytes(row.cost.textureBytes) }}</td>
            <td>{{ row.cost.ddsFiles }}</td>
            <td :data-testid="`startup-bad-dds-${row.cost.modId}`">
              <span v-if="row.badDimensionDds.imported">{{
                row.badDimensionDds.isLowerBound
                  ? t("startup.atLeast", { count: row.badDimensionDds.count })
                  : row.badDimensionDds.count
              }}</span>
              <span
                v-else
                class="text-text-faint italic"
              >{{ t("startup.notImported") }}</span>
            </td>
            <td>{{ row.cost.assemblyCount }}</td>
            <td>{{ formatBytes(row.cost.assemblyBytes) }}</td>
            <td>{{ row.cost.defCount }}</td>
            <td>{{ formatBytes(row.cost.overriddenTextureBytes) }}</td>
            <td :data-testid="`startup-observed-timers-${row.cost.modId}`">
              <span v-if="row.observedTimers.imported">
                {{
                  row.observedTimers.isLowerBound
                    ? t("startup.observedTimersAtLeast", {
                      count: row.observedTimers.count,
                      ms: row.observedTimers.totalMilliseconds,
                    })
                    : t("startup.observedTimersValue", {
                      count: row.observedTimers.count,
                      ms: row.observedTimers.totalMilliseconds,
                    })
                }}
              </span>
              <template v-else-if="row.observedTimers.reason === 'consoleSnapshot'">
                <span
                  class="text-text-faint"
                  aria-hidden="true"
                  :title="t('startup.notAvailableFromSnapshot')"
                >—</span>
                <span class="sr-only">{{ t("startup.notAvailableFromSnapshot") }}</span>
              </template>
              <span
                v-else
                class="text-text-faint italic"
              >{{ t("startup.notImported") }}</span>
            </td>
            <td>
              <span v-if="row.cost.contentOnly">{{ t("startup.contentOnly") }}</span>
              <span
                v-else
                class="text-text-faint"
              >—</span>
            </td>
          </tr>
          <tr
            v-if="paddingBottom > 0"
            aria-hidden="true"
          >
            <td
              :colspan="COLUMN_COUNT"
              class="p-0"
              :style="{ height: `${paddingBottom}px` }"
            />
          </tr>
        </tbody>
      </table>
    </div>
  </div>
</template>
