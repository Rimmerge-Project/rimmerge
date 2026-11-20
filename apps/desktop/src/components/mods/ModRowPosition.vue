<script setup lang="ts">
import { useI18n } from "vue-i18n";

/**
 * A mod row's load-order position, shown while the Active list is sorted by anything but load
 * order: the visible `#n` is decoration, the screen-reader text names it.
 */
const { modId, position } = defineProps<{
  modId: string;
  /** The row's 0-based place in the working set. */
  position: number;
}>();

const { t } = useI18n();
</script>

<template>
  <span
    class="text-text-muted w-10 shrink-0 text-right text-xs tabular-nums"
    aria-hidden="true"
    :data-testid="`mod-row-position-${modId}`"
  >#{{ position + 1 }}</span>
  <span class="sr-only">{{ t("mods.table.loadOrderPosition", { position: position + 1 }) }}</span>
</template>
