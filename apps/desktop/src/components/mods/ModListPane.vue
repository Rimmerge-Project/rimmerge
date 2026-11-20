<script setup lang="ts">
import { computed, useTemplateRef, watch } from "vue";
import { useI18n } from "vue-i18n";

import ModListStatus from "@/components/mods/ModListStatus.vue";
import ModSelectionBar from "@/components/mods/ModSelectionBar.vue";
import ModTable, {
  type DropMode,
  type MenuAction,
  type RowMenu,
} from "@/components/mods/ModTable.vue";
import { useModSelection } from "@/composables/useModSelection";
import type { ListSortKeyDto } from "@/types/generated/ListSortKeyDto";
import type { ModListDto } from "@/types/generated/ModListDto";
import type { ModListRowDto } from "@/types/generated/ModListRowDto";

const {
  kind,
  modList,
  isPending,
  error,
  isFiltered,
  showActiveColumns = true,
  sortKey,
  showPosition = false,
  draggable = false,
  draggingIds = [],
  dropMode = "off",
  refusedText = "",
  rowMenu = "none",
  reorderBlockedReason = null,
} = defineProps<{
  /** Which list this is: names the pane's heading and its `data-testid`. */
  kind: "active" | "inactive";
  /** The query's current list; `undefined` until the first response. */
  modList: ModListDto | undefined;
  isPending: boolean;
  error: Error | null;
  /**
   * Whether the list query carries a search/tag/source filter. An unfiltered
   * response is every row, so a selected id missing from it is gone, not hidden.
   */
  isFiltered: boolean;
  /** Passed through to {@link ModTable}: off for the Inactive list. */
  showActiveColumns?: boolean;
  /** The list's display sort criterion; sorting by last updated shows each row's time. */
  sortKey: ListSortKeyDto;
  /** Passed through to {@link ModTable}: each row's load-order position, while the Active list is sorted by something else. */
  showPosition?: boolean;
  /** Passed through to {@link ModTable}: whether rows can be picked up. */
  draggable?: boolean;
  /** Passed through to {@link ModTable}: the ids being dragged. */
  draggingIds?: readonly string[];
  /** Passed through to {@link ModTable}: what this list does with the drag in progress. */
  dropMode?: DropMode;
  /** Passed through to {@link ModTable}: why a refused drag is refused. */
  refusedText?: string;
  /** Passed through to {@link ModTable}: the per-row action button. */
  rowMenu?: RowMenu;
  /** Passed through to {@link ModTable}: why moving is off, or `null`. */
  reorderBlockedReason?: string | null;
}>();
const emit = defineEmits<{
  selectMod: [modId: string];
  focusMod: [modId: string];
  dragStart: [modId: string];
  dragEnd: [];
  drop: [slot: number];
  moveStep: [modId: string, direction: "up" | "down"];
  menuAction: [modId: string, action: MenuAction];
}>();
const selectedIds = defineModel<string[]>("selectedIds", { required: true });
defineSlots<{
  /** This list's own actions, at the end of the header row; shown in every state. */
  actions(): unknown;
  /** The list's filter and sort controls, under its header. */
  toolbar(): unknown;
  /** Anything the list has to say about itself (why it cannot be reordered, which order Apply writes). */
  notes(): unknown;
}>();

const { t } = useI18n();

/** The table's rows, only once the list loaded without error. */
const rows = computed<ModListRowDto[] | null>(() =>
  isPending || error || modList === undefined ? null : modList.items,
);

const selection = useModSelection({ selectedIds, rows: () => rows.value ?? [] });

// An unfiltered response is the whole list, so an id missing from it left the
// list (activated elsewhere, gone after a rescan). Under a filter a missing id
// is only hidden, and stays selected.
watch(
  () => [rows.value, isFiltered] as const,
  ([current, filtered]) => {
    if (current !== null && !filtered) {
      selection.pruneToShown();
    }
  },
);

const table = useTemplateRef<{ focusRow: (modId: string) => Promise<void> }>("table");

/** Scrolls the row for `modId` into view and focuses it. */
function focusRow(modId: string): Promise<void> {
  return table.value?.focusRow(modId) ?? Promise.resolve();
}
defineExpose({ focusRow });
</script>

<template>
  <section
    class="flex min-h-0 min-w-0 flex-col gap-3"
    :aria-labelledby="`mods-pane-heading-${kind}`"
    :data-testid="`mods-pane-${kind}`"
  >
    <h2
      :id="`mods-pane-heading-${kind}`"
      class="text-text shrink-0 text-lg font-semibold"
    >
      {{ kind === "active" ? t("mods.page.activeHeading") : t("mods.page.inactiveHeading") }}
    </h2>
    <slot name="toolbar" />
    <slot name="notes" />
    <ModListStatus
      :mod-list="modList"
      :is-pending="isPending"
      :error="error"
    >
      <template #actions>
        <slot name="actions" />
      </template>
    </ModListStatus>
    <ModSelectionBar
      v-if="rows"
      :selection="selection"
    />
    <ModTable
      v-if="rows"
      ref="table"
      :mods="rows"
      selectable
      :selected-ids="selectedIds"
      :show-active-columns="showActiveColumns"
      :show-updated="sortKey === 'lastUpdated'"
      :show-position="showPosition"
      :draggable="draggable"
      :dragging-ids="draggingIds"
      :drop-mode="dropMode"
      :refused-text="refusedText"
      :row-menu="rowMenu"
      :reorder-blocked-reason="reorderBlockedReason"
      @anchor-row="selection.setAnchor"
      @toggle-select="selection.toggle"
      @range-select="selection.selectRange"
      @extend-select="selection.extendSelection"
      @select-all="selection.selectAllShown"
      @clear-selection="selection.clear"
      @select-mod="(modId: string) => emit('selectMod', modId)"
      @focus-mod="(modId: string) => emit('focusMod', modId)"
      @drag-start="(modId: string) => emit('dragStart', modId)"
      @drag-end="emit('dragEnd')"
      @drop="(slot: number) => emit('drop', slot)"
      @move-step="(modId: string, direction: 'up' | 'down') => emit('moveStep', modId, direction)"
      @menu-action="(modId: string, action: MenuAction) => emit('menuAction', modId, action)"
    />
  </section>
</template>
