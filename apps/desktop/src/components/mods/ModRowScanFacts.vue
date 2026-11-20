<script setup lang="ts">
import { computed } from "vue";

import type { ModRowStateDto } from "@/types/generated/ModRowStateDto";
import { assertNever } from "@/utils/assertNever";

/**
 * A mod row's tags and, on the Active list, its Hard-dependent count, both hidden on a narrow
 * row. A scanned row shows every tag it carries; an unscanned row (every inactive mod, or one
 * activated since the last scan) shows the tags added to it by hand.
 */
const { state, showDependents } = defineProps<{
  state: ModRowStateDto;
  /** Whether the Hard-dependent count column shows (the Active list only). */
  showDependents: boolean;
}>();

/** A row's tags: a scanned row's own, an unscanned row's manual ones. */
const tags = computed<readonly string[]>(() => {
  switch (state.kind) {
    case "scanned":
      return state.tags;
    case "pendingScan":
      return state.manualTags;
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
    data-testid="mod-row-tags"
  >
    <span
      v-for="tag in tags"
      :key="tag"
      class="bg-surface-2 text-text-muted shrink-0 rounded px-1.5 py-0.5 text-xs"
    >{{ tag }}</span>
  </span>
  <span
    v-if="showDependents"
    class="text-text-muted hidden w-10 shrink-0 text-right text-xs @md:inline"
  >{{ hardDependents ?? "—" }}</span>
</template>
