<script setup lang="ts">
import { useVirtualizer, type VirtualItem } from "@tanstack/vue-virtual";
import Button from "primevue/button";
import Checkbox from "primevue/checkbox";
import Menu, { type MenuMethods } from "primevue/menu";
import type { MenuItem } from "primevue/menuitem";
import { computed, nextTick, ref, useTemplateRef, watch } from "vue";
import { useI18n } from "vue-i18n";

import SourceBadge from "@/components/mods/SourceBadge.vue";
import { isInert } from "@/composables/shortcutTargets";
import { useDragAutoScroll } from "@/composables/useDragAutoScroll";
import { useModLabel } from "@/composables/useModLabel";
import { useTranslateMessage } from "@/composables/useTranslateMessage";
import { descriptor } from "@/i18n/messageDescriptor";
import type { ModListRowDto } from "@/types/generated/ModListRowDto";
import { assertNever } from "@/utils/assertNever";
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

/** The roving-tabindex row — only this one is in the Tab order at a time. `null` before anything has been focused (every row then reads `tabindex="0"`, so the first Tab press lands on the first one). */
const rovingIndex = computed<number | null>(() => {
  const index = mods.findIndex((mod) => mod.modId === lastFocusedId.value);
  return index >= 0 ? index : null;
});
const lastFocusedId = defineModel<string | null>("focusedId", { default: null });

/** A row's tags; only a scanned row has any. */
function tagsOf(mod: ModListRowDto): readonly string[] {
  const state = mod.state;
  switch (state.kind) {
    case "scanned":
      return state.tags;
    case "pendingScan":
    case "missing":
      return [];
    default:
      return assertNever(state);
  }
}

/** A row's Hard-dependent count; `null` while no scan facts exist for it. */
function hardDependentsOf(mod: ModListRowDto): number | null {
  const state = mod.state;
  switch (state.kind) {
    case "scanned":
      return state.hardDependents;
    case "pendingScan":
    case "missing":
      return null;
    default:
      return assertNever(state);
  }
}

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

/**
 * Tracks the roving-tabindex target only — never emits `focusMod`. A
 * plain click already fires `openRow`'s own immediate `selectMod`
 * synchronously, and a click also natively focuses the row (it has a
 * tabindex), so this same DOM `focus` event fires right alongside that
 * click every time. Emitting `focusMod` here too would arm the caller's
 * debounced auto-follow (`ModsPage.vue`'s `followFocus`) on every
 * click, racing it against whatever explicit navigation the click (or
 * something the user does moments later, e.g. inside the now-open
 * panel) already triggered — a stale timer could fire mid-flight and
 * cancel that explicit navigation. Only {@link moveFocus} (pure
 * keyboard arrow/Home/End movement, with no immediate navigation of its
 * own) emits `focusMod`.
 */
function onRowFocus(modId: string): void {
  lastFocusedId.value = modId;
}

/** Moves roving focus by `delta` rows (±1), or to the very first/last row when `toEnd` is given. */
async function moveFocus(delta: number, toEnd?: "start" | "end"): Promise<void> {
  if (mods.length === 0) {
    return;
  }
  const current = rovingIndex.value ?? 0;
  const target =
    toEnd === "start"
      ? 0
      : toEnd === "end"
        ? mods.length - 1
        : Math.min(Math.max(current + delta, 0), mods.length - 1);
  const targetMod = mods[target];
  if (!targetMod) {
    return;
  }
  await focusRow(targetMod.modId);
  emit("focusMod", targetMod.modId);
}

/**
 * Scrolls the row for `modId` into view and focuses it, whether or not it is rendered yet.
 * A row that is not in the list is ignored.
 */
async function focusRow(modId: string): Promise<void> {
  const index = mods.findIndex((mod) => mod.modId === modId);
  if (index === -1) {
    return;
  }
  virtualizer.value.scrollToIndex(index);
  lastFocusedId.value = modId;
  // The target row may not exist in the DOM yet (virtualization) until
  // the scroll above re-renders `visibleRows` — wait for Vue to flush
  // that render before focusing it.
  await nextTick();
  const element = scrollElementRef.value?.querySelector<HTMLElement>(
    `[data-testid="mod-row-${modId}"]`,
  );
  element?.focus();
}
defineExpose({ focusRow });

