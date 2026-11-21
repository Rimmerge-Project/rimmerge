<script setup lang="ts">
import { useVirtualizer, type VirtualItem } from "@tanstack/vue-virtual";
import Checkbox from "primevue/checkbox";
import Menu, { type MenuMethods } from "primevue/menu";
import type { MenuItem } from "primevue/menuitem";
import { computed, ref, useTemplateRef } from "vue";
import { useI18n } from "vue-i18n";

import MembershipBadge from "@/components/mods/MembershipBadge.vue";
import ModRowColor from "@/components/mods/ModRowColor.vue";
import ModRowGroups from "@/components/mods/ModRowGroups.vue";
import ModRowMenuButton from "@/components/mods/ModRowMenuButton.vue";
import ModRowPosition from "@/components/mods/ModRowPosition.vue";
import ModRowScanFacts from "@/components/mods/ModRowScanFacts.vue";
import ModRowStatePill from "@/components/mods/ModRowStatePill.vue";
import SourceBadge from "@/components/mods/SourceBadge.vue";
import { isKeyboardOpen, markFirstMenuItem } from "@/composables/popupMenuFocus";
import { useModLabel } from "@/composables/useModLabel";
import { useRowDrag } from "@/composables/useRowDrag";
import { useRowKeys } from "@/composables/useRowKeys";
import { useTranslateMessage } from "@/composables/useTranslateMessage";
import { descriptor } from "@/i18n/messageDescriptor";
import type { ColorTokenDto } from "@/types/generated/ColorTokenDto";
import type { ModListRowDto } from "@/types/generated/ModListRowDto";
import { updateUnavailableLabel } from "@/utils/modListSort";
import { anchorForStep, moveSetFor } from "@/utils/modMove";
import { COLOR_NAME_KEY, COLOR_SWATCH_CLASS, COLOR_TOKENS } from "@/utils/organization";

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

/** A row menu entry that is not a one-step move: a position, or one of the mod own organization dialogs. */
export type MenuAction = "top" | "bottom" | "after" | "activateAt" | "groups" | "color";

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
  /** Whether the row menu also offers the mod own "Groups…" and "Colour…" (organization is usable). */
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

const selectedSet = computed(() => new Set(selectedIds));

function isSelected(modId: string): boolean {
  return selectedSet.value.has(modId);
}

function openRow(modId: string): void {
  emit("selectMod", modId);
}

/**
 * Ctrl/Cmd+click toggles and Shift+click extends a range, neither opening the info panel; a plain
 * click only opens it and never touches the selection.
 */
function onRowClick(event: MouseEvent, modId: string): void {
  if (selectable && (event.ctrlKey || event.metaKey)) {
    emit("toggleSelect", modId);
    return;
  }
  if (selectable && event.shiftKey) {
    // Shift+click would otherwise also select the text between the two rows.
    window.getSelection()?.removeAllRanges();
    emit("rangeSelect", modId);
    return;
  }
  emit("anchorRow", modId);
  openRow(modId);
}

/** Descendants of a row that handle their own clicks, so a double click on them is not the row's. */
const ROW_CONTROL_SELECTOR = 'button, input, a, [role="checkbox"], .p-checkbox';

/** A double click would also select the word under the pointer; the row toggles instead. */
function onRowMouseDown(event: MouseEvent): void {
  if (event.detail > 1) {
    event.preventDefault();
  }
}

/**
 * A plain double click (no Ctrl/Cmd/Shift, which belong to selection) asks to toggle the row's
 * activation. Its two clicks already ran `onRowClick`, which only opens the info panel and moves
 * the anchor, so it never fights the single click.
 */
function onRowDblClick(event: MouseEvent, modId: string): void {
  if (event.ctrlKey || event.metaKey || event.shiftKey) {
    return;
  }
  // A double click on a control inside the row (checkbox, menu button, group chip) belongs to it.
  if (event.target instanceof Element && event.target.closest(ROW_CONTROL_SELECTOR) !== null) {
    return;
  }
  emit("toggleActive", modId);
}

/**
 * A Shift+click on the row checkbox extends a range like one on the row; any other click is left
 * to the checkbox, which toggles through its own change event.
 */
function onCheckboxClick(event: MouseEvent, modId: string): void {
  if (event.shiftKey) {
    event.preventDefault();
    window.getSelection()?.removeAllRanges();
    emit("rangeSelect", modId);
  }
}

/** Whether rows carry a ⋮ button and a menu at all. */
const hasRowMenu = computed(() => rowMenu !== "none" || hasOrganizationActions);

/**
 * Whether the menu would hold one entry only: the Inactive list's "Activate at…" while
 * organization is not usable. The button then runs it directly instead of opening a one-item menu.
 */
const isDirectActivate = computed(() => rowMenu === "activate" && !hasOrganizationActions);

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
const menuModColor = computed<ColorTokenDto | null>(
  () => mods.find((mod) => mod.modId === menuModId.value)?.color ?? null,
);

/** A swatch in the menu: set the colour in one click and close the menu. */
function pickColor(color: ColorTokenDto | null): void {
  const modId = menuModId.value;
  menu.value?.hide();
  if (modId !== null) {
    emit("setColor", modId, color);
  }
}

