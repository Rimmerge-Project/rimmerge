<script setup lang="ts">
import { useVirtualizer, type VirtualItem } from "@tanstack/vue-virtual";
import Checkbox from "primevue/checkbox";
import { computed, nextTick, useTemplateRef } from "vue";
import { useI18n } from "vue-i18n";

import { useModLabel } from "@/composables/useModLabel";
import type { ModSummaryDto } from "@/types/generated/ModSummaryDto";

const {
  mods,
  selectable = false,
  showActiveColumns = true,
} = defineProps<{
  mods: ModSummaryDto[];
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
}>();
const emit = defineEmits<{ selectMod: [modId: string]; focusMod: [modId: string] }>();
/** Checked ids, only meaningful while {@link selectable} is set. */
const selectedIds = defineModel<string[]>("selectedIds", { default: () => [] });
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
  mod: ModSummaryDto;
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

/** Moves roving focus by `delta` rows (±1), or to the very first/last row when `toEnd` is given — scrolling the target into view first when it isn't currently rendered. */
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
  virtualizer.value.scrollToIndex(target);
  lastFocusedId.value = targetMod.modId;
  // The target row may not exist in the DOM yet (virtualization) until
  // the scroll above re-renders `visibleRows` — wait for Vue to flush
  // that render before focusing it.
  await nextTick();
  const element = scrollElementRef.value?.querySelector<HTMLElement>(
    `[data-testid="mod-row-${targetMod.modId}"]`,
  );
  element?.focus();
  emit("focusMod", targetMod.modId);
}

function onRowKeydown(event: KeyboardEvent, modId: string): void {
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
</script>

<template>
  <div
    ref="scrollElement"
    class="surface-card min-h-0 flex-1 overflow-y-auto"
    role="table"
    :aria-label="t('mods.table.regionLabel')"
    data-testid="mod-table"
  >
    <span
      class="sr-only"
      data-testid="mod-table-roving-hint"
    >{{ t("mods.table.rovingHint") }}</span>
    <div :style="{ height: `${totalSize}px`, position: 'relative', width: '100%' }">
      <div
        v-for="{ virtualRow, mod } in visibleRows"
        :key="mod.modId"
        role="row"
        :tabindex="rovingIndex === null ? (virtualRow.index === 0 ? 0 : -1) : virtualRow.index === rovingIndex ? 0 : -1"
        :aria-current="mod.modId === lastFocusedId ? 'true' : undefined"
        class="border-border-subtle hover:bg-surface-2 focus-visible:bg-accent-soft absolute top-0 left-0 flex w-full cursor-pointer items-center gap-3 border-b px-3 text-sm focus-visible:outline"
        :style="{
          height: `${virtualRow.size}px`,
          transform: `translateY(${virtualRow.start}px)`,
        }"
        :data-testid="`mod-row-${mod.modId}`"
        @click="openRow(mod.modId)"
        @keydown="onRowKeydown($event, mod.modId)"
        @focus="onRowFocus(mod.modId)"
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
        <span
          class="text-text flex-1 truncate font-medium"
          :title="modLabel.titleFor(mod.modId)"
        >{{ modLabel.label(mod.modId) }}</span>
        <span
          v-if="mod.missing"
          class="bg-status-danger-soft text-status-danger shrink-0 rounded px-1.5 py-0.5 text-xs font-medium"
          data-testid="mod-row-missing-pill"
        >{{ t("mods.table.missing") }}</span>
        <span class="text-text-muted w-20 text-xs">{{ mod.source ?? "—" }}</span>
        <template v-if="showActiveColumns">
          <span class="flex gap-1">
            <span
              v-for="tag in mod.tags"
              :key="tag"
              class="bg-surface-2 text-text-muted rounded px-1.5 py-0.5 text-xs"
            >{{ tag }}</span>
          </span>
          <span class="text-text-muted w-10 text-right text-xs">{{ mod.hardDependents }}</span>
        </template>
      </div>
    </div>
  </div>
</template>
