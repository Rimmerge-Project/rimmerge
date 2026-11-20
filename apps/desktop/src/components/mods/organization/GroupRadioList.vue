<script setup lang="ts">
import { useId } from "vue";
import { useI18n } from "vue-i18n";

import type { GroupNodeDto } from "@/types/generated/GroupNodeDto";
import { COLOR_NAME_KEY, COLOR_SWATCH_CLASS } from "@/utils/organization";

/**
 * Every group of a tree as one radio, indented by depth, each with its colour dot (decoration
 * only: the name is the label) and, when `counts` is given, how many of the selected mods it
 * holds. A group in `disabledReasons` is listed disabled with its reason written out. The legend
 * is shown as the list's visible heading when `isLegendVisible` is set (it is always the radio
 * group's accessible name).
 */
const {
  nodes,
  legend,
  name,
  counts = null,
  disabledReasons = new Map<number, string>(),
  isLegendVisible = false,
} = defineProps<{
  nodes: readonly GroupNodeDto[];
  /** Names the group of radios for assistive technology. */
  legend: string;
  /** The radio group's form name; two lists on one dialog need two. */
  name: string;
  /** Selected mods held directly per group id; omit to show no counts. */
  counts?: ReadonlyMap<number, number> | null;
  /** Why a group cannot be chosen, by group id. */
  disabledReasons?: ReadonlyMap<number, string>;
  /** Show the legend as a heading instead of only naming the group for assistive technology. */
  isLegendVisible?: boolean;
}>();
const picked = defineModel<number | null>({ required: true });

const { t } = useI18n();
const legendId = useId();

function heldBy(id: number): number {
  return counts?.get(id) ?? 0;
}
</script>

<template>
  <fieldset
    role="radiogroup"
    :aria-labelledby="legendId"
    class="border-border-subtle bg-surface-1 flex max-h-56 flex-col gap-0.5 overflow-y-auto rounded border p-1"
  >
    <legend
      :id="legendId"
      :class="isLegendVisible ? 'px-1 text-sm font-medium' : 'sr-only'"
    >
      {{ legend }}
    </legend>
    <label
      v-for="node in nodes"
      :key="node.id"
      class="flex items-center gap-2 rounded px-2 py-1"
      :class="
        disabledReasons.has(node.id) ? 'text-text-faint' : 'hover:bg-surface-2 cursor-pointer'
      "
      :style="{ paddingInlineStart: `${(node.depth - 1) * 14 + 8}px` }"
    >
      <input
        v-model="picked"
        type="radio"
        :name="name"
        :value="node.id"
        :disabled="disabledReasons.has(node.id)"
        :data-testid="`${name}-${node.id}`"
      >
      <span
        v-if="node.color !== null"
        class="size-3 shrink-0 rounded-full"
        :class="COLOR_SWATCH_CLASS[node.color]"
        :title="t(COLOR_NAME_KEY[node.color])"
        aria-hidden="true"
      />
      <span class="truncate">{{ node.name }}</span>
      <span
        v-if="heldBy(node.id) > 0"
        class="text-text-muted text-xs"
        :data-testid="`${name}-count-${node.id}`"
      >{{ t("mods.bulk.dialog.holdsCount", { count: heldBy(node.id) }, heldBy(node.id)) }}</span>
      <span
        v-if="disabledReasons.has(node.id)"
        class="text-xs"
        :data-testid="`${name}-reason-${node.id}`"
      >{{ disabledReasons.get(node.id) }}</span>
    </label>
  </fieldset>
</template>