/**
 * Alt+ArrowUp/Down moves the row (the Active list only). Text entry and dialogs keep their own
 * keys (`isInert`); plain arrows stay the roving focus. Returns whether the key was consumed.
 */
function onMoveKey(event: KeyboardEvent, modId: string): boolean {
  if (!event.altKey || (event.key !== "ArrowUp" && event.key !== "ArrowDown")) {
    return false;
  }
  if (isInert(event.target)) {
    return true;
  }
  event.preventDefault();
  if (rowMenu === "move") {
    emit("moveStep", modId, event.key === "ArrowUp" ? "up" : "down");
  }
  return true;
}

/**
 * Shift+F10 or the ContextMenu key on a focused row opens that row's menu, the keyboard route to
 * the ⋮ button (which is out of the Tab order, so rows stay one tab stop each). The same inert
 * rules as the other row shortcuts apply. Returns whether the key was consumed.
 */
function onMenuKey(event: KeyboardEvent, modId: string): boolean {
  const isMenuKey = event.key === "ContextMenu" || (event.key === "F10" && event.shiftKey);
  if (!isMenuKey || rowMenu === "none" || isInert(event.target)) {
    return false;
  }
  // Without this the browser's own context menu opens over ours.
  event.preventDefault();
  onRowMenuClick(event, modId);
  return true;
}

function onRowKeydown(event: KeyboardEvent, modId: string): void {
  if (onMoveKey(event, modId) || onMenuKey(event, modId)) {
    return;
  }
  switch (event.key) {
    case "Enter":
      openRow(modId);
      break;
    case " ":
      if (selectable) {
        event.preventDefault();
        toggleSelected(modId);
      }
      break;
    case "ArrowDown":
      event.preventDefault();
      void moveFocus(1);
      break;
    case "ArrowUp":
      event.preventDefault();
      void moveFocus(-1);
      break;
    case "Home":
      event.preventDefault();
      void moveFocus(0, "start");
      break;
    case "End":
      event.preventDefault();
      void moveFocus(0, "end");
      break;
    default:
      break;
  }
}

const draggingSet = computed(() => new Set(draggingIds));
const autoScroll = useDragAutoScroll(() => scrollElementRef.value);
/** The gap a position drop would land in (a line shows it); `null` when nothing hovers. */
const dragSlot = ref<number | null>(null);
const isHoveringPane = ref(false);
const isHoveringRefused = ref(false);

function clearHover(): void {
  dragSlot.value = null;
  isHoveringPane.value = false;
  isHoveringRefused.value = false;
  autoScroll.stop();
}

watch(
  () => dropMode,
  (mode) => {
    if (mode === "off") {
      clearHover();
    }
  },
);

function onRowDragStart(event: DragEvent, modId: string): void {
  if (!draggable) {
    event.preventDefault();
    return;
  }
  if (event.dataTransfer) {
    // Some engines refuse to start a drag with no data.
    event.dataTransfer.setData("text/plain", modId);
    event.dataTransfer.effectAllowed = "move";
  }
  emit("dragStart", modId);
}

function onRowDragEnd(): void {
  clearHover();
  emit("dragEnd");
}

/** The gap nearest the pointer: the row's top half is the gap before it, the bottom half after. */
function slotAt(event: DragEvent): number {
  const scroller = scrollElementRef.value;
  if (!scroller) {
    return mods.length;
  }
  const offset = event.clientY - scroller.getBoundingClientRect().top + scroller.scrollTop;
  return Math.min(Math.max(Math.round(offset / ROW_HEIGHT_PX), 0), mods.length);
}

