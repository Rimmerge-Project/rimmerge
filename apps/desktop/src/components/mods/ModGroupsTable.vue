<script setup lang="ts">
import { useVirtualizer, type VirtualItem } from "@tanstack/vue-virtual";
import { computed, ref, useTemplateRef, watch } from "vue";
import { useI18n } from "vue-i18n";

import ModFolderMenu from "@/components/mods/ModFolderMenu.vue";
import ModFolderRow from "@/components/mods/ModFolderRow.vue";
import ModRowCells from "@/components/mods/ModRowCells.vue";
import ModRowMenu from "@/components/mods/ModRowMenu.vue";
import { useFolderDropTarget } from "@/composables/useFolderDropTarget";
import { useModLabel } from "@/composables/useModLabel";
import { useRowDrag } from "@/composables/useRowDrag";
import { useRowPointer } from "@/composables/useRowPointer";
import { useTreeRowKeys } from "@/composables/useTreeRowKeys";
import { useUpdatedText } from "@/composables/useUpdatedText";
import { formatList } from "@/i18n/format";
import type { ColorTokenDto } from "@/types/generated/ColorTokenDto";
import type { FolderKeyDto } from "@/types/generated/FolderKeyDto";
import type { GroupedModListDto } from "@/types/generated/GroupedModListDto";
import type { ModListRowDto } from "@/types/generated/ModListRowDto";
import { folderTestToken, type TreeRow, treeRows } from "@/utils/groupedEntries";
import {
  type DropMode,
  type FolderAction,
  hasRowMenuFor,
  isDirectActivateFor,
  type MenuAction,
  type RowMenu,
} from "@/utils/modRowMenu";

type FolderTreeRow = Extract<TreeRow, { kind: "folder" }>;
type ModTreeRow = Extract<TreeRow, { kind: "mod" }>;

/**
 * The Groups view of one Mods pane: the folders and the mods inside them as one virtualized
 * treegrid. The backend lays the lines out (which folder holds which copy, in what order, what is
 * folded, how many it matches); this draws them, keeps one roving tab stop, and reports what the
 * user did. A mod in several groups is a row under each, with its own entry key, but selection is
 * per mod id. Reordering is off in this view; a group folder takes a drop as an add to that group.
 */
const {
  grouped,
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
  isFiltered = false,
} = defineProps<{
  grouped: GroupedModListDto;
  selectedIds?: readonly string[];
  showActiveColumns?: boolean;
  /** Adds each row's last-updated time (the list is sorted by it). */
  showUpdated?: boolean;
  /** Adds each row's load-order position (the Active list). */
  showPosition?: boolean;
  /** The mod open in the info panel: every copy of it is drawn with the accent ring and tint. */
  openModId?: string | null;
  draggable?: boolean;
  draggingIds?: readonly string[];
  /** What the pane does with the drag in progress; a group folder takes any drag but "off". */
  dropMode?: DropMode;
  refusedText?: string;
  rowMenu?: RowMenu;
  hasOrganizationActions?: boolean;
  reorderBlockedReason?: string | null;
  /** Whether the query carries a search, tag, source or group filter (words the empty state). */
  isFiltered?: boolean;
}>();
const emit = defineEmits<{
  selectMod: [modId: string];
  focusMod: [modId: string];
  dragStart: [modId: string];
  dragEnd: [];
  /** A drop that is not on a folder header: the pane's own drop (activate at the end). */
  drop: [slot: number];
  /** A Ctrl/Cmd+click, the checkbox or Space: toggle this mod; `rowKey` is the row it was done on. */
  toggleSelect: [modId: string, rowKey: string];
  /** A plain click or plain keyboard movement: the range anchor follows to this row. */
  anchorRow: [rowKey: string];
  /** A Shift+click or Shift+Space: add the range from the anchor to this row. */
  rangeSelect: [rowKey: string];
  /** Shift+Arrow: extend the selection from the anchor row to the row focus moved to. */
  extendSelect: [fromRowKey: string, toRowKey: string];
  selectAll: [];
  clearSelection: [];
  menuAction: [modId: string, action: MenuAction];
  moveStep: [modId: string, direction: "up" | "down"];
  toggleActive: [modId: string];
  setColor: [modId: string, color: ColorTokenDto | null];
  scopeGroup: [groupId: number];
  /** A folder header was folded (`true`) or opened (`false`). */
  setFolded: [folder: FolderKeyDto, isCollapsed: boolean];
  /** Mods were dropped on a group's header: add them to that group. */
  dropOnFolder: [groupId: number];
  /** A choice in a folder's menu. */
  folderAction: [folder: FolderKeyDto, action: FolderAction];
}>();
const { t, locale } = useI18n();
const modLabel = useModLabel();
const updatedText = useUpdatedText();

