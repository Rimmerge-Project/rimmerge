<script setup lang="ts">
import { useVirtualizer, type VirtualItem } from "@tanstack/vue-virtual";
import { type ComponentPublicInstance, computed, useTemplateRef } from "vue";
import { useI18n } from "vue-i18n";

import MergeFieldRow from "@/components/merge/MergeFieldRow.vue";
import { useModLabel } from "@/composables/useModLabel";
import { asFieldPath, type FieldPath } from "@/types/brands";
import type { MergeChoiceDto } from "@/types/generated/MergeChoiceDto";
import type { MergeFieldDto } from "@/types/generated/MergeFieldDto";
import type { MergeOwnerDto } from "@/types/generated/MergeOwnerDto";
import {
  containerStats,
  effectiveContainerCollapsed,
  type FieldGroup,
  groupConsecutiveByContainer,
} from "@/utils/mergeFieldContainers";

const {
  conflictFields,
  autoFields,
  unchangedFields,
  conflictTotal,
  autoTotal,
  unchangedTotal,
  owners,
  collapsedAuto,
  showUnchanged,
  rowCursor,
  columnCursor,
  editingPath,
  editingDraft,
  isPatchCollision = false,
  pendingChoices = {},
  containerOverrides = {},
} = defineProps<{
  /** `Conflict`-classified rows — always shown, open. May be fewer than `conflictTotal` while a later page hasn't loaded yet. */
  conflictFields: MergeFieldDto[];
  /** `OneSided`/`Agreeing` rows — collapsed by default. May be fewer than `autoTotal` while a later page hasn't loaded yet. */
  autoFields: MergeFieldDto[];
  /** `Unchanged` rows — hidden unless toggled. May be fewer than `unchangedTotal` while a later page hasn't loaded yet. */
  unchangedFields: MergeFieldDto[];
  /** The section header counts — computed server-side over the whole diff, so they're accurate even before every page of rows has loaded (see `useMergeFieldPage`). */
  conflictTotal: number;
  autoTotal: number;
  unchangedTotal: number;
  owners: MergeOwnerDto[];
  collapsedAuto: boolean;
  showUnchanged: boolean;
  /** Index into the flattened, currently-visible field rows (mirrors `useMergeStore.rowCursor`). */
  rowCursor: number;
  columnCursor: number;
  editingPath: FieldPath | null;
  editingDraft: string;
  /** Whether the preview these rows belong to is for a `PatchCollision` finding — see `MergeFieldRow`'s own prop of the same name. */
  isPatchCollision?: boolean;
  /** `useMergeChoices`' own client-side choices map, keyed by field path text — see `MergeFieldRow`'s own `pendingChoice` prop. */
  pendingChoices?: Record<string, MergeChoiceDto>;
  /** `useMergeStore.containerOverrides` — explicit per-container collapse picks; a container with no entry here uses the data-driven default (see `utils/mergeFieldContainers.ts`). */
  containerOverrides?: Record<string, boolean>;
}>();
const emit = defineEmits<{
  toggleAuto: [];
  toggleUnchanged: [];
  /** A keyed map's own container header was toggled (auto-resolved/unchanged sections only — the conflicts section's own header never collapses). */
  toggleContainer: [container: string, collapsed: boolean];
  selectRow: [index: number];
  choose: [path: FieldPath, choice: MergeChoiceDto];
  drop: [path: FieldPath];
  revert: [path: FieldPath];
  editCommit: [text: string];
  editCancel: [];
}>();
const { t } = useI18n();
const modLabel = useModLabel();

interface HeaderRow {
  type: "header";
  key: "auto" | "unchanged" | "conflicts";
  /** A literal `en.json` key under `merge.table.section*` — already includes the `({count})` suffix as its own `{count}` param, so the template calls `t(row.labelKey, { count: row.count })` directly. */
  labelKey: string;
  count: number;
  collapsible: boolean;
  collapsed: boolean;
}
/**
 * A tag-keyed map's own container header — one per container per section,
 * shown only when
 * more than one row of that section shares the container (a lone entry
 * renders as a plain field row, no header, exactly like an ordinary
 * field). `total`/`conflicts` are counted across *every* section, so the
 * header reads the same ("wildAnimals — 8 entries, 1 conflict") no
 * matter which section it appears in; `fields` is this *section's* own
 * members, the set `acceptUnion` below actually acts on.
 */
interface ContainerHeaderRow {
  type: "containerHeader";
  key: string;
  /** Which section this instance renders in — a container spanning more than one section (the common case: most keys auto-resolve, a few stay `Conflict`) gets one header per section it appears in, so this also disambiguates their otherwise-identical `data-testid`s. */
  section: HeaderRow["key"];
  container: string;
  total: number;
  conflicts: number;
  collapsible: boolean;
  collapsed: boolean;
  /** Only the conflicts-section instance offers "Accept suggested" — the auto/unchanged sections' own members already resolved without input. */
  showAccept: boolean;
  fields: MergeFieldDto[];
}
interface FieldRow {
  type: "field";
  key: string;
  field: MergeFieldDto;
  /** Position among the currently-visible field rows — matches `rowCursor`'s own indexing. */
  index: number;
}
type Row = HeaderRow | ContainerHeaderRow | FieldRow;