function onDragOver(event: DragEvent): void {
  if (dropMode === "off") {
    return;
  }
  if (dropMode === "refused") {
    // Not cancelling the event leaves the drop refused and shows the not-allowed cursor.
    if (event.dataTransfer) {
      event.dataTransfer.dropEffect = "none";
    }
    isHoveringRefused.value = true;
    return;
  }
  event.preventDefault();
  if (event.dataTransfer) {
    event.dataTransfer.dropEffect = "move";
  }
  autoScroll.update(event);
  if (dropMode === "position") {
    dragSlot.value = slotAt(event);
  } else {
    isHoveringPane.value = true;
  }
}

function onDragLeave(event: DragEvent): void {
  const next = event.relatedTarget;
  if (next instanceof Node && scrollElementRef.value?.contains(next)) {
    return;
  }
  clearHover();
}

function onDrop(event: DragEvent): void {
  if (dropMode !== "position" && dropMode !== "pane") {
    return;
  }
  event.preventDefault();
  const slot = dropMode === "position" ? slotAt(event) : mods.length;
  clearHover();
  emit("drop", slot);
}

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

/** The row button's name and tooltip. */
function rowMenuLabel(modId: string): string {
  const name = modLabel.label(modId);
  return rowMenu === "move"
    ? t("mods.move.menuButton", { name })
    : t("mods.move.activateAtButton", { name });
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
          class="border-border-subtle hover:bg-surface-2 focus-visible:bg-accent-soft absolute top-0 left-0 flex w-full cursor-pointer items-center gap-3 border-b px-3 text-sm focus-visible:outline"
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
          <template v-if="showPosition && mod.position !== null">
            <span
              class="text-text-muted w-10 shrink-0 text-right text-xs tabular-nums"
              aria-hidden="true"
              :data-testid="`mod-row-position-${mod.modId}`"
            >#{{ mod.position + 1 }}</span>
            <span class="sr-only">{{ t("mods.table.loadOrderPosition", { position: mod.position + 1 }) }}</span>
          </template>
          <SourceBadge :source="mod.source" />
          <span
            class="text-text flex-1 truncate font-medium"
            :title="modLabel.titleFor(mod.modId)"
          >{{ modLabel.label(mod.modId) }}</span>
          <span
            v-if="mod.state.kind === 'missing'"
            class="bg-status-danger-soft text-status-danger shrink-0 rounded px-1.5 py-0.5 text-xs font-medium"
            data-testid="mod-row-missing-pill"
          >{{ t("mods.table.missing") }}</span>
          <span
            v-else-if="showActiveColumns && mod.state.kind === 'pendingScan'"
            class="bg-surface-2 text-text-muted shrink-0 rounded px-1.5 py-0.5 text-xs font-medium"
            data-testid="mod-row-pending-scan-pill"
          >{{ t("mods.table.pendingScan") }}</span>
          <span
            v-if="showUpdated"
            class="text-text-muted w-28 shrink-0 truncate text-right text-xs"
            :title="updatedText(mod)"
            :data-testid="`mod-row-updated-${mod.modId}`"
          >{{ updatedText(mod) }}</span>
          <template v-if="showActiveColumns">
            <span class="flex gap-1">
              <span
                v-for="tag in tagsOf(mod)"
                :key="tag"
                class="bg-surface-2 text-text-muted rounded px-1.5 py-0.5 text-xs"
              >{{ tag }}</span>
            </span>
            <span class="text-text-muted w-10 text-right text-xs">{{ hardDependentsOf(mod) ?? "—" }}</span>
          </template>
          <Button
            v-if="rowMenu !== 'none'"
            class="shrink-0"
            icon="pi pi-ellipsis-v"
            severity="secondary"
            text
            rounded
            size="small"
            tabindex="-1"
            :aria-haspopup="rowMenu === 'move' ? 'menu' : 'dialog'"
            :aria-label="rowMenuLabel(mod.modId)"
            :title="rowMenuLabel(mod.modId)"
            :data-testid="`mod-row-menu-${mod.modId}`"
            @click.stop="onRowMenuClick($event, mod.modId)"
            @keydown.stop
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
