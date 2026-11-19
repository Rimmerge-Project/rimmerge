<script setup lang="ts">
import { useI18n } from "vue-i18n";

import { useTranslateMessage } from "@/composables/useTranslateMessage";
import type { ListSortDto } from "@/types/generated/ListSortDto";
import type { ListSortKeyDto } from "@/types/generated/ListSortKeyDto";
import { nextSort, sortButtonLabel, sortKeyLabel } from "@/utils/modListSort";

/**
 * The display-sort controls of one Mods list: one toggle button per criterion.
 * Clicking the active criterion flips its direction; clicking another selects it
 * ascending. Each is a native button, so Tab, Enter and Space work. There is no
 * `aria-pressed`: the button never un-presses and its name changes, which is not
 * the toggle pattern. The active one says so in its accessible name and title
 * ("Sorted by name, A to Z. Click for Z to A."), and visually by a filled style, a
 * thicker border and an arrow showing its direction, never colour alone. Display
 * only: choosing a sort sends no mutation.
 */
const { keys } = defineProps<{
  /** The criteria this list offers, in display order. */
  keys: readonly ListSortKeyDto[];
}>();
const sort = defineModel<ListSortDto>("sort", { required: true });

const { t } = useI18n();
const tm = useTranslateMessage();

function isActive(key: ListSortKeyDto): boolean {
  return sort.value.key === key;
}

function iconFor(key: ListSortKeyDto): string {
  if (!isActive(key)) {
    return "pi-sort-alt";
  }
  return sort.value.direction === "ascending" ? "pi-arrow-up" : "pi-arrow-down";
}

function choose(key: ListSortKeyDto): void {
  sort.value = nextSort(sort.value, key);
}
</script>

<template>
  <div
    role="group"
    class="flex shrink-0 flex-wrap items-center gap-x-2 gap-y-1 text-sm"
    :aria-label="t('mods.sort.groupLabel')"
    data-testid="mods-sort-bar"
  >
    <span class="text-text-muted">{{ t("mods.sort.heading") }}</span>
    <button
      v-for="key in keys"
      :key="key"
      type="button"
      class="border-border hover:bg-surface-2 focus-visible:outline-accent inline-flex h-7 items-center gap-1.5 rounded border px-2 focus-visible:outline-2"
      :class="isActive(key) ? 'bg-accent-soft text-accent border-accent border-2 font-semibold' : 'text-text'"
      :aria-label="tm(sortButtonLabel(sort, key))"
      :title="tm(sortButtonLabel(sort, key))"
      :data-testid="`mods-sort-${key}`"
      @click="choose(key)"
    >
      <span>{{ tm(sortKeyLabel(key)) }}</span>
      <i
        :class="['pi', iconFor(key), isActive(key) ? '' : 'opacity-50']"
        aria-hidden="true"
      />
    </button>
  </div>
</template>
