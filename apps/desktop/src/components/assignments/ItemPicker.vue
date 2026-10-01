<script setup lang="ts">
import { refDebounced } from "@vueuse/core";
import InputText from "primevue/inputtext";
import { computed, provide, ref, useId, useTemplateRef, watch } from "vue";
import { useI18n } from "vue-i18n";

import DefThumbnail from "@/components/graphics/DefThumbnail.vue";
import { SCROLL_CONTAINER_KEY } from "@/composables/scrollContainer";
import { usePagedRows } from "@/composables/usePagedRows";
import { useAssignmentItemsQuery } from "@/queries/assignments";
import { defRefOf } from "@/types/brands";
import type { CardinalityDto } from "@/types/generated/CardinalityDto";

/**
 * An `ItemSlot` field's own picker: a searchable, paged list of every
 * active def of the slot's item type (`list_assignment_items`), with a
 * checkbox per item, listing each item's owner and hint. Renders a
 * single-select (radio) list instead when `cardinality` is `"scalar"` —
 * `rim-merge`'s `render_leaf` only ever emits a `Cardinality::Scalar`
 * `ItemSlot`
 * for exactly one name, silently dropping the field as a `SkippedField`
 * otherwise, so this picker must make a second name unrepresentable
 * rather than merely discouraged.
 */
const {
  defType,
  selected,
  cardinality,
  assignmentId = null,
} = defineProps<{
  /** The slot's own item type. */
  defType: string;
  /** The currently chosen item names. */
  selected: string[];
  /** Whether this slot holds at most one name (`"scalar"`) or several (`"list"`). */
  cardinality: CardinalityDto;
  /**
   * The assignment project this picker belongs to, when it has one —
   * its own free-standing rows of `defType` (if it has a section for
   * one) come back `own: true` and are grouped first under a "This
   * project" heading. `null`
   * for a picker with no owning project in scope (there are none today,
   * but the prop stays optional rather than forcing every future caller
   * to thread one through).
   */
  assignmentId?: string | null;
}>();
const emit = defineEmits<{ "update:selected": [names: string[]] }>();
const { t } = useI18n();
// This list scrolls on its own, so its thumbnails measure "near the viewport" against it.
const listElement = useTemplateRef<HTMLElement>("list");
provide(SCROLL_CONTAINER_KEY, listElement);

const isScalar = computed(() => cardinality === "scalar");
/** Unique per instance so two scalar pickers on the same page never group their radios together. */
const radioGroupId = useId();

const search = ref("");
const debouncedSearch = refDebounced(search, 300);
const offset = ref(0);
const limit = 20;

const filter = computed(() => ({
  search: debouncedSearch.value.length > 0 ? debouncedSearch.value : null,
  offset: offset.value,
  limit,
}));

const { data } = useAssignmentItemsQuery(
  () => defType,
  filter,
  () => assignmentId,
);

const resetSource = computed(() => debouncedSearch.value);
const { rows, total, hasMore, loadMore } = usePagedRows(offset, data, resetSource);

// `defType` itself changing (a different slot's picker instance is
// vanishingly rare — pickers are keyed per field — but reset defensively
// all the same) also starts the list over.
watch(
  () => defType,
  () => {
    offset.value = 0;
  },
);

const selectedSet = computed(() => new Set(selected));

/**
 * "This project"'s own rows first, grouped by `ListItemDto.own` — never
 * inferred from `owner` (a real active def's owner happens to never
 * equal the project's own package id, but that's incidental, not the
 * contract; see that DTO's own doc comment). The server already orders
 * own rows ahead of the active list's matches within one page
 * (`list_assignment_items`), so this is a stable partition, not a sort.
 */
const ownRows = computed(() => rows.value.filter((item) => item.own));
const otherRows = computed(() => rows.value.filter((item) => !item.own));

function toggle(name: string): void {
  const next = new Set(selectedSet.value);
  if (next.has(name)) {
    next.delete(name);
  } else {
    next.add(name);
  }
  emit("update:selected", [...next]);
}

