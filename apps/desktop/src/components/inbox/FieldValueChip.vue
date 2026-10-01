<script setup lang="ts">
import MergeValueCell from "@/components/merge/MergeValueCell.vue";
import { useModLabel } from "@/composables/useModLabel";
import { fitsMergeValueCell } from "@/utils/format";

/**
 * One "mod id, its value" unit — the small building block
 * `FieldRowsTable` repeats for every toucher's own candidate, `inGame`,
 * and `afterMerge` cell. Factored out once so those call sites (and the
 * accent ring a `Conflict` row's preferred value gets) share one
 * rendering instead of three copies of the same `MergeValueCell`/
 * `fitsMergeValueCell` fallback — the same "fits or falls back to a
 * truncated `<code>`" rule `ChangeSummary`'s own patch-collision block
 * already uses, just factored into a component here since it repeats
 * more than twice in one table.
 */
const {
  modId,
  value,
  isXml = false,
  highlighted = false,
} = defineProps<{
  modId: string;
  value: string | null;
  /** Whether `value` is an XML fragment (a `li` item or a keyed-map entry) rather than plain leaf text — see `MergeValueCell`'s own prop of the same name. */
  isXml?: boolean;
  /** Rings this chip — the row's own `preference` names `modId` as the winner. */
  highlighted?: boolean;
}>();

const modLabel = useModLabel();
</script>

<template>
  <span
    class="flex items-center gap-1 rounded px-1"
    :class="highlighted ? 'ring-accent ring-2' : ''"
  >
    <span
      class="text-text-faint text-[10px]"
      :title="modLabel.titleFor(modId)"
    >{{ modLabel.label(modId) }}</span>
    <MergeValueCell
      v-if="fitsMergeValueCell(value, isXml)"
      :value="value"
      :is-xml="isXml"
    />
    <code
      v-else
      class="bg-surface-2 max-w-40 truncate rounded px-1"
      :title="value ?? ''"
    >{{ value }}</code>
  </span>
</template>
