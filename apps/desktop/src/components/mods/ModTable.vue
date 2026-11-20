<script setup lang="ts">
import { useVirtualizer, type VirtualItem } from "@tanstack/vue-virtual";
import Checkbox from "primevue/checkbox";
import Menu, { type MenuMethods } from "primevue/menu";
import type { MenuItem } from "primevue/menuitem";
import { computed, ref, useTemplateRef } from "vue";
import { useI18n } from "vue-i18n";

import MembershipBadge from "@/components/mods/MembershipBadge.vue";
import ModRowMenuButton from "@/components/mods/ModRowMenuButton.vue";
import ModRowPosition from "@/components/mods/ModRowPosition.vue";
import ModRowScanFacts from "@/components/mods/ModRowScanFacts.vue";
import ModRowStatePill from "@/components/mods/ModRowStatePill.vue";
import SourceBadge from "@/components/mods/SourceBadge.vue";
import { useModLabel } from "@/composables/useModLabel";
import { useRowDrag } from "@/composables/useRowDrag";
import { useRowKeys } from "@/composables/useRowKeys";
import { useTranslateMessage } from "@/composables/useTranslateMessage";
import { descriptor } from "@/i18n/messageDescriptor";
import type { ModListRowDto } from "@/types/generated/ModListRowDto";
import { updateUnavailableLabel } from "@/utils/modListSort";
import { anchorForStep, moveSetFor } from "@/utils/modMove";

/** What this table's rows may be dragged to, set by the page from the drag in progress. */
export type DropMode =
  /** No drag is in progress. */
  | "off"
  /** Accepts the drag at a position between rows (a line shows where). */
  | "position"
  /** Accepts the drag anywhere on the list (it has no order of its own to drop into). */
  | "pane"
  /** Refuses the drag (the not-allowed cursor shows, with the reason). */
  | "refused";

/** Which row action button the rows carry. */
export type RowMenu = "none" | "move" | "activate";

/** A Move menu entry that is not a one-step move. */
export type MenuAction = "top" | "bottom" | "after" | "activateAt";

const {
  mods,
  selectable = false,
  showActiveColumns = true,
  showUpdated = false,
  showPosition = false,
  draggable = false,
  draggingIds = [],
  dropMode = "off",
  refusedText = "",
  rowMenu = "none",
  reorderBlockedReason = null,
} = defineProps<{
  mods: ModListRowDto[];
  /** Adds a checkbox column bound to {@link selectedIds}. */
  selectable?: boolean;
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
   * Adds each row's load-order position, for the Active list while it is sorted
   * by anything but load order: the sort must not be mistaken for the order.
   */
  showPosition?: boolean;
  /** Whether a row can be picked up with the pointer. */
  draggable?: boolean;
  /** The ids being dragged (from this or the other list), drawn dimmed where shown. */
  draggingIds?: readonly string[];
  /** What this table does with the drag in progress. */
  dropMode?: DropMode;
  /** Why a refused drag is refused, shown over the list while it hovers. */
  refusedText?: string;
  /** The per-row action: a Move menu (Active), "Activate at…" (Inactive) or none. */
  rowMenu?: RowMenu;
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
  /** One shown row up or down, from the Move menu or Alt+Arrow. */
  moveStep: [modId: string, direction: "up" | "down"];
  menuAction: [modId: string, action: MenuAction];
}>();
/** Checked ids, only meaningful while {@link selectable} is set. */
const selectedIds = defineModel<string[]>("selectedIds", { default: () => [] });
const { t, locale } = useI18n();
const tm = useTranslateMessage();
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

// One formatter for the whole table, not one per rendered row.
const dateFormat = computed(() => new Intl.DateTimeFormat(locale.value, { dateStyle: "medium" }));

/** A row's update time as a date in the app language, or the reason it has none. */
function updatedText(mod: ModListRowDto): string {
  const updated = mod.updated;
  if (updated.kind === "known") {
    const date = new Date(updated.seconds * 1000);
    // A date past the JS range makes `format` throw during render, which would
    // blank the whole list; such a time reads as unknown instead.
    if (Number.isFinite(date.getTime())) {
      return dateFormat.value.format(date);
    }
    return tm(descriptor("mods.sort.updatedUnavailable.unknown"));
  }
  const reason = updateUnavailableLabel(updated);
  return reason === null ? "" : tm(reason);
}

function isSelected(modId: string): boolean {
  return selectedIds.value.includes(modId);
}

function toggleSelected(modId: string): void {
  selectedIds.value = isSelected(modId)
    ? selectedIds.value.filter((id) => id !== modId)
    : [...selectedIds.value, modId];
}

function openRow(modId: string): void {
  emit("selectMod", modId);
}

