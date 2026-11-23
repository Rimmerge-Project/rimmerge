<script setup lang="ts">
import Select from "primevue/select";
import { computed, nextTick, useId, useTemplateRef, watch } from "vue";
import { useI18n } from "vue-i18n";

import { useCollapseToFit } from "@/composables/useCollapseToFit";
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
 *
 * When the buttons at their natural width do not fit the pane (a narrow window, or a language
 * with longer words), the bar collapses to a select of the criteria plus one button for the
 * direction, so it never wraps onto a second line. The same sort model drives both forms.
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

const root = useTemplateRef<HTMLElement>("root");
const natural = useTemplateRef<HTMLElement>("natural");
const isCollapsed = useCollapseToFit(root, natural);
const headingId = useId();

// Swapping forms removes the focused control from the page; a keyboard user keeps their place by
// landing on its counterpart. The snapshot is taken before the DOM patch (flush "pre").
watch(
  isCollapsed,
  async (collapsed) => {
    const hadFocus = root.value?.contains(document.activeElement) ?? false;
    await nextTick();
    if (!hadFocus) {
      return;
    }
    const counterpart = collapsed ? "mods-sort-direction" : `mods-sort-${sort.value.key}`;
    root.value?.querySelector<HTMLElement>(`[data-testid="${counterpart}"]`)?.focus();
  },
  { flush: "pre" },
);

const criteria = computed(() => keys.map((key) => ({ key, label: tm(sortKeyLabel(key)) })));
const chosenKey = computed({
  get: () => sort.value.key,
  set: (key: ListSortKeyDto) => {
    // Picking the current criterion again must not flip it; the direction button does that.
    if (key !== sort.value.key) {
      choose(key);
    }
  },
});
const directionLabel = computed(() => tm(sortButtonLabel(sort.value, sort.value.key)));
</script>

<template>
  <div
    ref="root"
    role="group"
    class="relative flex h-8 shrink-0 items-center gap-x-2 text-sm"
    :aria-label="t('mods.sort.groupLabel')"
    data-testid="mods-sort-bar"
  >
    <!-- The expanded form is always rendered, so its natural width stays measurable; collapsed, it is only hidden, and the clip sits on this wrapper so the visible controls keep their focus rings. -->
    <div :class="isCollapsed ? 'pointer-events-none absolute inset-0 overflow-hidden' : 'contents'">
      <div
        ref="natural"
        class="flex w-max shrink-0 flex-nowrap items-center gap-x-2"
        :class="isCollapsed ? 'invisible absolute top-0 left-0' : ''"
        :inert="isCollapsed"
      >
        <span class="text-text-muted">{{ t("mods.sort.heading") }}</span>
        <button
          v-for="key in keys"
          :key="key"
          type="button"
          class="border-border hover:bg-surface-2 focus-visible:outline-accent inline-flex h-7 items-center gap-1.5 rounded border px-2 whitespace-nowrap focus-visible:outline-2"
          :class="isActive(key) ? 'bg-accent-soft text-accent border-accent border-2 font-semibold' : 'text-text'"
          :aria-label="tm(sortButtonLabel(sort, key))"
          :title="tm(sortButtonLabel(sort, key))"
          :data-testid="`mods-sort-${key}`"
          :data-active="isActive(key)"
          @click="choose(key)"
        >
          <span>{{ tm(sortKeyLabel(key)) }}</span>
          <i
            :class="['pi', iconFor(key), isActive(key) ? '' : 'opacity-50']"
            aria-hidden="true"
          />
        </button>
      </div>
    </div>
    <template v-if="isCollapsed">
      <span
        :id="headingId"
        class="text-text-muted shrink-0"
      >{{ t("mods.sort.heading") }}</span>
      <Select
        v-model="chosenKey"
        class="min-w-0 flex-1"
        size="small"
        :options="criteria"
        option-label="label"
        option-value="key"
        :aria-labelledby="headingId"
        data-testid="mods-sort-select"
      />
      <button
        type="button"
        class="border-border hover:bg-surface-2 text-accent focus-visible:outline-accent inline-flex h-7 shrink-0 items-center rounded border px-2 focus-visible:outline-2"
        :aria-label="directionLabel"
        :title="directionLabel"
        data-testid="mods-sort-direction"
        @click="choose(sort.key)"
      >
        <i
          :class="['pi', iconFor(sort.key)]"
          aria-hidden="true"
        />
      </button>
    </template>
  </div>
</template>
