<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import ModTable from "@/components/mods/ModTable.vue";
import type { ModPageDto } from "@/types/generated/ModPageDto";
import type { ModSummaryDto } from "@/types/generated/ModSummaryDto";

const {
  modPage,
  isPending,
  error,
  showActiveColumns = true,
} = defineProps<{
  /** The query's current page; `undefined` until the first response. */
  modPage: ModPageDto | undefined;
  isPending: boolean;
  error: Error | null;
  /** Passed through to {@link ModTable}: off for the Inactive list. */
  showActiveColumns?: boolean;
}>();
const emit = defineEmits<{ selectMod: [modId: string]; focusMod: [modId: string] }>();
const selectedIds = defineModel<string[]>("selectedIds", { required: true });
defineSlots<{
  /** This list's own actions, at the end of the header row; shown in every state. */
  actions(): unknown;
}>();

const { t } = useI18n();

// The IPC layer can reject with a non-`Error` value despite the type, so
// the fallback text stays.
const errorMessage = computed(() =>
  error instanceof Error ? error.message : t("mods.page.loadFailed"),
);
/** The backend caps a page, so `total` can exceed the rows it returned. */
const isCapped = computed(() => modPage !== undefined && modPage.total > modPage.items.length);
/** The table's rows, only once the page loaded without error. */
const rows = computed<ModSummaryDto[] | null>(() =>
  isPending || error || modPage === undefined ? null : modPage.items,
);
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
      v-else-if="modPage"
      class="text-text-muted text-sm"
      data-testid="mods-total"
    >
      {{ t("mods.page.totalMods", { count: modPage.total }, modPage.total) }}
      <span
        v-if="isCapped"
        class="text-text-faint"
        data-testid="mods-page-cap-hint"
      >{{ t("mods.page.pageCapHint", { count: modPage.items.length }) }}</span>
    </p>

    <div class="ml-auto flex items-center gap-2">
      <slot name="actions" />
    </div>
  </div>
  <ModTable
    v-if="rows"
    v-model:selected-ids="selectedIds"
    :mods="rows"
    selectable
    :show-active-columns="showActiveColumns"
    @select-mod="(modId: string) => emit('selectMod', modId)"
    @focus-mod="(modId: string) => emit('focusMod', modId)"
  />
</template>