const scrollElementRef = useTemplateRef<HTMLDivElement>("scrollElement");
const ROW_HEIGHT_PX = 40;
/** Each level of folders indents its rows this far. */
const INDENT_PX = 16;
const BASE_PADDING_PX = 12;

const rows = computed(() => treeRows(grouped.entries));
const modsById = computed(() => new Map(grouped.items.map((mod) => [mod.modId, mod])));

const virtualizer = useVirtualizer(
  computed(() => ({
    count: rows.value.length,
    getScrollElement: () => scrollElementRef.value,
    estimateSize: () => ROW_HEIGHT_PX,
    overscan: 12,
  })),
);

/** A rendered line: a header, or a copy of a mod with the row it joins to. */
type VisibleRow =
  | { kind: "folder"; virtualRow: VirtualItem; row: FolderTreeRow }
  | { kind: "mod"; virtualRow: VirtualItem; row: ModTreeRow; mod: ModListRowDto };

function visibleRowAt(virtualRow: VirtualItem): VisibleRow | null {
  const row = rows.value[virtualRow.index];
  if (row === undefined) {
    return null;
  }
  if (row.kind === "folder") {
    return { kind: "folder", virtualRow, row };
  }
  // Every copy names a row of `items` (the backend's contract); one that does not is not drawn.
  const mod = modsById.value.get(row.modId);
  return mod === undefined ? null : { kind: "mod", virtualRow, row, mod };
}

const visibleRows = computed<VisibleRow[]>(() =>
  virtualizer.value
    .getVirtualItems()
    .map(visibleRowAt)
    .filter((entry): entry is VisibleRow => entry !== null),
);
const totalSize = computed(() => virtualizer.value.getTotalSize());

const selectedSet = computed(() => new Set(selectedIds));
const draggingSet = computed(() => new Set(draggingIds));
const hiddenEntryCount = computed(() => grouped.totalEntries - grouped.entries.length);
const isCut = computed(() => hiddenEntryCount.value > 0);

const hasRowMenu = computed(() => hasRowMenuFor(rowMenu, hasOrganizationActions));
const isDirectActivate = computed(() => isDirectActivateFor(rowMenu, hasOrganizationActions));

const focusedKey = ref<string | null>(null);

const rowMenuRef = useTemplateRef<{ open: (event: Event, modId: string) => void }>("rowMenu");
const folderMenu = useTemplateRef<{ open: (event: Event, folder: FolderKeyDto) => void }>(
  "folderMenu",
);

const { rovingIndex, onRowFocus, onRowKeydown, focusKey, focusMod } = useTreeRowKeys({
  rows: () => rows.value,
  focusedKey,
  hasSelection: () => selectedIds.length > 0,
  hasModMenu: () => hasRowMenu.value,
  scrollToIndex: (index) => virtualizer.value.scrollToIndex(index),
  getScroller: () => scrollElementRef.value,
  actions: {
    open: (modId) => emit("selectMod", modId),
    toggleSelected: (modId, rowKey) => emit("toggleSelect", modId, rowKey),
    setAnchor: (rowKey) => emit("anchorRow", rowKey),
    selectRange: (rowKey) => emit("rangeSelect", rowKey),
    extendSelection: (fromKey, toKey) => emit("extendSelect", fromKey, toKey),
    selectAll: () => emit("selectAll"),
    clearSelection: () => emit("clearSelection"),
    setFolded: (folder, isCollapsed) => emit("setFolded", folder, isCollapsed),
    openMenu: (event, row) =>
      row.kind === "folder"
        ? folderMenu.value?.open(event, row.folder)
        : rowMenuRef.value?.open(event, row.modId),
    focusMod: (modId) => emit("focusMod", modId),
  },
});
defineExpose({ focusRow: focusMod, focusKey });