/** Scalar mode's own handler: picking a name always *replaces* the selection, never adds to it. */
function selectOnly(name: string): void {
  emit("update:selected", [name]);
}

function clearSelection(): void {
  emit("update:selected", []);
}
</script>

<template>
  <div
    class="flex flex-col gap-1"
    data-testid="item-picker"
  >
    <InputText
      v-model="search"
      :placeholder="t('assignments.itemPicker.searchPlaceholder')"
      size="small"
      :data-testid="`item-picker-search-${defType}`"
    />
    <ul
      ref="list"
      class="border-border-subtle bg-surface-1 flex max-h-48 flex-col gap-0.5 overflow-y-auto rounded border p-1"
      :data-testid="`item-picker-list-${defType}`"
    >
      <template v-if="ownRows.length > 0">
        <li
          class="text-text-faint px-2 pt-1 text-[10px] font-semibold tracking-wide uppercase"
          :data-testid="`item-picker-own-heading-${defType}`"
        >
          {{ t("assignments.itemPicker.thisProjectHeading") }}
        </li>
        <li
          v-for="item in ownRows"
          :key="item.def.defName"
        >
          <label class="hover:bg-surface-2 flex items-center gap-2 rounded px-2 py-1 text-xs">
            <input
              v-if="isScalar"
              type="radio"
              :name="radioGroupId"
              :checked="selectedSet.has(item.def.defName)"
              :data-testid="`item-picker-checkbox-${defType}-${item.def.defName}`"
              @change="selectOnly(item.def.defName)"
            >
            <input
              v-else
              type="checkbox"
              :checked="selectedSet.has(item.def.defName)"
              :data-testid="`item-picker-checkbox-${defType}-${item.def.defName}`"
              @change="toggle(item.def.defName)"
            >
            <span class="flex-1">{{ item.def.defName }}</span>
            <span
              v-if="item.hint"
              class="text-text-faint"
            >{{ item.hint }}</span>
          </label>
        </li>
      </template>
      <li
        v-for="item in otherRows"
        :key="item.def.defName"
      >
        <label class="hover:bg-surface-2 flex items-center gap-2 rounded px-2 py-1 text-xs">
          <input
            v-if="isScalar"
            type="radio"
            :name="radioGroupId"
            :checked="selectedSet.has(item.def.defName)"
            :data-testid="`item-picker-checkbox-${defType}-${item.def.defName}`"
            @change="selectOnly(item.def.defName)"
          >
          <input
            v-else
            type="checkbox"
            :checked="selectedSet.has(item.def.defName)"
            :data-testid="`item-picker-checkbox-${defType}-${item.def.defName}`"
            @change="toggle(item.def.defName)"
          >
          <DefThumbnail
            class="h-6 w-6 shrink-0"
            :def-ref="defRefOf(item.def.defType, item.def.defName)"
          />
          <span class="flex-1">{{ item.def.defName }}</span>
          <span
            v-if="item.hint"
            class="text-text-faint"
          >{{ item.hint }}</span>
        </label>
      </li>
      <li
        v-if="rows.length === 0"
        class="text-text-faint px-2 py-1 text-xs"
      >
        {{ t("assignments.itemPicker.noMatches") }}
      </li>
    </ul>
    <div class="flex items-center justify-between text-xs">
      <span class="text-text-faint">{{
        t("assignments.itemPicker.count", { loaded: rows.length, total })
      }}</span>
      <button
        v-if="isScalar && selected.length > 0"
        type="button"
        class="text-accent cursor-pointer hover:underline"
        :data-testid="`item-picker-clear-${defType}`"
        @click="clearSelection"
      >
        {{ t("assignments.itemPicker.clearSelection") }}
      </button>
      <button
        v-if="hasMore"
        type="button"
        class="text-accent cursor-pointer hover:underline"
        :data-testid="`item-picker-load-more-${defType}`"
        @click="loadMore"
      >
        {{ t("assignments.itemPicker.loadMore") }}
      </button>
    </div>
  </div>
</template>
