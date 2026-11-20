<script setup lang="ts">
import { useI18n } from "vue-i18n";

import type { ModRowStateDto } from "@/types/generated/ModRowStateDto";

/**
 * A mod row's state pill: "missing" always, "not scanned yet" only where the active columns
 * show; nothing for a scanned row.
 */
const { state, showPendingScan } = defineProps<{
  state: ModRowStateDto;
  /** Whether a pending-scan row says so (off for the Inactive list, like the active columns). */
  showPendingScan: boolean;
}>();

const { t } = useI18n();
</script>

<template>
  <span
    v-if="state.kind === 'missing'"
    class="bg-status-danger-soft text-status-danger shrink-0 rounded px-1.5 py-0.5 text-xs font-medium"
    data-testid="mod-row-missing-pill"
  >{{ t("mods.table.missing") }}</span>
  <span
    v-else-if="showPendingScan && state.kind === 'pendingScan'"
    role="img"
    class="bg-surface-2 text-text-muted inline-flex shrink-0 items-center rounded px-1.5 py-0.5 text-xs font-medium"
    :aria-label="t('mods.table.pendingScan')"
    :title="t('mods.table.pendingScan')"
    data-testid="mod-row-pending-scan-pill"
  >
    <!-- One accessible name on this wrapper, as SourceBadge does. Narrow, the words give way to an icon; a keyboard user has no hover tooltip, so the state reaches them through this name and the pending-changes banner. -->
    <span
      class="hidden @md:inline"
      aria-hidden="true"
    >{{ t("mods.table.pendingScan") }}</span>
    <span
      class="@md:hidden"
      aria-hidden="true"
    ><span class="pi pi-clock" /></span>
  </span>
</template>
