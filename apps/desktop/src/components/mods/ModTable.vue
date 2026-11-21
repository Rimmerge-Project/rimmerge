<script setup lang="ts">
import { useVirtualizer, type VirtualItem } from "@tanstack/vue-virtual";
import { computed, useTemplateRef } from "vue";
import { useI18n } from "vue-i18n";

import ModRowCells from "@/components/mods/ModRowCells.vue";
import ModRowMenu from "@/components/mods/ModRowMenu.vue";
import { useModLabel } from "@/composables/useModLabel";
import { useRowDrag } from "@/composables/useRowDrag";
import { useRowKeys } from "@/composables/useRowKeys";
import { useRowPointer } from "@/composables/useRowPointer";
import { useUpdatedText } from "@/composables/useUpdatedText";
import type { ColorTokenDto } from "@/types/generated/ColorTokenDto";
import type { ModListRowDto } from "@/types/generated/ModListRowDto";
import {
  type DropMode,
  hasRowMenuFor,
  isDirectActivateFor,
  type MenuAction,
  type RowMenu,
} from "@/utils/modRowMenu";

const {
  mods,
  selectable = false,
  selectedIds = [],
  showActiveColumns = true,
  showUpdated = false,
  showPosition = false,
  openModId = null,
  draggable = false,
  draggingIds = [],
  dropMode = "off",
  refusedText = "",
  rowMenu = "none",
  hasOrganizationActions = false,
  reorderBlockedReason = null,
} = defineProps<{
  mods: ModListRowDto[];
  /** Adds a checkbox column showing {@link selectedIds}; every change goes out as an event for the owner's selection. */
  selectable?: boolean;
  /** Selected ids, only meaningful while {@link selectable} is set. */
  selectedIds?: readonly string[];
  /**
   * Whether the tags/hard-dependents columns show — off for the Inactive
   * tab, where both are always empty (the analyzer never computes tags
   * or Hard-strength-edge dependents for a mod that isn't active). Every
   * row is focusable/clickable/Enter-activated regardless — both tabs
   * open the mod info panel.
   */
  showActiveColumns?: boolean;
  /** Adds each row's last-updated time (or why it has none), shown while the list is sorted by it. */
  showUpdated?: boolean;
  /**
   * Adds each row's load-order position: always on in the Active list (a row's number is its
   * place in the load order, whatever the list is sorted by), never in Inactive, which has none.
   */
  showPosition?: boolean;
  /** The mod open in the info panel: its row is drawn with the accent ring and tint. */
  openModId?: string | null;
  /** Whether a row can be picked up with the pointer. */
  draggable?: boolean;
  /** The ids being dragged (from this or the other list), drawn dimmed where shown. */
  draggingIds?: readonly string[];
  /** What this table does with the drag in progress. */
  dropMode?: DropMode;
  /** Why a refused drag is refused, shown over the list while it hovers. */
  refusedText?: string;
  /** The per-row position actions: Move entries (Active), "Activate at…" (Inactive) or none. */
  rowMenu?: RowMenu;
  /** Whether the row menu also offers the mod's own "Groups…" and "Colour…" (organization is usable). */
  hasOrganizationActions?: boolean;
  /** Why moving is off (set while the list is not in load order); `null` when it is on. */
  reorderBlockedReason?: string | null;
}>();
const emit = defineEmits<{
  selectMod: [modId: string];
  focusMod: [modId: string];
  dragStart: [modId: string];
  dragEnd: [];
  /** A drop; `slot` is the gap before that row index (`mods.length` is after the last row). */
  drop: [slot: number];
  /** A Ctrl/Cmd+click, the checkbox or Space: toggle this row's selection. */
  toggleSelect: [modId: string];
  /** A plain click or plain keyboard movement: the range anchor follows to this row. */
  anchorRow: [modId: string];
  /** A Shift+click or Shift+Space: add the range from the selection anchor to this row. */
  rangeSelect: [modId: string];
  /** Shift+Arrow: extend the selection from the anchor to the row focus moved to. */
  extendSelect: [fromModId: string, toModId: string];
  /** Ctrl/Cmd+A: select every shown row. */
  selectAll: [];
  /** Escape with a selection: clear it. */
  clearSelection: [];
  /** One shown row up or down, from the Move menu or Alt+Arrow. */
  moveStep: [modId: string, direction: "up" | "down"];
  menuAction: [modId: string, action: MenuAction];
  /** A double click: activate this inactive mod or deactivate this active one, as the buttons do. */
  toggleActive: [modId: string];
  /** A swatch in the row menu: set this mod's own colour, `null` for no colour. */
  setColor: [modId: string, color: ColorTokenDto | null];
  /** A group chip or dot on a row: scope the lists to that group. */
  scopeGroup: [groupId: number];
}>();
const { t } = useI18n();
const modLabel = useModLabel();