const { rovingIndex, onRowFocus, onRowKeydown, focusRow } = useRowKeys({
  mods: () => mods,
  focusedId: lastFocusedId,
  isSelectable: () => selectable,
  rowMenu: () => rowMenu,
  scrollToIndex: (index) => virtualizer.value.scrollToIndex(index),
  getScroller: () => scrollElementRef.value,
  actions: {
    open: openRow,
    toggleSelected,
    moveStep: (modId, direction) => emit("moveStep", modId, direction),
    openMenu: (event, modId) => onRowMenuClick(event, modId),
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
const menu = useTemplateRef<MenuMethods>("moveMenu");
const menuModId = ref<string | null>(null);

const menuItems = computed<MenuItem[]>(() => {
  const modId = menuModId.value;
  if (modId === null) {
    return [];
  }
  const moving = new Set(moveSetFor(modId, selectedIds.value, mods));
  const isBlocked = reorderBlockedReason !== null;
  // `testId` rides along on the item (a MenuItem accepts extra keys) for the item template.
  return [
    {
      label: t("mods.move.up"),
      testId: "mod-move-up",
      disabled: isBlocked || anchorForStep(mods, moving, "up") === null,
      command: () => emit("moveStep", modId, "up"),
    },
    {
      label: t("mods.move.down"),
      testId: "mod-move-down",
      disabled: isBlocked || anchorForStep(mods, moving, "down") === null,
      command: () => emit("moveStep", modId, "down"),
    },
    {
      label: t("mods.move.top"),
      testId: "mod-move-top",
      disabled: isBlocked,
      command: () => emit("menuAction", modId, "top"),
    },
    {
      label: t("mods.move.bottom"),
      testId: "mod-move-bottom",
      disabled: isBlocked,
      command: () => emit("menuAction", modId, "bottom"),
    },
    {
      label: t("mods.move.after"),
      testId: "mod-move-after",
      disabled: isBlocked,
      command: () => emit("menuAction", modId, "after"),
    },
  ];
});

/** The row button: the Active list opens the Move menu, the Inactive list goes straight to "Activate at…". */
function onRowMenuClick(event: Event, modId: string): void {
  if (rowMenu === "activate") {
    emit("menuAction", modId, "activateAt");
    return;
  }
  menuModId.value = modId;
  menu.value?.toggle(event);
}
</script>

<template>
  <div class="relative flex min-h-0 flex-1 flex-col">
    <div
      ref="scrollElement"
      class="surface-card @container min-h-0 flex-1 overflow-y-auto"
      :class="isHoveringPane ? 'outline-accent outline-2 -outline-offset-2' : ''"
      role="table"
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
          :tabindex="rovingIndex === null ? (virtualRow.index === 0 ? 0 : -1) : virtualRow.index === rovingIndex ? 0 : -1"
          :aria-current="mod.modId === lastFocusedId ? 'true' : undefined"
          :draggable="draggable"
          class="border-border-subtle hover:bg-surface-2 focus-visible:bg-accent-soft absolute top-0 left-0 flex w-full cursor-pointer items-center gap-2 overflow-hidden border-b @lg:gap-3 px-3 text-sm focus-visible:outline"
          :class="draggingSet.has(mod.modId) ? 'opacity-50' : ''"
          :style="{
            height: `${virtualRow.size}px`,
            transform: `translateY(${virtualRow.start}px)`,
          }"
          :data-testid="`mod-row-${mod.modId}`"
          @click="openRow(mod.modId)"
          @keydown="onRowKeydown($event, mod.modId)"
          @focus="onRowFocus(mod.modId)"
          @dragstart="onRowDragStart($event, mod.modId)"
          @dragend="onRowDragEnd"
        >
          <Checkbox
            v-if="selectable"
            class="shrink-0"
            binary
            :model-value="isSelected(mod.modId)"
            :data-testid="`mod-row-select-${mod.modId}`"
            @click.stop
            @update:model-value="toggleSelected(mod.modId)"
          />
          <ModRowPosition
            v-if="showPosition && mod.position !== null"
            :mod-id="mod.modId"
            :position="mod.position"
          />
          <SourceBadge :source="mod.source" />
          <span
            class="text-text min-w-12 flex-1 truncate font-medium"
            :title="modLabel.titleFor(mod.modId)"
            :data-testid="`mod-row-name-${mod.modId}`"
          >{{ modLabel.label(mod.modId) }}</span>
          <MembershipBadge :mark="mod.membership" />
          <ModRowStatePill
            :state="mod.state"
            :show-pending-scan="showActiveColumns"
          />
          <span
            v-if="showUpdated"
            class="text-text-muted w-20 min-w-0 truncate text-right text-xs @lg:w-28 @lg:shrink-0"
            :title="updatedText(mod)"
            :data-testid="`mod-row-updated-${mod.modId}`"
          >{{ updatedText(mod) }}</span>
          <ModRowScanFacts
            v-if="showActiveColumns"
            :state="mod.state"
          />
          <ModRowMenuButton
            v-if="rowMenu !== 'none'"
            :mod-id="mod.modId"
            :menu="rowMenu"
            :name="modLabel.label(mod.modId)"
            @open="onRowMenuClick($event, mod.modId)"
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
    <Menu
      v-if="rowMenu === 'move'"
      ref="moveMenu"
      popup
      :model="menuItems"
      data-testid="mod-move-menu"
    >
      <template #item="{ item, props }">
        <a
          v-bind="props.action"
          :data-testid="item['testId']"
        >
          {{ item.label }}
        </a>
      </template>
      <template
        v-if="reorderBlockedReason !== null"
        #end
      >
        <p
          class="text-text-muted max-w-64 px-3 py-2 text-xs"
          data-testid="mod-move-menu-reason"
        >
          {{ reorderBlockedReason }}
        </p>
      </template>
    </Menu>
  </div>
</template>