const allFields = computed(() => [...conflictFields, ...autoFields, ...unchangedFields]);
const stats = computed(() => containerStats(allFields.value));

/**
 * Appends `fields`' own rows (as `groupConsecutiveByContainer` groups
 * them) to `result`, advancing `index` for every field row actually
 * shown — a row hidden behind a collapsed container header consumes no
 * index, matching `MergeEditorPage`'s own `visibleFields` filtering (see
 * that computed's own doc comment) so `rowCursor` keeps addressing
 * exactly what's on screen.
 */
function pushSectionRows(
  result: Row[],
  sectionKey: HeaderRow["key"],
  fields: readonly MergeFieldDto[],
  index: { value: number },
): void {
  const groups: FieldGroup[] = groupConsecutiveByContainer(fields);
  for (const group of groups) {
    if (group.container !== null && group.fields.length > 1) {
      const groupStats = stats.value.get(group.container) ?? {
        total: group.fields.length,
        conflicts: 0,
      };
      const collapsible = sectionKey !== "conflicts";
      const collapsed =
        collapsible && effectiveContainerCollapsed(group.container, groupStats, containerOverrides);
      result.push({
        type: "containerHeader",
        key: `merge-container-${sectionKey}-${group.container}`,
        section: sectionKey,
        container: group.container,
        total: groupStats.total,
        conflicts: groupStats.conflicts,
        collapsible,
        collapsed,
        showAccept: sectionKey === "conflicts",
        fields: group.fields,
      });
      if (collapsed) {
        continue;
      }
    }
    for (const field of group.fields) {
      result.push({ type: "field", key: field.path, field, index: index.value++ });
    }
  }
}

const rows = computed<Row[]>(() => {
  const result: Row[] = [];
  const index = { value: 0 };
  result.push({
    type: "header",
    key: "conflicts",
    labelKey: "merge.table.sectionConflicts",
    count: conflictTotal,
    collapsible: false,
    collapsed: false,
  });
  pushSectionRows(result, "conflicts", conflictFields, index);
  result.push({
    type: "header",
    key: "auto",
    labelKey: "merge.table.sectionAuto",
    count: autoTotal,
    collapsible: true,
    collapsed: collapsedAuto,
  });
  if (!collapsedAuto) {
    pushSectionRows(result, "auto", autoFields, index);
  }
  result.push({
    type: "header",
    key: "unchanged",
    labelKey: "merge.table.sectionUnchanged",
    count: unchangedTotal,
    collapsible: true,
    collapsed: !showUnchanged,
  });
  if (showUnchanged) {
    pushSectionRows(result, "unchanged", unchangedFields, index);
  }
  return result;
});

const HEADER_HEIGHT_PX = 28;
/** Only an *estimate* — a field row's real height varies with its
 * content (a plain leaf vs. a multi-line `<pre>` list-item value), so the
 * virtualizer measures each row's actual rendered height once mounted
 * (see `measureRow` below) rather than trusting this fixed number for
 * every row forever — a fixed estimate alone would make a taller row
 * overlap the next. */
const ROW_HEIGHT_PX = 36;

const scrollElementRef = useTemplateRef<HTMLDivElement>("scrollElement");
const virtualizer = useVirtualizer(
  computed(() => ({
    count: rows.value.length,
    getScrollElement: () => scrollElementRef.value,
    estimateSize: (index: number) => {
      const type = rows.value[index]?.type;
      return type === "header" || type === "containerHeader" ? HEADER_HEIGHT_PX : ROW_HEIGHT_PX;
    },
    overscan: 8,
  })),
);
const totalSize = computed(() => virtualizer.value.getTotalSize());

/** Registers `el` with the virtualizer's own `ResizeObserver`-backed dynamic
 * sizing (needs the matching `data-index` attribute on the same element —
 * see the template below) so a row's real height, once known, replaces
 * `ROW_HEIGHT_PX`'s estimate for it specifically. */
function measureRow(el: Element | ComponentPublicInstance | null): void {
  if (el instanceof Element) {
    virtualizer.value.measureElement(el);
  }
}

interface VisibleEntry {
  virtualRow: VirtualItem;
  row: Row;
}
const visibleEntries = computed<VisibleEntry[]>(() =>
  virtualizer.value
    .getVirtualItems()
    .map((virtualRow) => ({ virtualRow, row: rows.value[virtualRow.index] }))
    .filter((entry): entry is VisibleEntry => entry.row !== undefined),
);

function toggleHeader(row: HeaderRow): void {
  if (row.key === "auto") {
    emit("toggleAuto");
  } else if (row.key === "unchanged") {
    emit("toggleUnchanged");
  }
}

function toggleContainer(row: ContainerHeaderRow): void {
  if (!row.collapsible) {
    return;
  }
  emit("toggleContainer", row.container, !row.collapsed);
}

