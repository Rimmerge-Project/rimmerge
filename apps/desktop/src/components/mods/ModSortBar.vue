<script setup lang="ts">
import { useI18n } from "vue-i18n";

import { useTranslateMessage } from "@/composables/useTranslateMessage";
import type { ListSortDto } from "@/types/generated/ListSortDto";
import type { ListSortKeyDto } from "@/types/generated/ListSortKeyDto";
import type { SortDirectionDto } from "@/types/generated/SortDirectionDto";
import { sortButtonLabel, sortKeyLabel } from "@/utils/modListSort";

/**
 * The display-sort controls of one Mods list: per criterion, a pair of arrow
 * buttons (ascending, descending). Each is a native toggle button, so Tab,
 * Enter and Space work; the active one says so through `aria-pressed` and a
 * filled style with a thicker border, never colour alone. Display only: choosing a sort sends no
 * mutation.
 */
const { keys } = defineProps<{
  /** The criteria this list offers, in display order. */
  keys: readonly ListSortKeyDto[];
}>();
const sort = defineModel<ListSortDto>("sort", { required: true });

const { t } = useI18n();
const tm = useTranslateMessage();

const DIRECTIONS: readonly SortDirectionDto[] = ["ascending", "descending"];
const ARROW_ICON: Readonly<Record<SortDirectionDto, string>> = {
  ascending: "pi-arrow-up",
  descending: "pi-arrow-down",
};

function isActive(key: ListSortKeyDto, direction: SortDirectionDto): boolean {
  return sort.value.key === key && sort.value.direction === direction;
}

function choose(key: ListSortKeyDto, direction: SortDirectionDto): void {
  sort.value = { key, direction };
}
</script>

<template>
  <div
    role="group"
    class="flex shrink-0 flex-wrap items-center gap-x-4 gap-y-1 text-sm"
    :aria-label="t('mods.sort.groupLabel')"
    data-testid="mods-sort-bar"
  >
    <span class="text-text-muted">{{ t("mods.sort.heading") }}</span>
    <span
      v-for="key in keys"
      :key="key"
      class="flex items-center gap-1"
    >
      <span class="text-text">{{ tm(sortKeyLabel(key)) }}</span>
      <button
        v-for="direction in DIRECTIONS"
        :key="direction"
        type="button"
        class="border-border hover:bg-surface-2 focus-visible:outline-accent inline-flex size-7 items-center justify-center rounded border focus-visible:outline-2"
        :class="isActive(key, direction) ? 'bg-accent-soft text-accent border-accent border-2 font-semibold' : 'text-text-muted'"
        :aria-pressed="isActive(key, direction)"
        :aria-label="tm(sortButtonLabel(key, direction))"
        :title="tm(sortButtonLabel(key, direction))"
        :data-testid="`mods-sort-${key}-${direction}`"
        @click="choose(key, direction)"
      >
        <i
          :class="['pi', ARROW_ICON[direction]]"
          aria-hidden="true"
        />
      </button>
    </span>
  </div>
</template>
