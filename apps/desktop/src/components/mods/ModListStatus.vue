<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import type { ModListDto } from "@/types/generated/ModListDto";

/**
 * A Mods list's status line: loading, the load error, or the mod count (and whether the backend
 * capped the rows), with the list's own actions at its end in every state.
 */
const { modList, isPending, error } = defineProps<{
  /** The query's current list; `undefined` until the first response. */
  modList: ModListDto | undefined;
  isPending: boolean;
  error: Error | null;
}>();
defineSlots<{
  /** The list's own actions, at the end of the line. */
  actions(): unknown;
}>();

const { t } = useI18n();

// The IPC layer can reject with a non-`Error` value despite the type, so
// the fallback text stays.
const errorMessage = computed(() =>
  error instanceof Error ? error.message : t("mods.page.loadFailed"),
);
/** The backend bounds a list, so `total` can exceed the rows it returned. */
const isCapped = computed(() => modList !== undefined && modList.total > modList.items.length);
</script>

<template>
  <div class="flex shrink-0 flex-wrap items-center gap-2">
    <p
      v-if="isPending"
      class="text-text-muted text-sm"
      data-testid="mods-loading"
    >
      {{ t("common.loading") }}
    </p>
    <p
      v-else-if="error"
      class="text-status-danger text-sm"
      data-testid="mods-error"
    >
      {{ errorMessage }}
    </p>
    <p
      v-else-if="modList"
      class="text-text-muted text-sm"
      data-testid="mods-total"
    >
      {{ t("mods.page.totalMods", { count: modList.total }, modList.total) }}
      <span
        v-if="isCapped"
        class="text-text-faint"
        data-testid="mods-page-cap-hint"
      >{{ t("mods.page.pageCapHint", { count: modList.items.length }) }}</span>
    </p>

    <div class="ml-auto flex items-center gap-2">
      <slot name="actions" />
    </div>
  </div>
</template>
