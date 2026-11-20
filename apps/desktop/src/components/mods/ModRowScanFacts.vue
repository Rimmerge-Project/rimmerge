<script setup lang="ts">
import { computed } from "vue";

import type { ModRowStateDto } from "@/types/generated/ModRowStateDto";
import { assertNever } from "@/utils/assertNever";

/**
 * A mod row's scan facts for the Active list: its tags and its Hard-dependent count, both hidden
 * on a narrow row.
 */
const { state } = defineProps<{ state: ModRowStateDto }>();

/** A row's tags; only a scanned row has any. */
const tags = computed<readonly string[]>(() => {
  switch (state.kind) {
    case "scanned":
      return state.tags;
    case "pendingScan":
    case "missing":
      return [];
    default:
      return assertNever(state);
  }
});

/** A row's Hard-dependent count; `null` while no scan facts exist for it. */
const hardDependents = computed<number | null>(() => {
  switch (state.kind) {
    case "scanned":
      return state.hardDependents;
    case "pendingScan":
    case "missing":
      return null;
    default:
      return assertNever(state);
  }
});
</script>

<template>
  <span
    v-if="tags.length > 0"
    class="hidden min-w-0 gap-1 overflow-hidden @md:flex"
  >
    <span
      v-for="tag in tags"
      :key="tag"
      class="bg-surface-2 text-text-muted shrink-0 rounded px-1.5 py-0.5 text-xs"
    >{{ tag }}</span>
  </span>
  <span class="text-text-muted hidden w-10 shrink-0 text-right text-xs @md:inline">{{ hardDependents ?? "—" }}</span>
</template>