const scrollElementRef = useTemplateRef<HTMLDivElement>("scrollElement");
const ROW_HEIGHT_PX = 40;

// The whole options object is one `computed` — see `OrderTable.vue`'s
// identical comment for why.
const virtualizer = useVirtualizer(
  computed(() => ({
    count: mods.length,
    getScrollElement: () => scrollElementRef.value,
    estimateSize: () => ROW_HEIGHT_PX,
    overscan: 12,
  })),
);

interface VisibleRow {
  virtualRow: VirtualItem;
  mod: ModListRowDto;
}

const visibleRows = computed<VisibleRow[]>(() =>
  virtualizer.value
    .getVirtualItems()
    .map((virtualRow) => ({ virtualRow, mod: mods[virtualRow.index] }))
    .filter((entry): entry is VisibleRow => entry.mod !== undefined),
);
const totalSize = computed(() => virtualizer.value.getTotalSize());

const lastFocusedId = defineModel<string | null>("focusedId", { default: null });

const updatedText = useUpdatedText();

const selectedSet = computed(() => new Set(selectedIds));

function isSelected(modId: string): boolean {
  return selectedSet.value.has(modId);
}

function openRow(modId: string): void {
  emit("selectMod", modId);
}

const { onRowClick, onRowMouseDown, onRowDblClick, onCheckboxClick } = useRowPointer<string>({
  isSelectable: () => selectable,
  actions: {
    open: openRow,
    anchor: (modId) => emit("anchorRow", modId),
    toggle: (modId) => emit("toggleSelect", modId),
    range: (modId) => emit("rangeSelect", modId),
    toggleActive: (modId) => emit("toggleActive", modId),
  },
});

/** Whether rows carry a ⋮ button and a menu at all. */
const hasRowMenu = computed(() => hasRowMenuFor(rowMenu, hasOrganizationActions));
const isDirectActivate = computed(() => isDirectActivateFor(rowMenu, hasOrganizationActions));

const { rovingIndex, onRowFocus, onRowKeydown, focusRow } = useRowKeys({
  mods: () => mods,
  focusedId: lastFocusedId,
  isSelectable: () => selectable,
  hasSelection: () => selectedIds.length > 0,
  rowMenu: () => rowMenu,
  hasRowMenu: () => hasRowMenu.value,
  scrollToIndex: (index) => virtualizer.value.scrollToIndex(index),
  getScroller: () => scrollElementRef.value,
  actions: {
    open: openRow,
    setAnchor: (modId) => emit("anchorRow", modId),
    toggleSelected: (modId) => emit("toggleSelect", modId),
    selectRange: (modId) => emit("rangeSelect", modId),
    extendSelection: (fromModId, toModId) => emit("extendSelect", fromModId, toModId),
    selectAll: () => emit("selectAll"),
    clearSelection: () => emit("clearSelection"),
    moveStep: (modId, direction) => emit("moveStep", modId, direction),
    openMenu: (event, modId) => rowMenuRef.value?.open(event, modId),
    focusMod: (modId) => emit("focusMod", modId),
  },
});
defineExpose({ focusRow });

const draggingSet = computed(() => new Set(draggingIds));
const {
  dragSlot,
  isHoveringPane,
  isHoveringRefused,
  onRowDragStart,
  onRowDragEnd,
  onDragOver,
  onDragLeave,
  onDrop,
} = useRowDrag({
  getScroller: () => scrollElementRef.value,
  rowCount: () => mods.length,
  rowHeightPx: ROW_HEIGHT_PX,
  isDraggable: () => draggable,
  dropMode: () => dropMode,
  actions: {
    dragStart: (modId) => emit("dragStart", modId),
    dragEnd: () => emit("dragEnd"),
    drop: (slot) => emit("drop", slot),
  },
});

// One popup menu for the whole table: a menu per virtualized row would be a menu per row.
const rowMenuRef = useTemplateRef<{ open: (event: Event, modId: string) => void }>("rowMenu");
</script>

