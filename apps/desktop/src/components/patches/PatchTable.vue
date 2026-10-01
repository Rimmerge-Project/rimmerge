<script setup lang="ts">
import { useI18n } from "vue-i18n";

import BaseEmpty from "@/components/base/BaseEmpty.vue";
import type { PatchSummaryDto } from "@/types/generated/PatchSummaryDto";

const { patches, activeIndex } = defineProps<{
  patches: PatchSummaryDto[];
  /**
   * The keyboard cursor's row, owned by `PatchListPage`'s own `j`/`k`
   * state and rendered here (`aria-current` plus an accent background) —
   * never tracked locally, so the page's cursor and this table's
   * highlight can never disagree. An out-of-range value (e.g. `-1`)
   * highlights nothing.
   */
  activeIndex: number;
}>();
const emit = defineEmits<{
  open: [patchId: string];
  /**
   * A row received real DOM focus (Tab, or the moment a click lands on
   * it) — `PatchListPage` uses this to keep its own cursor in sync with
   * whichever row is actually focused. `Enter` is handled once, at the
   * page's own `window` keydown listener, never here: an `@keydown.enter`
   * on the row itself would fire *in addition to* that listener (the
   * event still bubbles to `window`), double-firing `open`.
   */
  focusRow: [index: number];
}>();
const { t } = useI18n();
</script>

<template>
  <BaseEmpty
    v-if="patches.length === 0"
    :message="t('patches.table.emptyState')"
  />
  <div
    v-else
    class="surface-card overflow-hidden"
  >
    <table class="w-full border-collapse text-sm">
      <thead>
        <tr class="table-head text-left">
          <th
            scope="col"
            class="px-3 py-2 font-medium"
          >
            {{ t("patches.table.nameHeader") }}
          </th>
          <th
            scope="col"
            class="px-3 py-2 font-medium"
          >
            {{ t("patches.table.packageIdHeader") }}
          </th>
          <th
            scope="col"
            class="px-3 py-2 font-medium"
          >
            {{ t("patches.table.scopeHeader") }}
          </th>
          <th
            scope="col"
            class="px-3 py-2 text-right font-medium"
          >
            {{ t("patches.table.needsInputHeader") }}
          </th>
          <th
            scope="col"
            class="px-3 py-2 text-right font-medium"
          >
            {{ t("patches.table.completeHeader") }}
          </th>
          <th
            scope="col"
            class="px-3 py-2 font-medium"
          >
            {{ t("patches.table.lastExportHeader") }}
          </th>
        </tr>
      </thead>
      <tbody>
        <tr
          v-for="(patch, index) in patches"
          :key="patch.id"
          tabindex="0"
          class="border-border-subtle hover:bg-surface-2 focus-visible:bg-accent-soft cursor-pointer border-b focus-visible:outline"
          :class="index === activeIndex ? 'bg-accent-soft' : ''"
          :aria-current="index === activeIndex"
          :data-testid="`patch-row-${patch.id}`"
          @click="emit('open', patch.id)"
          @focus="emit('focusRow', index)"
        >
          <td class="text-text px-3 py-2 font-medium">
            {{ patch.name }}
          </td>
          <td class="text-text-muted px-3 py-2 font-mono text-xs">
            {{ patch.packageId }}
          </td>
          <td class="px-3 py-2">
            <span class="flex flex-wrap gap-1">
              <span
                v-for="member in patch.scope"
                :key="member.modId"
                class="bg-surface-2 text-text-muted rounded-full px-2 py-0.5 text-xs"
              >{{ member.name }}</span>
            </span>
          </td>
          <td class="text-text px-3 py-2 text-right tabular-nums">
            {{ patch.needsInputCount }}
          </td>
          <td class="text-text px-3 py-2 text-right tabular-nums">
            {{ patch.completeCount }}
          </td>
          <td class="text-text-muted px-3 py-2 text-xs">
            {{ patch.exportDir ?? t("patches.table.notExported") }}
          </td>
        </tr>
      </tbody>
    </table>
  </div>
</template>