const menuItems = computed<MenuItem[]>(() => {
  const modId = menuModId.value;
  if (modId === null) {
    return [];
  }
  // `testId` rides along on the item (a MenuItem accepts extra keys) for the item template.
  const positionItems = rowMenu === "move" ? moveItems(modId) : activateItems(modId);
  if (!hasOrganizationActions) {
    return positionItems;
  }
  const color = mods.find((mod) => mod.modId === modId)?.color ?? null;
  const organizationItems: MenuItem[] = [
    {
      label: t("mods.groups.rowMenu.groups"),
      testId: "mod-menu-groups",
      command: () => emit("menuAction", modId, "groups"),
    },
    {
      label:
        color === null
          ? t("mods.groups.rowMenu.color")
          : t("mods.groups.rowMenu.colorCurrent", { name: t(COLOR_NAME_KEY[color]) }),
      testId: "mod-menu-color",
      command: () => emit("menuAction", modId, "color"),
    },
  ];
  return positionItems.length === 0
    ? organizationItems
    : [...positionItems, { separator: true }, ...organizationItems];
});

function activateItems(modId: string): MenuItem[] {
  if (rowMenu !== "activate") {
    return [];
  }
  return [
    {
      label: t("mods.move.activateAtItem"),
      testId: "mod-activate-at",
      command: () => emit("menuAction", modId, "activateAt"),
    },
  ];
}

function moveItems(modId: string): MenuItem[] {
  const moving = new Set(moveSetFor(modId, selectedIds, mods));
  const isBlocked = reorderBlockedReason !== null;
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
}

/** Whether the open menu came from the keyboard, so its first item is marked once it shows. */
let isMenuFromKeyboard = false;

/**
 * The row button (and Shift+F10): opens the one shared menu for this row, or, with a single
 * possible entry, runs it.
 */
function onRowMenuClick(event: Event, modId: string): void {
  if (isDirectActivate.value) {
    emit("menuAction", modId, "activateAt");
    return;
  }
  menuModId.value = modId;
  isMenuFromKeyboard = isKeyboardOpen(event);
  menu.value?.toggle(event);
}

function onMenuShown(): void {
  if (isMenuFromKeyboard) {
    markFirstMenuItem();
  }
}
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
          <ModRowColor
            :color="mod.color"
            :groups="mod.groups"
            part="stripe"
          />
          <div
            v-if="selectable"
            role="gridcell"
            class="contents"
          >
            <Checkbox
              class="shrink-0"
              binary
              :model-value="isSelected(mod.modId)"
              :data-testid="`mod-row-select-${mod.modId}`"
              @click.stop="onCheckboxClick($event, mod.modId)"
              @dblclick.stop
              @update:model-value="emit('toggleSelect', mod.modId)"
            />
          </div>
          <div
            role="gridcell"
            class="contents"
          >
            <ModRowPosition
              v-if="showPosition && mod.position !== null"
              :mod-id="mod.modId"
              :position="mod.position"
            />
            <ModRowColor
              :color="mod.color"
              :groups="mod.groups"
              part="name"
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
            <ModRowGroups
              :groups="mod.groups"
              @scope="(groupId: number) => emit('scopeGroup', groupId)"
            />
            <span
              v-if="showUpdated"
              class="text-text-muted w-20 min-w-0 truncate text-right text-xs @lg:w-28 @lg:shrink-0"
              :title="updatedText(mod)"
              :data-testid="`mod-row-updated-${mod.modId}`"
            >{{ updatedText(mod) }}</span>
            <ModRowScanFacts
              :state="mod.state"
              :show-dependents="showActiveColumns"
            />
          </div>
          <div
            v-if="hasRowMenu"
            role="gridcell"
            class="contents"
          >
            <ModRowMenuButton
              :mod-id="mod.modId"
              :name="modLabel.label(mod.modId)"
              :is-direct="isDirectActivate"
              @open="onRowMenuClick($event, mod.modId)"
            />
          </div>
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
      v-if="hasRowMenu"
      ref="moveMenu"
      popup
      :model="menuItems"
      data-testid="mod-move-menu"
      @show="onMenuShown"
    >
      <template
        v-if="hasOrganizationActions"
        #start
      >
        <div
          role="group"
          class="flex flex-wrap items-center gap-1 px-3 pt-2 pb-1"
          :aria-label="t('mods.colors.swatchesLabel')"
          data-testid="mod-menu-swatches"
        >
          <button
            v-for="color in COLOR_TOKENS"
            :key="color"
            type="button"
            class="focus-visible:outline-accent size-5 cursor-pointer rounded-full focus-visible:outline-2"
            :class="[
              COLOR_SWATCH_CLASS[color],
              menuModColor === color ? 'ring-text ring-2 ring-offset-1' : '',
            ]"
            :aria-label="t('mods.colors.swatch', { name: t(COLOR_NAME_KEY[color]) })"
            :title="t(COLOR_NAME_KEY[color])"
            :aria-pressed="menuModColor === color"
            :data-testid="`mod-menu-swatch-${color}`"
            @click="pickColor(color)"
          />
          <button
            type="button"
            class="border-border-strong text-text-muted focus-visible:outline-accent flex size-5 cursor-pointer items-center justify-center rounded-full border border-dashed focus-visible:outline-2"
            :aria-label="t('mods.colors.swatchNone')"
            :title="t('mods.colors.swatchNone')"
            :aria-pressed="menuModColor === null"
            data-testid="mod-menu-swatch-none"
            @click="pickColor(null)"
          >
            <i
              class="pi pi-times text-[0.6rem]"
              aria-hidden="true"
            />
          </button>
        </div>
      </template>
      <template #item="{ item, props }">
        <a
          v-bind="props.action"
          :data-testid="item['testId']"
        >
          {{ item.label }}
        </a>
      </template>
      <template
        v-if="rowMenu === 'move' && reorderBlockedReason !== null"
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