/**
 * The "auto merge (or at least suggest)" action a keyed-map conflict asks
 * for: applies every one of `row.fields`' own `preselected` winner in a
 * single batch, one `choose` emission per field — `useMergeChoices`'
 * debounced flush (last write wins, one send per burst) already coalesces
 * these into exactly one `set_merge_choices` request, so a user resolving
 * several contested keys under one container never sends more than one
 * round trip for it.
 */
function acceptUnion(row: ContainerHeaderRow): void {
  for (const field of row.fields) {
    if (field.preselected === null) {
      continue;
    }
    emit("choose", asFieldPath(field.path), { choice: "from", modId: field.preselected });
  }
}

/**
 * A container header's own summary line — `"wildAnimals — 8 entries, 1
 * conflict"` — built here rather than composed inline in the template
 * since it independently pluralizes two counts (entries, conflicts) and
 * only shows the second clause when there's at least one conflict; see
 * `utils/format.ts`'s `describeMergeModGroups` for the same
 * composite-sentence pattern.
 */
function containerSummaryText(row: ContainerHeaderRow): string {
  const entries = t("merge.table.containerEntries", { count: row.total }, row.total);
  const base = t("merge.table.containerSummary", { container: row.container, entries });
  if (row.conflicts === 0) {
    return base;
  }
  const conflicts = t("merge.table.containerConflicts", { count: row.conflicts }, row.conflicts);
  return t("merge.table.containerSummaryWithConflicts", { base, conflicts });
}
</script>

<template>
  <div class="flex h-full flex-col">
    <div
      class="table-head flex shrink-0 items-center gap-2 px-2 py-1.5"
      data-testid="merge-field-table-head"
    >
      <span class="w-40 shrink-0">{{ t("merge.table.pathHeader") }}</span>
      <span class="w-20 shrink-0">{{ t("merge.table.classHeader") }}</span>
      <span
        v-for="owner in owners"
        :key="owner.modId"
        class="w-32 shrink-0 truncate"
        :title="modLabel.titleFor(owner.modId)"
      >
        {{ modLabel.label(owner.modId) }}
      </span>
      <span class="w-32 shrink-0">{{ t("merge.table.resultHeader") }}</span>
      <span class="shrink-0">{{ t("merge.table.actionsHeader") }}</span>
    </div>
    <div
      ref="scrollElement"
      class="flex-1 overflow-y-auto"
      data-testid="merge-field-table"
    >
      <div :style="{ height: `${totalSize}px`, position: 'relative', width: '100%' }">
        <div
          v-for="{ virtualRow, row } in visibleEntries"
          :key="row.key"
          :ref="measureRow"
          :data-index="virtualRow.index"
          class="absolute top-0 left-0 w-full"
          :style="{ transform: `translateY(${virtualRow.start}px)` }"
        >
          <button
            v-if="row.type === 'header'"
            type="button"
            class="bg-surface-1 text-text-muted flex w-full items-center gap-2 px-2 py-1 text-xs font-medium focus-visible:outline"
            :class="row.collapsible ? 'cursor-pointer' : 'cursor-default'"
            :data-testid="`merge-section-${row.key}`"
            @click="row.collapsible && toggleHeader(row)"
          >
            <span>{{ t(row.labelKey, { count: row.count }) }}</span>
            <span v-if="row.collapsible">{{ row.collapsed ? "▸" : "▾" }}</span>
          </button>
          <div
            v-else-if="row.type === 'containerHeader'"
            class="bg-surface-1 text-text-muted flex w-full items-center gap-2 px-2 py-1 pl-4 text-xs font-medium"
          >
            <button
              type="button"
              class="flex flex-1 items-center gap-2 text-left focus-visible:outline"
              :class="row.collapsible ? 'cursor-pointer' : 'cursor-default'"
              :data-testid="`merge-container-${row.section}-${row.container}`"
              @click="toggleContainer(row)"
            >
              <span v-if="row.collapsible">{{ row.collapsed ? "▸" : "▾" }}</span>
              <span>{{ containerSummaryText(row) }}</span>
            </button>
            <button
              v-if="row.showAccept"
              type="button"
              class="border-border-subtle text-text-muted shrink-0 cursor-pointer rounded border px-1.5 py-0.5 text-[11px] focus-visible:outline"
              :data-testid="`merge-accept-union-${row.container}`"
              @click="acceptUnion(row)"
            >
              {{ t("merge.table.acceptSuggested", { count: row.fields.length }) }}
            </button>
          </div>
          <MergeFieldRow
            v-else
            :field="row.field"
            :owners="owners"
            :active="rowCursor === row.index"
            :active-column="columnCursor"
            :editing="editingPath === asFieldPath(row.field.path)"
            :editing-draft="editingDraft"
            :is-patch-collision="isPatchCollision"
            :pending-choice="pendingChoices[row.field.path] ?? null"
            @select="emit('selectRow', row.index)"
            @choose="emit('choose', asFieldPath(row.field.path), $event)"
            @drop="emit('drop', asFieldPath(row.field.path))"
            @revert="emit('revert', asFieldPath(row.field.path))"
            @edit-commit="emit('editCommit', $event)"
            @edit-cancel="emit('editCancel')"
          />
        </div>
      </div>
    </div>
  </div>
</template>