<template>
  <div class="relative flex min-h-0 flex-1 flex-col">
    <div
      ref="scrollElement"
      class="surface-card @container min-h-0 flex-1 overflow-y-auto"
      :class="isHoveringPane ? 'outline-accent outline-2 -outline-offset-2' : ''"
      role="grid"
      :aria-multiselectable="selectable ? 'true' : undefined"
      :aria-rowcount="mods.length"
      :aria-label="t('mods.table.regionLabel')"
      data-testid="mod-table"
      @dragover="onDragOver"
      @dragleave="onDragLeave"
      @drop="onDrop"
    >
      <span
        class="sr-only"
        data-testid="mod-table-roving-hint"
      >{{ t("mods.table.rovingHint") }}</span>
      <div :style="{ height: `${totalSize}px`, position: 'relative', width: '100%' }">
        <div
          v-if="dragSlot !== null"
          class="bg-accent pointer-events-none absolute left-0 z-10 h-0.5 w-full"
          :style="{ top: `${dragSlot * ROW_HEIGHT_PX - 1}px` }"
          :data-slot="dragSlot"
          data-testid="mod-table-drop-indicator"
        />
        <div
          v-for="{ virtualRow, mod } in visibleRows"
          :key="mod.modId"
          role="row"
          :aria-rowindex="virtualRow.index + 1"
          :tabindex="rovingIndex === null ? (virtualRow.index === 0 ? 0 : -1) : virtualRow.index === rovingIndex ? 0 : -1"
          :aria-current="mod.modId === openModId ? 'true' : undefined"
          :aria-selected="selectable ? isSelected(mod.modId) : undefined"
          :draggable="draggable"
          class="border-border-subtle focus-visible:bg-accent-soft absolute top-0 left-0 flex w-full cursor-pointer items-center gap-2 overflow-hidden border-b @lg:gap-3 px-3 text-sm focus-visible:outline"
          :class="[
            draggingSet.has(mod.modId) ? 'opacity-50' : '',
            mod.modId === openModId ? 'bg-accent-soft' : 'hover:bg-surface-2',
            (selectable && isSelected(mod.modId)) || mod.modId === openModId
              ? 'inset-ring-accent inset-ring-2'
              : '',
          ]"
          :style="{
            height: `${virtualRow.size}px`,
            transform: `translateY(${virtualRow.start}px)`,
          }"
          :data-testid="`mod-row-${mod.modId}`"
          @click="onRowClick($event, mod.modId)"
          @mousedown="onRowMouseDown"
          @dblclick="onRowDblClick($event, mod.modId)"
          @keydown="onRowKeydown($event, mod.modId)"
          @focus="onRowFocus(mod.modId)"
          @dragstart="onRowDragStart($event, mod.modId)"
          @dragend="onRowDragEnd"
        >
          <ModRowCells
            :mod="mod"
            :selectable="selectable"
            :is-selected="isSelected(mod.modId)"
            :show-active-columns="showActiveColumns"
            :show-updated="showUpdated"
            :show-position="showPosition"
            :has-row-menu="hasRowMenu"
            :is-direct-activate="isDirectActivate"
            :label="modLabel.label(mod.modId)"
            :label-title="modLabel.titleFor(mod.modId)"
            :updated-text="showUpdated ? updatedText(mod) : ''"
            @toggle-select="emit('toggleSelect', mod.modId)"
            @checkbox-click="onCheckboxClick($event, mod.modId)"
            @scope-group="(groupId: number) => emit('scopeGroup', groupId)"
            @open-menu="rowMenuRef?.open($event, mod.modId)"
          />
        </div>
      </div>
    </div>
    <p
      v-if="isHoveringRefused && refusedText !== ''"
      class="bg-surface-1 text-status-danger border-border pointer-events-none absolute inset-x-4 top-4 z-20 rounded border px-3 py-2 text-center text-sm shadow"
      data-testid="mod-table-refused-text"
    >
      {{ refusedText }}
    </p>
    <ModRowMenu
      v-if="hasRowMenu"
      ref="rowMenu"
      :mods="mods"
      :selected-ids="selectedIds"
      :row-menu="rowMenu"
      :has-organization-actions="hasOrganizationActions"
      :reorder-blocked-reason="reorderBlockedReason"
      @move-step="(modId: string, direction: 'up' | 'down') => emit('moveStep', modId, direction)"
      @menu-action="(modId: string, action: MenuAction) => emit('menuAction', modId, action)"
      @set-color="(modId: string, color: ColorTokenDto | null) => emit('setColor', modId, color)"
    />
  </div>
</template>