const pointer = useRowPointer<ModTreeRow>({
  isSelectable: () => true,
  actions: {
    open: (row) => emit("selectMod", row.modId),
    anchor: (row) => emit("anchorRow", row.key),
    toggle: (row) => emit("toggleSelect", row.modId, row.key),
    range: (row) => emit("rangeSelect", row.key),
    toggleActive: (row) => emit("toggleActive", row.modId),
  },
});

/**
 * A plain click on a folder header folds or opens it, and the anchor and roving focus follow. A
 * Ctrl/Cmd/Shift click is a selection gesture, which a header takes no part in, and the second
 * click of a double click is not a second fold.
 */
function onFolderClick(event: MouseEvent, row: FolderTreeRow): void {
  if (event.ctrlKey || event.metaKey || event.shiftKey || event.detail > 1) {
    return;
  }
  emit("anchorRow", row.key);
  focusedKey.value = row.key;
  emit("setFolded", row.folder, !row.entry.isCollapsed);
}

const {
  isHoveringPane,
  isHoveringRefused,
  onRowDragStart,
  onRowDragEnd,
  onDragOver,
  onDragLeave,
  onDrop,
  clearHover,
} = useRowDrag({
  getScroller: () => scrollElementRef.value,
  rowCount: () => rows.value.length,
  rowHeightPx: ROW_HEIGHT_PX,
  isDraggable: () => draggable,
  dropMode: () => dropMode,
  actions: {
    dragStart: (modId) => emit("dragStart", modId),
    dragEnd: () => emit("dragEnd"),
    drop: (slot) => emit("drop", slot),
  },
});

const folderDrop = useFolderDropTarget({
  isDragActive: () => dropMode !== "off",
  openFolder: (folder) => emit("setFolded", folder, false),
  dropOnGroup: (groupId) => emit("dropOnFolder", groupId),
  clearPaneHover: clearHover,
});

// A drag that ends (a drop, a cancel) leaves no hover behind.
watch(
  () => dropMode,
  (mode) => {
    if (mode === "off") {
      folderDrop.clear();
    }
  },
);

function folderName(row: FolderTreeRow): string {
  return row.entry.folder.kind === "group" ? row.entry.folder.name : t("mods.groupsView.ungrouped");
}

/** The header's spoken name: its name, how many mods and whether it is open. */
function folderLabel(row: FolderTreeRow): string {
  const count = row.entry.matched;
  return row.entry.isCollapsed
    ? t("mods.groupsView.folderLabelCollapsed", { name: folderName(row), count }, count)
    : t("mods.groupsView.folderLabelExpanded", { name: folderName(row), count }, count);
}

/** The names of the mod's other groups, for the "also in" tooltip. */
function otherGroupNames(mod: ModListRowDto, row: ModTreeRow): string {
  const current = row.folder.kind === "group" ? row.folder.groupId : null;
  return formatList(
    locale.value,
    mod.groups.filter((group) => group.id !== current).map((group) => group.name),
  );
}

function rowTabindex(index: number): 0 | -1 {
  const roving = rovingIndex.value;
  return (roving === null ? index === 0 : index === roving) ? 0 : -1;
}

function indent(level: number): string {
  return `${BASE_PADDING_PX + (level - 1) * INDENT_PX}px`;
}

function isHovered(row: FolderTreeRow): boolean {
  return folderDrop.hoverKey.value === row.key;
}
</script>

<template>
  <div class="relative flex min-h-0 flex-1 flex-col">
    <!-- Outside the treegrid: an empty answer has no rows, and a status line is not one. -->
    <p
      v-if="rows.length === 0"
      class="text-text-muted shrink-0 px-3 py-2 text-sm"
      data-testid="mods-groups-empty"
    >
      {{ isFiltered ? t("mods.groupsView.emptyFiltered") : t("mods.groupsView.empty") }}
    </p>
    <div
      ref="scrollElement"
      class="surface-card @container min-h-0 flex-1 overflow-y-auto"
      :class="isHoveringPane ? 'outline-accent outline-2 -outline-offset-2' : ''"
      role="treegrid"
      aria-multiselectable="true"
      :aria-rowcount="rows.length"
      :aria-label="t('mods.groupsView.regionLabel')"
      data-testid="mod-groups-table"
      @dragover="onDragOver"
      @dragleave="onDragLeave"
      @drop="onDrop"
    >
      <span
        class="sr-only"
        data-testid="mod-groups-roving-hint"
      >{{ t("mods.groupsView.rovingHint") }}</span>
      <div :style="{ height: `${totalSize}px`, position: 'relative', width: '100%' }">
        <template
          v-for="item in visibleRows"
          :key="item.row.key"
        >
          <div
            v-if="item.kind === 'folder'"
            role="row"
            :aria-rowindex="item.virtualRow.index + 1"
            :aria-level="item.row.level"
            :aria-expanded="!item.row.entry.isCollapsed"
            :aria-label="folderLabel(item.row)"
            :tabindex="rowTabindex(item.virtualRow.index)"
            :data-entry-key="item.row.key"
            :data-testid="`mods-folder-${folderTestToken(item.row.folder)}`"
            class="border-border-subtle bg-surface-1 hover:bg-surface-2 focus-visible:bg-accent-soft absolute top-0 left-0 flex w-full cursor-pointer items-center gap-2 overflow-hidden border-b px-3 text-sm focus-visible:outline"
            :class="
              isHovered(item.row)
                ? folderDrop.isRefusedHover.value
                  ? 'inset-ring-status-danger inset-ring-2'
                  : 'inset-ring-accent bg-accent-soft inset-ring-2'
                : ''
            "
            :style="{
              height: `${item.virtualRow.size}px`,
              transform: `translateY(${item.virtualRow.start}px)`,
              paddingLeft: indent(item.row.level),
            }"
            @click="onFolderClick($event, item.row)"
            @mousedown="pointer.onRowMouseDown"
            @keydown="onRowKeydown($event, item.row)"
            @focus="onRowFocus(item.row.key)"
            @contextmenu.prevent="folderMenu?.open($event, item.row.folder)"
            @dragover="folderDrop.onHeaderDragOver($event, item.row)"
            @dragleave="folderDrop.onHeaderDragLeave($event, item.row)"
            @drop="folderDrop.onHeaderDrop($event, item.row)"
          >
            <ModFolderRow
              :row="item.row"
              @open-menu="folderMenu?.open($event, item.row.folder)"
            />
          </div>
          <div
            v-else
            role="row"
            :aria-rowindex="item.virtualRow.index + 1"
            :aria-level="item.row.level"
            :aria-selected="selectedSet.has(item.row.modId)"
            :aria-current="item.row.modId === openModId ? 'true' : undefined"
            :tabindex="rowTabindex(item.virtualRow.index)"
            :draggable="draggable"
            :data-entry-key="item.row.key"
            :data-mod-id="item.row.modId"
            :data-testid="`mod-entry-${folderTestToken(item.row.folder)}-${item.row.modId}`"
            class="border-border-subtle focus-visible:bg-accent-soft absolute top-0 left-0 flex w-full cursor-pointer items-center gap-2 overflow-hidden border-b @lg:gap-3 px-3 text-sm focus-visible:outline"
            :class="[
              draggingSet.has(item.row.modId) ? 'opacity-50' : '',
              item.row.modId === openModId ? 'bg-accent-soft' : 'hover:bg-surface-2',
              selectedSet.has(item.row.modId) || item.row.modId === openModId
                ? 'inset-ring-accent inset-ring-2'
                : '',
            ]"
            :style="{
              height: `${item.virtualRow.size}px`,
              transform: `translateY(${item.virtualRow.start}px)`,
              paddingLeft: indent(item.row.level),
            }"
            @click="pointer.onRowClick($event, item.row)"
            @mousedown="pointer.onRowMouseDown"
            @dblclick="pointer.onRowDblClick($event, item.row)"
            @keydown="onRowKeydown($event, item.row)"
            @focus="onRowFocus(item.row.key)"
            @dragstart="onRowDragStart($event, item.row.modId)"
            @dragend="onRowDragEnd"
          >
            <ModRowCells
              :mod="item.mod"
              selectable
              :is-selected="selectedSet.has(item.row.modId)"
              :show-active-columns="showActiveColumns"
              :show-updated="showUpdated"
              :show-position="showPosition"
              :has-row-menu="hasRowMenu"
              :is-direct-activate="isDirectActivate"
              :label="modLabel.label(item.row.modId)"
              :label-title="modLabel.titleFor(item.row.modId)"
              :updated-text="showUpdated ? updatedText(item.mod) : ''"
              @toggle-select="emit('toggleSelect', item.row.modId, item.row.key)"
              @checkbox-click="pointer.onCheckboxClick($event, item.row)"
              @scope-group="(groupId: number) => emit('scopeGroup', groupId)"
              @open-menu="rowMenuRef?.open($event, item.row.modId)"
            >
              <template #afterName>
                <span
                  v-if="item.row.alsoIn > 0"
                  class="border-border text-text-muted shrink-0 rounded border px-1.5 text-xs"
                  :title="t('mods.groupsView.alsoInTitle', { groups: otherGroupNames(item.mod, item.row) })"
                  data-testid="mod-also-in"
                >
                  <span aria-hidden="true">{{ t("mods.groupsView.alsoInShort", { count: item.row.alsoIn }) }}</span>
                  <span class="sr-only">{{
                    t("mods.groupsView.alsoInLong", { count: item.row.alsoIn, groups: otherGroupNames(item.mod, item.row) }, item.row.alsoIn)
                  }}</span>
                </span>
              </template>
            </ModRowCells>
          </div>
        </template>
      </div>
    </div>
    <p
      v-if="isCut"
      class="text-text-muted shrink-0 px-3 py-1 text-xs"
      data-testid="mods-groups-cut-note"
    >
      {{ t("mods.groupsView.cutNote", { hidden: hiddenEntryCount }, hiddenEntryCount) }}
    </p>
    <p
      v-if="isHoveringRefused && refusedText !== ''"
      class="bg-surface-1 text-status-danger border-border pointer-events-none absolute inset-x-4 top-4 z-20 rounded border px-3 py-2 text-center text-sm shadow"
      data-testid="mod-table-refused-text"
    >
      {{ refusedText }}
    </p>
    <p
      v-if="folderDrop.isRefusedHover.value"
      class="bg-surface-1 text-status-danger border-border pointer-events-none absolute inset-x-4 top-4 z-20 rounded border px-3 py-2 text-center text-sm shadow"
      data-testid="mods-groups-ungrouped-refused"
    >
      {{ t("mods.groupsView.ungroupedRefusesDrop") }}
    </p>
    <ModRowMenu
      v-if="hasRowMenu"
      ref="rowMenu"
      :mods="grouped.items"
      :selected-ids="selectedIds"
      :row-menu="rowMenu"
      :has-organization-actions="hasOrganizationActions"
      :reorder-blocked-reason="reorderBlockedReason"
      @move-step="(modId: string, direction: 'up' | 'down') => emit('moveStep', modId, direction)"
      @menu-action="(modId: string, action: MenuAction) => emit('menuAction', modId, action)"
      @set-color="(modId: string, color: ColorTokenDto | null) => emit('setColor', modId, color)"
    />
    <ModFolderMenu
      ref="folderMenu"
      @action="(folder: FolderKeyDto, action: FolderAction) => emit('folderAction', folder, action)"
    />
  </div>
</template>
